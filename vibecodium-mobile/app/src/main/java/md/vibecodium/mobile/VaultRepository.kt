package md.vibecodium.mobile

import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.provider.DocumentsContract
import androidx.documentfile.provider.DocumentFile
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.InputStream
import java.net.URLDecoder
import java.security.MessageDigest
import java.util.ArrayDeque
import java.util.concurrent.Callable
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * One regular file in the vault, relative to its root.
 *
 * [uri] is set for SAF vaults and lets the app reopen a note without resolving
 * its path segment by segment.
 */
data class VaultFile(
    val path: String,
    val size: Long,
    val lastModified: Long,
    val uri: String? = null,
)

/** One traversal of a vault: every regular file and every visible folder. */
data class VaultSnapshot(
    val files: List<VaultFile>,
    val folders: List<String>,
)

/** One vault, either a plain directory (JNI core) or a SAF tree. */
interface VaultRepository {
    val name: String
    val isScoped: Boolean

    suspend fun listNotes(): List<NoteItem>
    suspend fun readNote(path: String): String
    suspend fun writeNote(path: String, text: String)
    suspend fun createNote(path: String, text: String): NoteItem
    suspend fun createFolder(path: String)
    suspend fun deleteNote(path: String)
    suspend fun renameNote(oldPath: String, newPath: String): NoteItem
    /** Renames a folder in place; its children move with it. */
    suspend fun renameFolder(oldPath: String, newPath: String)
    suspend fun search(query: String, limit: Int): List<SearchHit>

    /**
     * Builds or refreshes the full-text index. Runs in the background after a
     * vault opens; [search] falls back to a linear scan until it completes.
     */
    suspend fun warmSearchIndex() {}

    suspend fun readAttachment(target: String, notePath: String): ByteArray?

    /** All regular files and folders in one pass; the explorer and sync share it. */
    suspend fun snapshot(refresh: Boolean = false): VaultSnapshot =
        VaultSnapshot(listFiles(), listFolders())

    /** All regular files (notes and attachments), used by the sync engine. */
    suspend fun listFiles(): List<VaultFile>
    /** All non-hidden folders, including empty ones, for the explorer tree. */
    suspend fun listFolders(): List<String> = emptyList()
    suspend fun readBytes(path: String): ByteArray?
    suspend fun writeBytes(path: String, bytes: ByteArray)
    suspend fun deleteFile(path: String)

    /** Drops any cached directory index before a live-state operation. */
    fun invalidate() = Unit

    /** Persisted SAF URI for [path], or null for plain files / unknown paths. */
    fun uriFor(path: String): String? = null

    /** Reads a persisted SAF document URI directly; null for other vault kinds. */
    suspend fun readDocument(uri: String): String? = null

    fun close()
}

sealed class VaultLocator {
    abstract val value: String

    data class Local(val path: String) : VaultLocator() {
        override val value: String get() = path
    }

    data class Scoped(val uri: String) : VaultLocator() {
        override val value: String get() = uri
    }
}

object VaultLocatorStore {
    private const val KEY = "vault_locator"

    fun save(context: Context, locator: VaultLocator) {
        val kind = if (locator is VaultLocator.Scoped) "saf" else "file"
        val raw = org.json.JSONObject()
            .put("kind", kind)
            .put("value", locator.value)
            .toString()
        prefs(context).edit().putString(KEY, raw).apply()
    }

    fun load(context: Context): VaultLocator? {
        val raw = prefs(context).getString(KEY, null) ?: return null
        return runCatching {
            val value = org.json.JSONObject(raw)
            when (value.optString("kind")) {
                "saf" -> VaultLocator.Scoped(value.getString("value"))
                "file" -> VaultLocator.Local(value.getString("value"))
                else -> null
            }
        }.getOrNull()
    }

    fun clear(context: Context) {
        prefs(context).edit().remove(KEY).apply()
    }
}

/** One vault remembered for the chooser, most recently opened first. */
data class KnownVault(
    val locator: VaultLocator,
    val name: String,
    val lastOpened: Long,
)

object VaultListStore {
    private const val KEY = "vaults:known"
    private const val LIMIT = 20

