package md.vibecodium.mobile

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap

/**
 * Counts are split so the UI can show how many files exist on each side and
 * how many of them actually need a transfer. [total] is the transfer plan,
 * not the number of paths scanned.
 */
data class SyncProgress(
    val phase: String,
    val done: Int = 0,
    val total: Int = 0,
    val localFiles: Int = 0,
    val remoteFiles: Int = 0,
    val planned: Int = 0,
    /**
     * Which counter [done]/[total] belongs to. Preparation totals (files
     * scanned, hashes checked, plan size) are different units, so the UI must
     * label them separately instead of showing one restarting sync number.
     */
    val kind: SyncProgressKind = SyncProgressKind.Preparing,
)

data class SyncOutcome(
    val uploaded: Int,
    val downloaded: Int,
    val deletedLocal: Int,
    val deletedRemote: Int,
    val conflicts: Int,
    val skipped: Int,
    val lastSync: Long,
)

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

    /** Stable key for the remote cursor: a changed filter requires a full list. */
    fun fingerprint(): String = buildString {
        append("images=").append(allowImages)
        append(";audio=").append(allowAudio)
        append(";video=").append(allowVideo)
        append(";pdf=").append(allowPdf)
        append(";other=").append(allowOther)
        append(";excluded=")
        excludedFolders
            .map { it.replace('\\', '/').trim('/') }
            .filter { it.isNotEmpty() }
            .sorted()
            .joinTo(this, "|")
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

class SyncAbortedException(message: String) : Exception(message)

/**
 * Provider-neutral Dropbox sync over [VaultRepository]. Works for both the
 * file vault (JNI core) and a SAF tree.
 *
 * Policy follows the desktop engine's decisions:
 * - the remote side is read through a persisted Dropbox cursor and a delta;
 * - a local file is hashed only when its size or modified stamp changed;
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
    private val bytesCache = ConcurrentHashMap<String, ByteArray>()
    private val hashCache = ConcurrentHashMap<String, String>()

    suspend fun sync(remoteRoot: String, localSnapshot: List<VaultFile>? = null): SyncOutcome {
        // A pass must see the current disk; a previous pass may have cached
        // bytes before an external edit.
        bytesCache.clear()
        hashCache.clear()

        progress(SyncProgress("Scanning local files"))
        val localFiles = (localSnapshot ?: repository.snapshot(refresh = true).files)
            .filterNot { isServicePath(it.path) }
            .filter { filter.accepts(it.path) }
            .associateBy { it.path }
        progress(SyncProgress("Scanning local files", localFiles = localFiles.size))

        val existing = records.all(vaultKey).filterKeys { filter.accepts(it) }
        val previous = records.state(vaultKey)
        val scope = filter.fingerprint()
        // An upload whose response lacked metadata leaves an empty rev. A full
        // listing repairs those records before a delta could hide them.
        val needsRepair = existing.values.any { it.remoteRev.isBlank() }
        val delta = if (
            !needsRepair &&
            previous != null &&
            previous.remoteRoot == remoteRoot &&
            previous.scope == scope &&
            previous.cursor.isNotBlank()
        ) {
            progress(SyncProgress("Checking Dropbox", localFiles = localFiles.size))
            provider.delta(remoteRoot, previous.cursor)
        } else {
            null
        }

        val view: RemoteView
        val nextCursor: String
        if (delta != null) {
            val removed = delta.removed.toSet()
            val changed = delta.changed
                .filter { filter.accepts(it.path) && !removedUnder(it.path, removed) }
                .associateBy { it.path }
            view = RemoteView.Delta(changed, removed)
            nextCursor = delta.cursor
        } else {
            progress(SyncProgress("Listing Dropbox", localFiles = localFiles.size))
            val cursor = provider.latestCursor(remoteRoot)
            val listed = provider.listFolder(remoteRoot)
                .filterNot { isServicePath(it.path) }
            // Compare the raw listing: a filter that excludes every category
            // must not look like a deleted remote folder.
            if (listed.isEmpty() && existing.isNotEmpty()) {
                throw SyncAbortedException(
                    "The Dropbox folder is empty or missing while local sync records exist",
                )
            }
            val remoteFiles = listed
                .filter { filter.accepts(it.path) }
                .associateBy { it.path }
            view = RemoteView.Full(remoteFiles)
            nextCursor = cursor
        }

        val remoteFiles = view.count(existing)
        progress(
            SyncProgress(
                phase = "Checking local changes",
                localFiles = localFiles.size,
                remoteFiles = remoteFiles,
            ),
        )

        // Hash same-size files whose modified stamp is missing or different.
        // The reads run on a small IO pool, so a vault without reliable mtimes
        // is verified in parallel instead of one file at a time.
        val hashCandidates = localFiles.keys.filter { path ->
            val local = localFiles[path]
            val record = existing[path]
            local != null && record != null && local.size == record.localSize && !sameStamp(local, record)
        }
        if (hashCandidates.isNotEmpty()) {
            var checked = 0
            for (chunk in hashCandidates.chunked(HASH_CONCURRENCY)) {
                coroutineScope {
                    chunk.map { path -> async(Dispatchers.IO) { hashOnly(path) } }.awaitAll()
                }
                checked += chunk.size
                progress(
                    SyncProgress(
                        phase = "Reading local files",
                        done = checked,
                        total = hashCandidates.size,
                        localFiles = localFiles.size,
                        remoteFiles = remoteFiles,
                        kind = SyncProgressKind.ReadingLocal,
                    ),
                )
            }
        }

        val paths = (localFiles.keys + view.paths() + existing.keys)
            .filter { filter.accepts(it) }
            .toSortedSet()
        val decisions = ArrayList<Decision>(paths.size)
        for (path in paths) {
            val local = localFiles[path]
            val record = existing[path]
            decisions.add(decide(path, local, view.get(path, record), record))
        }
        val plan = decisions.filter { it !is Decision.Noop && it !is Decision.Forget }

        val localDeletes = plan.count { it is Decision.DeleteLocal }
        val remoteDeletes = plan.count { it is Decision.DeleteRemote }
        if (localFiles.size >= 10 && localDeletes > localFiles.size / 2) {
            throw SyncAbortedException(
                "Refusing to delete $localDeletes local files: more than half of ${localFiles.size}",
            )
        }
        if (existing.size >= 10 && remoteDeletes > existing.size / 2) {
            throw SyncAbortedException(
                "Refusing to delete $remoteDeletes remote files: more than half of ${existing.size}",
            )
        }

        progress(
            SyncProgress(
                phase = "Planning",
                localFiles = localFiles.size,
                remoteFiles = remoteFiles,
                planned = plan.size,
            ),
        )

        var uploaded = 0
        var downloaded = 0
        var deletedLocal = 0
        var deletedRemote = 0
        var conflicts = 0
        var skipped = 0
        val usedPaths = paths.toMutableSet()
        var done = 0

        for (decision in plan) {
            done += 1
            progress(
                SyncProgress(
                    phase = "Syncing cloud files",
                    done = done,
                    total = plan.size,
                    localFiles = localFiles.size,
                    remoteFiles = remoteFiles,
                    planned = plan.size,
                    kind = SyncProgressKind.Transferring,
                ),
            )
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
                        val target = remotePath(remoteRoot, decision.path)
                        val entry = uploadWithMetadata(target, bytes, decision.rev)
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
                    val remote = decision.remote
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
                    val remote = provider.download(remotePath(remoteRoot, decision.remote.path))
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
        records.saveState(
            vaultKey,
            SyncState(
                lastSync = now,
                remoteRoot = remoteRoot,
                cursor = nextCursor,
                scope = scope,
            ),
        )
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
                    record == null -> Decision.Download(path, remote)
                    remoteChanged(remote, record) -> Decision.Download(path, remote)
                    else -> Decision.DeleteRemote(path)
                }
            }

            local != null && remote == null -> {
                when {
                    record == null -> Decision.Upload(path, null)
                    // An incomplete upload record is not a deletion baseline;
                    // re-upload instead of deleting the local file.
                    record.remoteRev.isBlank() -> Decision.Upload(path, null)
                    localChanged(local, record) -> Decision.Upload(path, null)
                    else -> Decision.DeleteLocal(path)
                }
            }

            local != null && remote != null -> {
                val localDiffers = record?.let { localChanged(local, it) } ?: true
                val remoteDiffers = record?.let { remoteChanged(remote, it) } ?: true
                when {
                    !localDiffers && !remoteDiffers -> Decision.Noop(path)
                    localDiffers && !remoteDiffers -> Decision.Upload(path, remote.rev)
                    !localDiffers && remoteDiffers -> Decision.Download(path, remote)
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

                    else -> Decision.Conflict(path, remote)
                }
            }

            else -> Decision.Skip(path)
        }
    }

    private suspend fun localChanged(local: VaultFile, record: SyncRecord): Boolean {
        if (local.size != record.localSize) return true
        if (sameStamp(local, record)) return false
        val hash = hash(local.path) ?: return true
        return hash != record.localHash
    }

    /**
     * A missing modified stamp (0, common for some SAF providers) cannot prove
     * that a file is unchanged, so those files are hashed instead.
     */
    private fun sameStamp(local: VaultFile, record: SyncRecord): Boolean =
        local.lastModified != 0L &&
            record.localModified != 0L &&
            local.lastModified == record.localModified

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
        val conflictEntry = uploadWithMetadata(conflictRemotePath, remote.bytes, null)
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

        val updated = uploadWithMetadata(
            remotePath(remoteRoot, path),
            localBytes,
            remote.rev,
        )
        records.put(vaultKey, record(local, localHash, updated.rev, updated.contentHash))
    }

    /**
     * Some upload responses arrive without the metadata header. Asking
     * `files/get_metadata` keeps the baseline complete so the next delta does
     * not turn a normal edit into a conflict.
     */
    private suspend fun uploadWithMetadata(
        path: String,
        bytes: ByteArray,
        rev: String?,
    ): RemoteEntry {
        val entry = provider.upload(path, bytes, rev)
        if (entry.rev.isNotBlank() && entry.contentHash.isNotBlank()) return entry
        val repaired = provider.metadata(path) ?: return entry
        return entry.copy(
            rev = entry.rev.ifBlank { repaired.rev },
            contentHash = entry.contentHash.ifBlank { repaired.contentHash },
            size = if (entry.size > 0) entry.size else repaired.size,
        )
    }

    private suspend fun bytes(path: String): ByteArray? =
        bytesCache[path] ?: repository.readBytes(path)?.also { bytesCache[path] = it }

    private suspend fun hashOnly(path: String): String? {
        hashCache[path]?.let { return it }
        val bytes = repository.readBytes(path) ?: return null
        val value = sha256(bytes)
        hashCache[path] = value
        return value
    }

    private suspend fun hash(path: String): String? = hashCache[path] ?: hashOnly(path)

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
        data class Download(val path: String, val remote: RemoteEntry) : Decision
        data class DeleteLocal(val path: String) : Decision
        data class DeleteRemote(val path: String) : Decision
        data class Conflict(val path: String, val remote: RemoteEntry) : Decision
        data class Forget(val path: String) : Decision
        data class Noop(val path: String) : Decision
        data class Skip(val path: String) : Decision
    }

    private sealed interface RemoteView {
        fun paths(): Set<String>
        fun get(path: String, record: SyncRecord?): RemoteEntry?
        fun count(records: Map<String, SyncRecord>): Int

        class Full(private val files: Map<String, RemoteEntry>) : RemoteView {
            override fun paths(): Set<String> = files.keys
            override fun get(path: String, record: SyncRecord?): RemoteEntry? = files[path]
            override fun count(records: Map<String, SyncRecord>): Int = files.size
        }

        class Delta(
            private val changed: Map<String, RemoteEntry>,
            private val removed: Set<String>,
        ) : RemoteView {
            override fun paths(): Set<String> = changed.keys

            override fun get(path: String, record: SyncRecord?): RemoteEntry? {
                changed[path]?.let { return it }
                if (removedUnder(path, removed)) return null
                return record?.let { RemoteEntry(path, it.remoteRev, it.remoteHash, 0L) }
            }

            override fun count(records: Map<String, SyncRecord>): Int {
                var total = changed.size
                for (path in records.keys) {
                    if (path !in changed && !removedUnder(path, removed)) total += 1
                }
                return total
            }
        }
    }

    private companion object {
        const val MAX_SYNC_FILE = 10L * 1024 * 1024
        const val HASH_CONCURRENCY = 4

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

private fun removedUnder(path: String, removed: Set<String>): Boolean {
    if (removed.isEmpty()) return false
    if (path in removed) return true
    var index = path.indexOf('/')
    while (index >= 0) {
        if (path.substring(0, index) in removed) return true
        index = path.indexOf('/', index + 1)
    }
    return false
}
