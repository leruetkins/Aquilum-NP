package md.vibecodium.mobile

import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

data class SyncProgress(val phase: String, val done: Int, val total: Int)

/** Selective-sync policy: excluded folders and file families. Notes always sync. */
data class SyncFilter(
    val allowImages: Boolean = true,
    val allowAudio: Boolean = true,
    val allowVideo: Boolean = true,
    val allowPdf: Boolean = true,
    val allowOther: Boolean = true,
    val excludedFolders: List<String> = emptyList(),
) {
    fun accepts(path: String): Boolean {
        val normalized = path.replace('\\', '/').trim('/')
        if (normalized.isEmpty()) return false
        if (isExcluded(normalized)) return false
        if (isMarkdownName(normalized)) return true
        val extension = normalized.substringAfterLast('/').substringAfterLast('.', "").lowercase()
        return when (extension) {
            in IMAGE_EXTENSIONS -> allowImages
            in AUDIO_EXTENSIONS -> allowAudio
            in VIDEO_EXTENSIONS -> allowVideo
            "pdf" -> allowPdf
            else -> allowOther
        }
    }

    private fun isExcluded(path: String): Boolean {
        for (folder in excludedFolders) {
            val clean = folder.replace('\\', '/').trim('/')
            if (clean.isEmpty()) continue
            if (path == clean || path.startsWith("$clean/")) return true
        }
        return false
    }

    companion object {
        private val IMAGE_EXTENSIONS =
            setOf("png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "heic", "heif")
        private val AUDIO_EXTENSIONS =
            setOf("mp3", "wav", "flac", "ogg", "oga", "m4a", "aac", "opus", "wma")
        private val VIDEO_EXTENSIONS =
            setOf("mp4", "mov", "webm", "mkv", "avi", "m4v", "3gp", "ogv")
    }
}

data class SyncOutcome(
    val uploaded: Int,
    val downloaded: Int,
    val deletedLocal: Int,
    val deletedRemote: Int,
    val conflicts: Int,
    val skipped: Int,
    val lastSync: Long,
)

class SyncAbortedException(message: String) : Exception(message)

/**
 * Provider-neutral Dropbox sync over [VaultRepository]. Works for both the
 * file vault (JNI core) and a SAF tree.
 *
 * Policy follows the desktop engine's decisions:
 * - new local -> upload, new remote -> download;
 * - one side changed -> push/pull;
 * - both changed -> conflict copy (the local version stays at the original
 *   path, the remote version is preserved next to it and on Dropbox);
 * - deletions propagate only while the other side is unchanged;
 * - more than half of 10+ files deleting is refused.
 */
class SyncEngine(
    private val repository: VaultRepository,
    private val vaultKey: String,
    private val records: SyncRecords,
    private val provider: SyncProvider,
    private val filter: SyncFilter = SyncFilter(),
    private val progress: suspend (SyncProgress) -> Unit = {},
) {
    private val bytesCache = mutableMapOf<String, ByteArray>()
    private val hashCache = mutableMapOf<String, String>()

    suspend fun sync(remoteRoot: String): SyncOutcome {
        // A pass must see the current disk; a previous pass may have cached
        // bytes before an external edit.
        bytesCache.clear()
        hashCache.clear()
        progress(SyncProgress("Scanning local files", 0, 0))
        val localFiles = repository.listFiles()
            .filterNot { isServicePath(it.path) }
            .filter { filter.accepts(it.path) }
            .associateBy { it.path }
        progress(SyncProgress("Listing Dropbox", 0, 0))
        provider.ensureFolder(remoteRoot)
        val remoteFiles = provider.listFolder(remoteRoot)
            .filterNot { isServicePath(it.path) }
            .filter { filter.accepts(it.path) }
            .associateBy { it.path }
        val existing = records.all(vaultKey).filterKeys { filter.accepts(it) }
        val paths = (localFiles.keys + remoteFiles.keys + existing.keys).toSortedSet()
        val usedPaths = paths.toMutableSet()

        val decisions = mutableListOf<Decision>()
        for (path in paths) {
            decisions.add(
                decide(
                    path = path,
                    local = localFiles[path],
                    remote = remoteFiles[path],
                    record = existing[path],
                ),
            )
        }

        val localDeletes = decisions.count { it is Decision.DeleteLocal }
        val remoteDeletes = decisions.count { it is Decision.DeleteRemote }
        if (localFiles.size >= 10 && localDeletes > localFiles.size / 2) {
            throw SyncAbortedException(
                "Refusing to delete $localDeletes local files: more than half of ${localFiles.size}",
            )
        }
        if (remoteFiles.size >= 10 && remoteDeletes > remoteFiles.size / 2) {
            throw SyncAbortedException(
                "Refusing to delete $remoteDeletes remote files: more than half of ${remoteFiles.size}",
            )
        }

        var uploaded = 0
        var downloaded = 0
        var deletedLocal = 0
        var deletedRemote = 0
        var conflicts = 0
        var skipped = 0
        var done = 0

        for (decision in decisions) {
            done += 1
            progress(SyncProgress("Syncing", done, decisions.size))
            when (decision) {
                is Decision.Forget -> records.remove(vaultKey, decision.path)
                is Decision.Noop -> Unit
                is Decision.Skip -> skipped += 1
                is Decision.DeleteLocal -> {
                    repository.deleteFile(decision.path)
                    records.remove(vaultKey, decision.path)
                    deletedLocal += 1
                }

                is Decision.DeleteRemote -> {
                    provider.delete(remotePath(remoteRoot, decision.path))
                    records.remove(vaultKey, decision.path)
                    deletedRemote += 1
                }

                is Decision.Upload -> {
                    val local = localFiles[decision.path] ?: continue
                    if (local.size > MAX_SYNC_FILE) {
                        skipped += 1
                        continue
                    }
                    val bytes = bytes(local.path)
                    if (bytes == null) {
                        skipped += 1
                        continue
                    }
                    val hash = sha256(bytes)
                    try {
                        val entry = provider.upload(
                            remotePath(remoteRoot, decision.path),
                            bytes,
                            decision.rev,
                        )
                        records.put(
                            vaultKey,
                            record(local, hash, entry.rev, entry.contentHash),
                        )
                        uploaded += 1
                    } catch (error: DropboxException) {
                        if (error.status == 409 && error.summary.contains("conflict")) {
                            val remote = provider.download(remotePath(remoteRoot, local.path))
                            resolveConflict(
                                remoteRoot = remoteRoot,
                                path = local.path,
                                local = local,
                                localBytes = bytes,
                                localHash = hash,
                                remote = remote,
                                usedPaths = usedPaths,
                            )
                            conflicts += 1
                        } else {
                            throw error
                        }
                    }
                }

                is Decision.Download -> {
                    val remote = remoteFiles[decision.path] ?: continue
                    if (remote.size > MAX_SYNC_FILE) {
                        skipped += 1
                        continue
                    }
                    val download = provider.download(remotePath(remoteRoot, remote.path))
                    repository.writeBytes(remote.path, download.bytes)
                    records.put(
                        vaultKey,
                        SyncRecord(
                            path = remote.path,
                            localHash = sha256(download.bytes),
                            localSize = download.bytes.size.toLong(),
                            localModified = System.currentTimeMillis(),
                            remoteRev = download.rev,
                            remoteHash = download.contentHash,
                            updatedAt = System.currentTimeMillis(),
                        ),
                    )
                    downloaded += 1
                }

                is Decision.Conflict -> {
                    val local = localFiles[decision.path] ?: continue
                    if (local.size > MAX_SYNC_FILE) {
                        skipped += 1
                        continue
                    }
                    val bytes = bytes(local.path)
                    if (bytes == null) {
                        skipped += 1
                        continue
                    }
                    val hash = sha256(bytes)
                    val remote = provider.download(remotePath(remoteRoot, local.path))
                    resolveConflict(
                        remoteRoot = remoteRoot,
                        path = local.path,
                        local = local,
                        localBytes = bytes,
                        localHash = hash,
                        remote = remote,
                        usedPaths = usedPaths,
                    )
                    conflicts += 1
                }
            }
        }

        val now = System.currentTimeMillis()
        records.saveState(vaultKey, SyncState(now, remoteRoot))
        return SyncOutcome(
            uploaded = uploaded,
            downloaded = downloaded,
            deletedLocal = deletedLocal,
            deletedRemote = deletedRemote,
            conflicts = conflicts,
            skipped = skipped,
            lastSync = now,
        )
    }

    private suspend fun decide(
        path: String,
        local: VaultFile?,
        remote: RemoteEntry?,
        record: SyncRecord?,
    ): Decision {
        return when {
            local == null && remote == null -> Decision.Forget(path)

            local == null && remote != null -> {
                when {
                    record == null -> Decision.Download(path)
                    remoteChanged(remote, record) -> Decision.Download(path)
                    else -> Decision.DeleteRemote(path)
                }
            }

            local != null && remote == null -> {
                when {
                    record == null -> Decision.Upload(path, null)
                    localChanged(local, record) -> Decision.Upload(path, null)
                    else -> Decision.DeleteLocal(path)
                }
            }

            local != null && remote != null -> {
                val localChanged = record?.let { localChanged(local, it) } ?: true
                val remoteChanged = record?.let { remoteChanged(remote, it) } ?: true
                when {
                    !localChanged && !remoteChanged -> Decision.Noop(path)
                    localChanged && !remoteChanged -> Decision.Upload(path, remote.rev)
                    !localChanged && remoteChanged -> Decision.Download(path)
                    sameContent(local, remote) -> {
                        val bytes = bytes(local.path)
                        if (bytes != null) {
                            records.put(
                                vaultKey,
                                record(local, sha256(bytes), remote.rev, remote.contentHash),
                            )
                        }
                        Decision.Noop(path)
                    }

                    else -> Decision.Conflict(path)
                }
            }

            else -> Decision.Skip(path)
        }
    }

    private suspend fun localChanged(local: VaultFile, record: SyncRecord): Boolean {
        val sameSize = local.size == record.localSize
        // An unknown mtime (0, common for some SAF providers) cannot prove the
        // file is unchanged; hash it instead of trusting the size alone.
        val sameModified = local.lastModified != 0L &&
            record.localModified != 0L &&
            local.lastModified == record.localModified
        if (sameSize && sameModified) return false
        val hash = hash(local.path) ?: return true
        return hash != record.localHash
    }

    private fun remoteChanged(remote: RemoteEntry, record: SyncRecord): Boolean =
        remote.rev != record.remoteRev

    private suspend fun sameContent(local: VaultFile, remote: RemoteEntry): Boolean {
        if (remote.contentHash.isBlank()) return false
        val bytes = bytes(local.path) ?: return false
        return dropboxContentHash(bytes) == remote.contentHash
    }

    private suspend fun resolveConflict(
        remoteRoot: String,
        path: String,
        local: VaultFile,
        localBytes: ByteArray,
        localHash: String,
        remote: RemoteDownload,
        usedPaths: MutableSet<String>,
    ) {
        val conflictPath = uniqueConflictPath(path, usedPaths)
        usedPaths.add(conflictPath)
        repository.writeBytes(conflictPath, remote.bytes)
        val conflictRemotePath = remotePath(remoteRoot, conflictPath)
        val conflictEntry = provider.upload(conflictRemotePath, remote.bytes, null)
        records.put(
            vaultKey,
            SyncRecord(
                path = conflictPath,
                localHash = sha256(remote.bytes),
                localSize = remote.bytes.size.toLong(),
                localModified = System.currentTimeMillis(),
                remoteRev = conflictEntry.rev,
                remoteHash = conflictEntry.contentHash,
                updatedAt = System.currentTimeMillis(),
            ),
        )

        val updated = provider.upload(
            remotePath(remoteRoot, path),
            localBytes,
            remote.rev,
        )
        records.put(vaultKey, record(local, localHash, updated.rev, updated.contentHash))
    }

    private suspend fun bytes(path: String): ByteArray? =
        bytesCache[path] ?: repository.readBytes(path)?.also { bytesCache[path] = it }

    private suspend fun hash(path: String): String? =
        hashCache[path] ?: bytes(path)?.let(::sha256)?.also { hashCache[path] = it }

    private fun record(local: VaultFile, hash: String, rev: String, remoteHash: String) =
        SyncRecord(
            path = local.path,
            localHash = hash,
            localSize = local.size,
            localModified = local.lastModified,
            remoteRev = rev,
            remoteHash = remoteHash,
            updatedAt = System.currentTimeMillis(),
        )

    private fun uniqueConflictPath(path: String, used: Set<String>): String {
        val stamp = SimpleDateFormat("yyyy-MM-dd HH-mm-ss", Locale.US).format(Date())
        var counter = 0
        while (true) {
            val candidate = conflictPath(path, stamp, counter)
            if (candidate !in used) return candidate
            counter += 1
        }
    }

    private fun conflictPath(path: String, stamp: String, counter: Int): String {
        val slash = path.lastIndexOf('/')
        val directory = if (slash >= 0) path.substring(0, slash + 1) else ""
        val name = path.substring(slash + 1)
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val extension = if (dot > 0) name.substring(dot) else ""
        val suffix = if (counter == 0) "" else " ${counter + 1}"
        return "$directory$stem (conflict $stamp$suffix)$extension"
    }

    private sealed interface Decision {
        data class Upload(val path: String, val rev: String?) : Decision
        data class Download(val path: String) : Decision
        data class DeleteLocal(val path: String) : Decision
        data class DeleteRemote(val path: String) : Decision
        data class Conflict(val path: String) : Decision
        data class Forget(val path: String) : Decision
        data class Noop(val path: String) : Decision
        data class Skip(val path: String) : Decision
    }

    private companion object {
        const val MAX_SYNC_FILE = 10L * 1024 * 1024

        fun remotePath(root: String, relative: String): String =
            root.trimEnd('/') + "/" + relative.trimStart('/')

        fun sha256(bytes: ByteArray): String {
            val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
            val output = StringBuilder(digest.size * 2)
            for (byte in digest) {
                output.append("%02x".format(byte))
            }
            return output.toString()
        }

        /** Dropbox's content_hash: SHA-256 of the concatenated 4 MiB block hashes. */
        fun dropboxContentHash(bytes: ByteArray): String {
            val block = MessageDigest.getInstance("SHA-256")
            val overall = MessageDigest.getInstance("SHA-256")
            var offset = 0
            while (offset < bytes.size) {
                val length = minOf(4 * 1024 * 1024, bytes.size - offset)
                block.reset()
                block.update(bytes, offset, length)
                overall.update(block.digest())
                offset += length
            }
            val digest = overall.digest()
            val output = StringBuilder(digest.size * 2)
            for (byte in digest) {
                output.append("%02x".format(byte))
            }
            return output.toString()
        }
    }
}