    fun load(context: Context): List<KnownVault> {
        val raw = prefs(context).getString(KEY, null) ?: return emptyList()
        return runCatching {
            val array = org.json.JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val value = array.optJSONObject(index) ?: continue
                    val locator = when (value.optString("kind")) {
                        "saf" -> VaultLocator.Scoped(value.optString("value"))
                        "file" -> VaultLocator.Local(value.optString("value"))
                        else -> null
                    } ?: continue
                    if (locator.value.isBlank()) continue
                    add(
                        KnownVault(
                            locator = locator,
                            name = value.optString("name").ifBlank {
                                locator.value.substringAfterLast('/')
                            },
                            lastOpened = value.optLong("lastOpened"),
                        ),
                    )
                }
            }.sortedByDescending { it.lastOpened }.take(LIMIT)
        }.getOrDefault(emptyList())
    }

    /** Adds or promotes [locator] after a successful open. */
    fun remember(context: Context, locator: VaultLocator, name: String): List<KnownVault> {
        val entry = KnownVault(
            locator = locator,
            name = name.ifBlank { locator.value.substringAfterLast('/') },
            lastOpened = System.currentTimeMillis(),
        )
        val updated = (listOf(entry) + load(context).filterNot { same(it.locator, locator) })
            .sortedByDescending { it.lastOpened }
            .take(LIMIT)
        save(context, updated)
        return updated
    }

    fun forget(context: Context, locator: VaultLocator): List<KnownVault> {
        val updated = load(context).filterNot { same(it.locator, locator) }
        save(context, updated)
        return updated
    }

    private fun same(left: VaultLocator, right: VaultLocator): Boolean =
        left::class == right::class && left.value == right.value

    private fun save(context: Context, vaults: List<KnownVault>) {
        val array = org.json.JSONArray()
        for (vault in vaults) {
            array.put(
                org.json.JSONObject()
                    .put("kind", if (vault.locator is VaultLocator.Scoped) "saf" else "file")
                    .put("value", vault.locator.value)
                    .put("name", vault.name)
                    .put("lastOpened", vault.lastOpened),
            )
        }
        prefs(context).edit().putString(KEY, array.toString()).apply()
    }
}

object VaultRepositories {
    const val DEMO_FOLDER = "vault"

    fun open(context: Context, locator: VaultLocator): VaultRepository = when (locator) {
        is VaultLocator.Local ->
            FileVaultRepository(File(locator.path), searchSnapshot(context, locator))
        is VaultLocator.Scoped ->
            SafVaultRepository(context, Uri.parse(locator.uri), searchSnapshot(context, locator))
    }

    fun createDemo(context: Context): Pair<VaultRepository, VaultLocator> {
        val base = context.getExternalFilesDir(null) ?: context.filesDir
        val dir = File(base, DEMO_FOLDER)
        CoreJson.data(CoreBridge.nativeCreateVault(dir.absolutePath))
        val locator = VaultLocator.Local(dir.absolutePath)
        return FileVaultRepository(dir, searchSnapshot(context, locator)) to locator
    }

