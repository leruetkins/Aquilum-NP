package md.vibecodium.mobile

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SyncEngineTest {
    private class FakeRepository(
        val files: MutableMap<String, ByteArray> = mutableMapOf(),
        val modified: MutableMap<String, Long> = mutableMapOf(),
    ) : VaultRepository {
        override val name: String = "vault"
        override val isScoped: Boolean = false

        override suspend fun listNotes(): List<NoteItem> = files.keys.map { noteItem(it, it) }
        override suspend fun readNote(path: String): String =
            files[path]?.toString(Charsets.UTF_8) ?: error("missing $path")

        override suspend fun writeNote(path: String, text: String) = writeBytes(path, text.toByteArray())
        override suspend fun createNote(path: String, text: String): NoteItem {
            writeBytes(path, text.toByteArray())
            return noteItem(path, path)
        }

        override suspend fun deleteNote(path: String) = deleteFile(path)
        override suspend fun createFolder(path: String) = Unit
        override suspend fun renameNote(oldPath: String, newPath: String): NoteItem {
            val bytes = files.remove(oldPath) ?: error("missing $oldPath")
            files[newPath] = bytes
            return noteItem(newPath, newPath)
        }

        override suspend fun search(query: String, limit: Int): List<SearchHit> = emptyList()

        override suspend fun readAttachment(target: String, notePath: String): ByteArray? = files[target]

        override suspend fun listFiles(): List<VaultFile> =
            files.map { (path, bytes) ->
                VaultFile(path, bytes.size.toLong(), modified[path] ?: 0L)
            }

        override suspend fun readBytes(path: String): ByteArray? = files[path]
        override suspend fun writeBytes(path: String, bytes: ByteArray) {
            files[path] = bytes
            modified[path] = System.currentTimeMillis()
        }

        override suspend fun deleteFile(path: String) {
            files.remove(path)
            modified.remove(path)
        }

        override fun close() = Unit
    }

    private class FakeRecords : SyncRecords {
        private val records = mutableMapOf<Pair<String, String>, SyncRecord>()
        private val states = mutableMapOf<String, SyncState>()

        override fun get(vault: String, path: String): SyncRecord? = records[vault to path]
        override fun all(vault: String): Map<String, SyncRecord> =
            records.filterKeys { it.first == vault }.mapKeys { it.key.second }

        override fun put(vault: String, record: SyncRecord) {
            records[vault to record.path] = record
        }

        override fun remove(vault: String, path: String) {
            records.remove(vault to path)
        }

        override fun state(vault: String): SyncState? = states[vault]
        override fun saveState(vault: String, state: SyncState) {
            states[vault] = state
        }
    }

    private class FakeProvider : SyncProvider {
        val remote = mutableMapOf<String, ByteArray>()
        val revs = mutableMapOf<String, Int>()
        val root = "/vault"
        var listCalls = 0
        var latestCursorCalls = 0
        var deltaPayload: RemoteDelta? = null

        private fun key(path: String): String =
            if (path == root) "" else path.removePrefix("$root/")

        override suspend fun accountName(): String = "test"

        override suspend fun ensureFolder(path: String) = Unit

        override suspend fun listFolder(root: String): List<RemoteEntry> {
            listCalls += 1
            return remote.map { (path, bytes) ->
                RemoteEntry(path, "rev-${revs[path]}", "", bytes.size.toLong())
            }
        }

        override suspend fun latestCursor(root: String): String {
            latestCursorCalls += 1
            return "cursor-0"
        }

        override suspend fun delta(root: String, cursor: String): RemoteDelta? = deltaPayload

        override suspend fun metadata(path: String): RemoteEntry? {
            val relative = key(path)
            val bytes = remote[relative] ?: return null
            return RemoteEntry(relative, "rev-${revs[relative]}", "", bytes.size.toLong())
        }

        override suspend fun download(path: String): RemoteDownload {
            val relative = key(path)
            val bytes = remote[relative] ?: throw DropboxException(409, "path/not_found")
            return RemoteDownload(bytes, "rev-${revs[relative]}", "")
        }

        override suspend fun upload(path: String, bytes: ByteArray, rev: String?): RemoteEntry {
            val relative = key(path)
            val current = revs[relative]
            if (rev != null && rev != "rev-$current") {
                throw DropboxException(409, "conflict")
            }
            val next = (current ?: 0) + 1
            remote[relative] = bytes
            revs[relative] = next
            return RemoteEntry(relative, "rev-$next", "", bytes.size.toLong())
        }

        override suspend fun delete(path: String) {
            val relative = key(path)
            remote.remove(relative)
            revs.remove(relative)
        }
    }

    private fun engine(
        repository: FakeRepository,
        records: FakeRecords,
        provider: FakeProvider,
    ) = SyncEngine(repository, "vault", records, provider)

    @Test
    fun newLocalFileIsUploaded() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "hello".toByteArray()
        val provider = FakeProvider()
        val outcome = engine(repository, FakeRecords(), provider).sync("/vault")

        assertEquals(1, outcome.uploaded)
        assertEquals("hello", provider.remote["note.md"]?.toString(Charsets.UTF_8))
    }

    @Test
    fun newRemoteFileIsDownloaded() = runBlocking {
        val provider = FakeProvider()
        provider.remote["note.md"] = "from dropbox".toByteArray()
        provider.revs["note.md"] = 1
        val repository = FakeRepository()
        val outcome = engine(repository, FakeRecords(), provider).sync("/vault")

        assertEquals(1, outcome.downloaded)
        assertEquals("from dropbox", repository.files["note.md"]?.toString(Charsets.UTF_8))
    }

    @Test
    fun localChangeIsPushed() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "one".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")

        repository.files["note.md"] = "two".toByteArray()
        repository.modified["note.md"] = (repository.modified["note.md"] ?: 0L) + 5_000
        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.uploaded)
        assertEquals("two", provider.remote["note.md"]?.toString(Charsets.UTF_8))
    }

    @Test
    fun remoteChangeIsPulled() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "one".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")

        provider.remote["note.md"] = "remote two".toByteArray()
        provider.revs["note.md"] = 2
        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.downloaded)
        assertEquals("remote two", repository.files["note.md"]?.toString(Charsets.UTF_8))
    }

    @Test
    fun bothSidesChangedCreateConflictCopy() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "base".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")

        repository.files["note.md"] = "local edit".toByteArray()
        repository.modified["note.md"] = (repository.modified["note.md"] ?: 0L) + 5_000
        provider.remote["note.md"] = "remote edit".toByteArray()
        provider.revs["note.md"] = 2

        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.conflicts)
        assertEquals("local edit", repository.files["note.md"]?.toString(Charsets.UTF_8))
        assertTrue(repository.files.keys.any { it.contains("conflict") })
        assertTrue(provider.remote.keys.any { it.contains("conflict") })
        assertEquals("local edit", provider.remote["note.md"]?.toString(Charsets.UTF_8))
    }

    @Test
    fun localDeletionPropagatesToRemote() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "one".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")

        repository.deleteFile("note.md")
        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.deletedRemote)
        assertTrue(provider.remote.isEmpty())
    }

    @Test
    fun bulkDeletionIsRefused() = runBlocking {
        val repository = FakeRepository()
        repeat(12) { index ->
            repository.files["note$index.md"] = "x".toByteArray()
        }
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")

        repository.files.keys.toList().forEach { repository.deleteFile(it) }

        var aborted = false
        try {
            engine.sync("/vault")
        } catch (_: SyncAbortedException) {
            aborted = true
        }
        assertTrue(aborted)
        assertEquals(12, provider.remote.size)
    }

    @Test
    fun secondPassUsesDeltaInsteadOfListingEverything() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "one".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")
        assertEquals(1, provider.listCalls)
        assertEquals(1, provider.latestCursorCalls)

        provider.remote["note.md"] = "remote two".toByteArray()
        provider.revs["note.md"] = 2
        provider.deltaPayload = RemoteDelta(
            changed = listOf(RemoteEntry("note.md", "rev-2", "", 10)),
            removed = emptyList(),
            cursor = "cursor-1",
        )

        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.downloaded)
        assertEquals("remote two", repository.files["note.md"]?.toString(Charsets.UTF_8))
        assertEquals(1, provider.listCalls)
        assertEquals(1, provider.latestCursorCalls)
    }

    @Test
    fun changedFilterForcesAFullList() = runBlocking {
        val repository = FakeRepository()
        repository.files["note.md"] = "one".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        engine(repository, records, provider).sync("/vault")
        assertEquals(1, provider.listCalls)

        provider.deltaPayload = RemoteDelta(emptyList(), emptyList(), "cursor-1")
        val filtered = SyncEngine(
            repository,
            "vault",
            records,
            provider,
            filter = SyncFilter(allowImages = false),
        )
        filtered.sync("/vault")

        assertEquals(2, provider.listCalls)
    }

    @Test
    fun deltaFolderRemovalDeletesLocalChildren() = runBlocking {
        val repository = FakeRepository()
        repository.files["Archive/note.md"] = "old".toByteArray()
        val records = FakeRecords()
        val provider = FakeProvider()
        val engine = engine(repository, records, provider)
        engine.sync("/vault")
        assertEquals(1, repository.files.size)

        provider.deltaPayload = RemoteDelta(
            changed = emptyList(),
            removed = listOf("Archive"),
            cursor = "cursor-1",
        )
        val outcome = engine.sync("/vault")

        assertEquals(1, outcome.deletedLocal)
        assertTrue(repository.files.isEmpty())
    }
}