    /**
     * `filesDir/search/<key>.vbix` — one snapshot per vault in the app's
     * private directory, so it is removed with the app and never synced.
     */
    private fun searchSnapshot(context: Context, locator: VaultLocator): File {
        val key = MessageDigest.getInstance("SHA-256")
            .digest(locator.value.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
            .take(24)
        return File(File(context.filesDir, "search"), "$key.vbix")
    }

    /** Persists the read/write grant attached to an OpenDocumentTree result. */
    fun takePermission(context: Context, uri: Uri) {
        runCatching {
            context.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        }
    }
}

private class FileVaultRepository(
    private val root: File,
    private val searchSnapshot: File? = null,
) : VaultRepository {
    override val name: String = root.name.ifBlank { "Vault" }
    override val isScoped: Boolean = false

    @Volatile
    private var searchIndexOrNull: SearchIndex? = null

    private fun searchIndex(): SearchIndex = searchIndexOrNull ?: synchronized(this) {
        searchIndexOrNull ?: SearchIndex(this, searchSnapshot).also { searchIndexOrNull = it }
    }

    init {
        CoreJson.data(CoreBridge.nativeOpenVault(root.absolutePath))
    }

    override suspend fun listNotes(): List<NoteItem> = withContext(Dispatchers.IO) {
        CoreJson.notes(CoreBridge.nativeListNotes())
    }

    override suspend fun readNote(path: String): String = withContext(Dispatchers.IO) {
        CoreJson.noteText(CoreBridge.nativeReadNote(path))
    }

    override suspend fun writeNote(path: String, text: String) = withContext(Dispatchers.IO) {
        CoreJson.data(CoreBridge.nativeWriteNote(path, text))
        searchIndexOrNull?.invalidate(path)
        Unit
    }

    override suspend fun createNote(path: String, text: String): NoteItem = withContext(Dispatchers.IO) {
        val value = CoreJson.data(CoreBridge.nativeCreateNote(path, text))
        noteItem(value.optString("path", path), value.optString("name", path.substringAfterLast('/')))
    }

    override suspend fun createFolder(path: String) = withContext(Dispatchers.IO) {
        val directory = File(root, path)
        if (directory.exists() && !directory.isDirectory) {
            error("Already exists: $path")
        }
        if (!directory.isDirectory && !directory.mkdirs()) {
            error("Cannot create folder: $path")
        }
        Unit
    }

    override suspend fun deleteNote(path: String) = withContext(Dispatchers.IO) {
        CoreJson.data(CoreBridge.nativeDeleteNote(path))
        Unit
    }

    override suspend fun renameNote(oldPath: String, newPath: String): NoteItem = withContext(Dispatchers.IO) {
        val value = CoreJson.data(CoreBridge.nativeRenameNote(oldPath, newPath))
        val path = value.optString("path", newPath)
        noteItem(path, path.substringAfterLast('/'))
    }

    override suspend fun renameFolder(oldPath: String, newPath: String) = withContext(Dispatchers.IO) {
        // The core rename moves a directory like any other entry; attachment
        // bookkeeping only applies to notes, and folder contents move too.
        CoreJson.data(CoreBridge.nativeRenameNote(oldPath, newPath))
        Unit
    }

    override suspend fun search(query: String, limit: Int): List<SearchHit> = withContext(Dispatchers.IO) {
        searchIndex().search(query, limit)
    }

    override suspend fun warmSearchIndex() = withContext(Dispatchers.IO) {
        // Constructing the index restores the snapshot from disk; keep that
        // off the main thread even when the caller launches from Main.
        searchIndex().refresh()
    }

    override suspend fun readAttachment(target: String, notePath: String): ByteArray? =
        withContext(Dispatchers.IO) {
            val cleaned = cleanAttachmentTarget(target) ?: return@withContext null
            val noteDir = File(root, notePath).parentFile
            val candidates = buildList {
                if (cleaned.startsWith("/")) add(File(cleaned))
                add(File(root, cleaned))
                if (noteDir != null) add(File(noteDir, cleaned))
                add(File(root, "attachments/$cleaned"))
                root.walkTopDown()
                    .maxDepth(6)
                    .firstOrNull { it.isFile && it.name == cleaned }
                    ?.let(::add)
            }
            candidates
                .firstOrNull { it.isFile && it.length() in 1..MAX_ATTACHMENT_BYTES }
                ?.readBytes()
        }

    override suspend fun snapshot(refresh: Boolean): VaultSnapshot = withContext(Dispatchers.IO) {
        val files = mutableListOf<VaultFile>()
        val folders = mutableListOf<String>()
        val pending = ArrayDeque<File>()
        pending.add(root)
        while (pending.isNotEmpty()) {
            val directory = pending.removeLast()
            val children = directory.listFiles() ?: continue
            for (child in children) {
                val name = child.name ?: continue
                if (name.startsWith(".")) continue
                val path = child.relativeTo(root).invariantSeparatorsPath
                if (isServicePath(path)) continue
                if (child.isDirectory) {
                    folders.add(path)
                    pending.add(child)
                } else if (child.isFile) {
                    files.add(VaultFile(path, child.length(), child.lastModified()))
                }
            }
        }
        VaultSnapshot(
            files = files.sortedBy { it.path.lowercase() },
            folders = folders.sortedBy { it.lowercase() },
        )
    }

    override suspend fun listFiles(): List<VaultFile> = snapshot().files

    override suspend fun listFolders(): List<String> = snapshot().folders

    override suspend fun readBytes(path: String): ByteArray? = withContext(Dispatchers.IO) {
        val file = File(root, path)
        if (file.isFile && file.length() in 0..MAX_ATTACHMENT_BYTES) file.readBytes() else null
    }

    override suspend fun writeBytes(path: String, bytes: ByteArray) = withContext(Dispatchers.IO) {
        val file = File(root, path)
        file.parentFile?.mkdirs()
        file.writeBytes(bytes)
    }

    override suspend fun deleteFile(path: String) = withContext(Dispatchers.IO) {
        val file = File(root, path)
        if (file.exists()) {
            if (!file.delete()) error("Cannot delete: $path")
        }
        Unit
    }

    override fun close() {
        searchIndexOrNull?.close()
        runCatching { CoreBridge.nativeCloseVault() }
    }
}

/**
 * SAF repository with one directory query per folder.
 *
 * `DocumentFile` 1.0.0 queries the provider separately for `name`,
 * `isDirectory`, `length` and `lastModified`; walking a large vault that way
 * costs several binder round-trips per file. Here the child cursor already
 * carries all five columns, so the whole tree is one query per directory and
 * the resulting path→document map serves every later open/read/write.
 */
private class SafVaultRepository(
    context: Context,
    treeUri: Uri,
    private val searchSnapshot: File? = null,
) : VaultRepository {
    private val resolver = context.contentResolver
    private val root: DocumentFile = DocumentFile.fromTreeUri(context, treeUri)
        ?: throw IllegalStateException("The selected folder is no longer accessible")
    private val treeBase: Uri = root.uri

    @Volatile
    private var searchIndexOrNull: SearchIndex? = null

    private fun searchIndex(): SearchIndex = searchIndexOrNull ?: synchronized(this) {
        searchIndexOrNull ?: SearchIndex(this, searchSnapshot).also { searchIndexOrNull = it }
    }

    @Volatile
    private var index: SafIndex? = null
    private val indexLock = Any()
    private val buildLock = Any()
    private var generation = 0

    @Volatile
    private var scanPoolOrNull: ExecutorService? = null

    private fun scanPool(): ExecutorService = scanPoolOrNull ?: synchronized(this) {
        scanPoolOrNull ?: Executors.newFixedThreadPool(SAF_SCAN_CONCURRENCY)
            .also { scanPoolOrNull = it }
    }

    override val name: String = root.name?.takeIf { it.isNotBlank() } ?: "Vault"
    override val isScoped: Boolean = true

    override suspend fun listNotes(): List<NoteItem> = withContext(Dispatchers.IO) {
        snapshot().files
            .filter { isMarkdownName(it.path) }
            .map { noteItem(it.path, it.path.substringAfterLast('/')) }
    }

    override suspend fun readNote(path: String): String = withContext(Dispatchers.IO) {
        val file = fileFor(path) ?: error("Not found: $path")
        readText(file.uri(path))
    }

    override suspend fun writeNote(path: String, text: String) = withContext(Dispatchers.IO) {
        val file = fileFor(path) ?: error("Not found: $path")
        writeText(file.uri(path), text)
        updateFile(path) {
            it.copy(
                size = text.toByteArray().size.toLong(),
                lastModified = System.currentTimeMillis(),
            )
        }
        searchIndexOrNull?.invalidate(path)
        Unit
    }

    override suspend fun createNote(path: String, text: String): NoteItem = withContext(Dispatchers.IO) {
        val parent = path.substringBeforeLast('/', "")
        val fileName = path.substringAfterLast('/')
        val current = ensureIndex()
        if (current.files.containsKey(path)) error("Already exists: $path")
        val parentUri = folderUri(parent, create = true) ?: error("Cannot create folder: $parent")
        val created = DocumentsContract.createDocument(resolver, parentUri, "text/markdown", fileName)
            ?: error("Cannot create: $path")
        writeText(created, text)
        val file = VaultFile(
            path = path,
            size = text.toByteArray().size.toLong(),
            lastModified = System.currentTimeMillis(),
            uri = created.toString(),
        )
        synchronized(indexLock) {
            index?.files?.put(path, file)
            index?.snapshotCache = null
        }
        noteItem(path, fileName)
    }

    override suspend fun createFolder(path: String) = withContext(Dispatchers.IO) {
        val parent = path.substringBeforeLast('/', "")
        val name = path.substringAfterLast('/')
        val current = ensureIndex()
        if (current.files.containsKey(path) || current.folders.contains(path)) {
            error("Already exists: $path")
        }
        val parentUri = folderUri(parent, create = true)
            ?: error("Cannot create folder: $parent")
        val created = DocumentsContract.createDocument(
            resolver,
            parentUri,
            DocumentsContract.Document.MIME_TYPE_DIR,
            name,
        ) ?: error("Cannot create folder: $path")
        synchronized(indexLock) {
            index?.folders?.add(path)
            index?.folderUris?.put(path, created)
            index?.snapshotCache = null
        }
    }

    override suspend fun deleteNote(path: String) = withContext(Dispatchers.IO) {
        val file = fileFor(path) ?: error("Not found: $path")
        if (!DocumentsContract.deleteDocument(resolver, file.uri(path))) {
            error("Cannot delete: $path")
        }
        removeFile(path)
    }

    override suspend fun renameNote(oldPath: String, newPath: String): NoteItem = withContext(Dispatchers.IO) {
        val oldParent = oldPath.substringBeforeLast('/', "")
        val newParent = newPath.substringBeforeLast('/', "")
        if (oldParent != newParent) error("Rename across folders is not supported")
        val file = fileFor(oldPath) ?: error("Not found: $oldPath")
        val newName = newPath.substringAfterLast('/')
        val renamed = DocumentsContract.renameDocument(resolver, file.uri(oldPath), newName)
            ?: error("Cannot rename: $oldPath")
        synchronized(indexLock) {
            index?.files?.remove(oldPath)
            index?.files?.put(newPath, file.copy(path = newPath, uri = renamed.toString()))
            index?.snapshotCache = null
        }
        noteItem(newPath, newName)
    }

    override suspend fun renameFolder(oldPath: String, newPath: String) = withContext(Dispatchers.IO) {
        val oldParent = oldPath.substringBeforeLast('/', "")
        val newParent = newPath.substringBeforeLast('/', "")
        if (oldParent != newParent) error("Rename across folders is not supported")
        val current = ensureIndex()
        if (current.files.containsKey(newPath) || current.folders.contains(newPath)) {
            error("Already exists: $newPath")
        }
        val uri = folderUri(oldPath, create = false) ?: error("Not found: $oldPath")
        DocumentsContract.renameDocument(resolver, uri, newPath.substringAfterLast('/'))
            ?: error("Cannot rename: $oldPath")
        // Some providers derive child document IDs from the folder path, so a
        // fresh scan is safer than patching every descendant URI by hand.
        synchronized(buildLock) {
            generation += 1
            index = null
        }
    }

    override suspend fun search(query: String, limit: Int): List<SearchHit> = withContext(Dispatchers.IO) {
        searchIndex().search(query, limit)
    }

    override suspend fun warmSearchIndex() = withContext(Dispatchers.IO) {
        // Constructing the index restores the snapshot from disk; keep that
        // off the main thread even when the caller launches from Main.
        searchIndex().refresh()
    }

    override suspend fun readAttachment(target: String, notePath: String): ByteArray? =
        withContext(Dispatchers.IO) {
            val cleaned = cleanAttachmentTarget(target) ?: return@withContext null
            val relative = cleaned.trimStart('/')
            val noteDir = notePath.substringBeforeLast('/', "")
            var file = attachmentFile(relative, noteDir)
            if (file == null) {
                ensureIndex(refresh = true)
                file = attachmentFile(relative, noteDir)
            }
            val found = file ?: return@withContext null
            if (found.size > MAX_ATTACHMENT_BYTES) return@withContext null
            runCatching {
                resolver.openInputStream(found.uri(found.path))?.use { it.readBounded(MAX_ATTACHMENT_BYTES) }
            }.getOrNull()
        }

    override suspend fun snapshot(refresh: Boolean): VaultSnapshot =
        withContext(Dispatchers.IO) { snapshotFrom(ensureIndex(refresh)) }

    override suspend fun listFiles(): List<VaultFile> = snapshot().files

    override suspend fun listFolders(): List<String> = snapshot().folders

    override suspend fun readBytes(path: String): ByteArray? = withContext(Dispatchers.IO) {
        val file = fileFor(path, refreshIfMissing = false) ?: return@withContext null
        if (file.size > MAX_ATTACHMENT_BYTES) return@withContext null
        runCatching {
            resolver.openInputStream(file.uri(path))?.use { it.readBounded(MAX_ATTACHMENT_BYTES) }
        }.getOrNull()
    }

    override suspend fun writeBytes(path: String, bytes: ByteArray) = withContext(Dispatchers.IO) {
        val existing = fileFor(path, refreshIfMissing = false)
        val uri = existing?.uri?.let(Uri::parse) ?: run {
            val parent = path.substringBeforeLast('/', "")
            val name = path.substringAfterLast('/')
            val parentUri = folderUri(parent, create = true) ?: error("Cannot create folder: $parent")
            DocumentsContract.createDocument(resolver, parentUri, mimeFor(name), name)
                ?: error("Cannot create: $path")
        }
        val stream = resolver.openOutputStream(uri, "wt")
            ?: resolver.openOutputStream(uri, "w")
            ?: error("Cannot write: $path")
        stream.use { it.write(bytes) }
        val file = VaultFile(path, bytes.size.toLong(), System.currentTimeMillis(), uri.toString())
        synchronized(indexLock) {
            index?.files?.put(path, file)
            index?.snapshotCache = null
        }
        Unit
    }

    override suspend fun deleteFile(path: String) = withContext(Dispatchers.IO) {
        val file = fileFor(path, refreshIfMissing = false) ?: return@withContext
        if (!DocumentsContract.deleteDocument(resolver, file.uri(path))) {
            error("Cannot delete: $path")
        }
        removeFile(path)
    }

    override fun invalidate() {
        synchronized(buildLock) {
            generation += 1
            index = null
        }
    }

    override fun uriFor(path: String): String? = synchronized(indexLock) {
        index?.files?.get(path)?.uri
    }

    override suspend fun readDocument(uri: String): String? = withContext(Dispatchers.IO) {
        runCatching { readText(Uri.parse(uri)) }.getOrNull()
    }

    override fun close() {
        searchIndexOrNull?.close()
        scanPoolOrNull?.shutdownNow()
    }

    private fun snapshotFrom(value: SafIndex): VaultSnapshot = synchronized(indexLock) {
        value.snapshotCache?.let { return it }
        val built = VaultSnapshot(
            files = value.files.values.sortedBy { it.path.lowercase() },
            folders = value.folders.sortedBy { it.lowercase() },
        )
        value.snapshotCache = built
        built
    }

    private fun ensureIndex(refresh: Boolean = false): SafIndex {
        if (refresh) {
            synchronized(buildLock) {
                generation += 1
                index = null
            }
        }
        index?.let { return it }
        val started = synchronized(buildLock) {
            index?.let { return it }
            generation
        }
        // The directory walk must not hold a lock that UI callers use for
        // `uriFor`/`persistTabs`; those stay responsive while IO scans.
        val built = buildIndex()
        synchronized(buildLock) {
            if (generation == started && index == null) {
                index = built
            }
            return index ?: built
        }
    }

    private fun buildIndex(): SafIndex {
        val result = SafIndex()
        val visited = java.util.Collections.synchronizedSet(HashSet<String>())
        // Breadth-first levels let sibling folders be queried concurrently:
        // cloud SAF providers spend most of the walk in per-folder queries.
        var level = listOf(treeBase to "")
        while (level.isNotEmpty()) {
            val tasks = level.map { (directoryUri, prefix) ->
                Callable { scanDirectory(directoryUri, prefix, visited) }
            }
            val scans = scanPool().invokeAll(tasks).map { it.get() }
            val next = ArrayList<Pair<Uri, String>>()
            for (scan in scans) {
                for (file in scan.files) result.files[file.path] = file
                result.folders.addAll(scan.folders)
                result.folderUris.putAll(scan.folderUris)
                next.addAll(scan.children)
            }
            level = next
        }
        return result
    }

    private fun scanDirectory(
        directoryUri: Uri,
        prefix: String,
        visited: MutableSet<String>,
    ): DirectoryScan {
        val directoryId = runCatching { DocumentsContract.getDocumentId(directoryUri) }.getOrNull()
            ?: return DirectoryScan()
        if (!visited.add(directoryId)) return DirectoryScan()
        val childrenUri = runCatching {
            DocumentsContract.buildChildDocumentsUriUsingTree(directoryUri, directoryId)
        }.getOrNull() ?: return DirectoryScan()
        val cursor = queryChildren(childrenUri) ?: return DirectoryScan()

        val files = ArrayList<VaultFile>()
        val folders = ArrayList<String>()
        val folderUris = HashMap<String, Uri>()
        val children = ArrayList<Pair<Uri, String>>()
        cursor.use {
            val idColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val modifiedColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            val sizeColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
            while (it.moveToNext()) {
                val name = when {
                    nameColumn >= 0 -> it.getString(nameColumn)
                    else -> null
                } ?: continue
                if (name.startsWith(".")) continue
                val id = when {
                    idColumn >= 0 -> it.getString(idColumn)
                    else -> null
                } ?: continue
                val path = if (prefix.isEmpty()) name else "$prefix/$name"
                if (isServicePath(path)) continue
                val uri = DocumentsContract.buildDocumentUriUsingTree(directoryUri, id)
                val mime = when {
                    mimeColumn >= 0 -> it.getString(mimeColumn)
                    else -> null
                }
                if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                    folders.add(path)
                    folderUris[path] = uri
                    children.add(uri to path)
                } else {
                    val modified = when {
                        modifiedColumn >= 0 && !it.isNull(modifiedColumn) -> it.getLong(modifiedColumn)
                        else -> 0L
                    }
                    val size = when {
                        sizeColumn >= 0 && !it.isNull(sizeColumn) -> it.getLong(sizeColumn)
                        else -> 0L
                    }
                    files.add(VaultFile(path, size, modified, uri.toString()))
                }
            }
        }
        return DirectoryScan(files, folders, folderUris, children)
    }

    private fun queryChildren(uri: Uri): Cursor? {
        runCatching { resolver.query(uri, CHILD_PROJECTION, null, null, null) }
            .getOrNull()
            ?.let { return it }
        return runCatching { resolver.query(uri, MINIMAL_CHILD_PROJECTION, null, null, null) }
            .getOrNull()
    }

    private fun fileFor(path: String, refreshIfMissing: Boolean = true): VaultFile? {
        synchronized(indexLock) {
            index?.files?.get(path)?.let { return it }
        }
        if (!refreshIfMissing) return null
        val refreshed = ensureIndex(refresh = true)
        return synchronized(indexLock) { refreshed.files[path] }
    }

    private fun updateFile(path: String, transform: (VaultFile) -> VaultFile) {
        synchronized(indexLock) {
            val value = index ?: return
            val file = value.files[path] ?: return
            value.files[path] = transform(file)
            value.snapshotCache = null
        }
    }

    private fun removeFile(path: String) {
        synchronized(indexLock) {
            index?.files?.remove(path)
            index?.snapshotCache = null
        }
    }

    private fun attachmentFile(cleaned: String, noteDir: String): VaultFile? = synchronized(indexLock) {
        val files = index?.files ?: return null
        val candidates = buildList {
            add(cleaned)
            if (noteDir.isNotEmpty()) add("$noteDir/$cleaned")
            add("attachments/$cleaned")
        }
        for (candidate in candidates) {
            files[candidate]?.let { return it }
        }
        if (!cleaned.contains('/')) {
            files.values.firstOrNull { it.path.substringAfterLast('/') == cleaned }
        } else {
            null
        }
    }

    private fun folderUri(path: String, create: Boolean): Uri? {
        if (path.isEmpty()) return treeBase
        synchronized(indexLock) {
            index?.folderUris?.get(path)?.let { return it }
        }
        if (!create) {
            val refreshed = ensureIndex(refresh = true)
            return synchronized(indexLock) { refreshed.folderUris[path] }
        }
        return ensureDirectoryUri(path)
    }

    /** Rare fallback for a folder that is not in the snapshot yet. */
    private fun ensureDirectoryUri(path: String): Uri? {
        var current = root
        var built = ""
        for (segment in path.split('/')) {
            if (segment.isEmpty()) continue
            built = if (built.isEmpty()) segment else "$built/$segment"
            val existing = current.findFile(segment)
            current = if (existing != null && existing.isDirectory) {
                existing
            } else {
                current.createDirectory(segment) ?: return null
            }
            synchronized(indexLock) {
                index?.folders?.add(built)
                index?.folderUris?.put(built, current.uri)
                index?.snapshotCache = null
            }
        }
        return current.uri
    }

    private fun readText(uri: Uri): String =
        resolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
            ?: error("Cannot read: $uri")

    private fun writeText(uri: Uri, text: String) {
        val stream = resolver.openOutputStream(uri, "wt")
            ?: resolver.openOutputStream(uri, "w")
            ?: error("Cannot write: $uri")
        stream.bufferedWriter().use { it.write(text) }
    }

    private fun VaultFile.uri(path: String): Uri =
        uri?.let(Uri::parse) ?: error("No document URI for $path")

    private class SafIndex(
        val files: LinkedHashMap<String, VaultFile> = LinkedHashMap(),
        val folders: HashSet<String> = HashSet(),
        val folderUris: HashMap<String, Uri> = HashMap(),
        var snapshotCache: VaultSnapshot? = null,
    )

    private class DirectoryScan(
        val files: List<VaultFile> = emptyList(),
        val folders: List<String> = emptyList(),
        val folderUris: Map<String, Uri> = emptyMap(),
        val children: List<Pair<Uri, String>> = emptyList(),
    )

    private companion object {
        const val SAF_SCAN_CONCURRENCY = 4
        val CHILD_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_SIZE,
        )
        val MINIMAL_CHILD_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )
    }
}

private const val MAX_ATTACHMENT_BYTES = 16L * 1024 * 1024

private fun InputStream.readBounded(max: Long): ByteArray {
    val output = java.io.ByteArrayOutputStream()
    val buffer = ByteArray(64 * 1024)
    var total = 0L
    while (true) {
        val read = read(buffer)
        if (read <= 0) break
        total += read
        if (total > max) error("Attachment is too large")
        output.write(buffer, 0, read)
    }
    return output.toByteArray()
}

internal fun isMarkdownName(name: String?): Boolean {
    val value = name ?: return false
    return value.endsWith(".md", ignoreCase = true) || value.endsWith(".markdown", ignoreCase = true)
}

private val SERVICE_PATHS = setOf(
    ".vault", ".obsidian", ".git", ".trash", "node_modules", "target",
)

/** Paths the explorer and sync must never surface. */
internal fun isServicePath(path: String): Boolean {
    val normalized = path.replace('\\', '/')
    val name = normalized.substringAfterLast('/')
    if (name == ".DS_Store" || name.startsWith("._")) return true
    return normalized.lowercase().split('/').any { segment ->
        segment in SERVICE_PATHS || segment.startsWith(".")
    }
}

internal fun mimeFor(name: String): String = when {
    name.endsWith(".md", true) || name.endsWith(".markdown", true) -> "text/markdown"
    name.endsWith(".txt", true) -> "text/plain"
    name.endsWith(".png", true) -> "image/png"
    name.endsWith(".jpg", true) || name.endsWith(".jpeg", true) -> "image/jpeg"
    name.endsWith(".gif", true) -> "image/gif"
    name.endsWith(".webp", true) -> "image/webp"
    name.endsWith(".pdf", true) -> "application/pdf"
    else -> "application/octet-stream"
}

internal fun noteItem(path: String, name: String): NoteItem {
    val base = name.substringBeforeLast('.', name)
    val extension = name.substringAfterLast('.', "").lowercase()
    return NoteItem(path = path, name = name, basename = base, extension = extension)
}

internal fun cleanAttachmentTarget(target: String): String? {
    val trimmed = target.trim().removeSurrounding("<", ">")
    if (trimmed.startsWith("http://") || trimmed.startsWith("https://") || trimmed.startsWith("data:")) {
        return null
    }
    val withoutScheme = trimmed.removePrefix("file://")
    val decoded = runCatching { URLDecoder.decode(withoutScheme, "UTF-8") }.getOrDefault(withoutScheme)
    return decoded.trim().ifEmpty { null }
}
