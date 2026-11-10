package md.vibecodium.mobile

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Indexed full-text search over one vault.
 *
 * Kotlin owns the IO, so a plain directory and a SAF tree take the same path:
 * every markdown note is read through the repository and handed to the Rust
 * core, which runs the desktop engine (tokenizer, postings, BM25F). Queries
 * rank paths in the index and only re-read the handful of candidates that
 * need a line number and a snippet — instead of scanning every note on every
 * keystroke.
 *
 * Until the first build completes, [search] answers with the same linear scan
 * the app used before, so search is never unavailable.
 */
internal class SearchIndex(
    private val repository: VaultRepository,
    /** App-private snapshot file; null disables persistence. */
    private val snapshot: java.io.File? = null,
) {
    private val mutex = Mutex()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val building = AtomicBoolean(false)

    @Volatile
    private var indexed = false

    /** `(mtime, size)` per indexed path, for incremental rebuilds. */
    private val fingerprints = HashMap<String, LongArray>()

    init {
        // One vault at a time: a new session must not inherit the previous
        // vault's postings, and a leftover build is cancelled outright.
        synchronized(ACTIVE) {
            ACTIVE.getAndSet(this)?.scope?.cancel()
        }
        CoreBridge.nativeSearchIndexReset()
        restore()
    }

    /**
     * Restores the snapshot the previous session wrote: the index is usable
     * at once and only files whose stamp changed are re-read by [refresh].
     */
    private fun restore() {
        val file = snapshot ?: return
        val stamps = runCatching {
            CoreJson.indexFingerprints(CoreBridge.nativeSearchIndexLoad(file.absolutePath))
        }.getOrNull() ?: return
        synchronized(fingerprints) {
            fingerprints.clear()
            fingerprints.putAll(stamps)
        }
        indexed = true
    }

    private fun persist() {
        val file = snapshot ?: return
        if (!indexed) return
        runCatching { CoreBridge.nativeSearchIndexSave(file.absolutePath) }
    }

    /**
     * `runCatching` that never swallows cancellation: a cancelled search must
     * stop at once instead of quietly falling through to the scan.
     */
    private inline fun <T> ignoringFailure(block: () -> T): T? = try {
        block()
    } catch (cancelled: kotlinx.coroutines.CancellationException) {
        throw cancelled
    } catch (_: Throwable) {
        null
    }

    fun close() {
        scope.cancel()
        synchronized(ACTIVE) {
            if (ACTIVE.get() === this) ACTIVE.set(null)
        }
    }

    /**
     * Builds or refreshes the index. Unchanged files are skipped by
     * fingerprint before they are read, so this is cheap when nothing changed.
     * Pass the file list when the caller already has one, to avoid a second
     * directory walk per query.
     */
    suspend fun refresh(files: List<VaultFile>? = null) = mutex.withLock {
        withContext(Dispatchers.IO) {
            val all = files ?: repository.snapshot(refresh = false).files
            val notes = all.filter { isMarkdownName(it.path) }
            val present = notes.mapTo(HashSet()) { it.path }
            val removed = synchronized(fingerprints) {
                val gone = fingerprints.keys.filter { it !in present }
                gone.forEach { fingerprints.remove(it) }
                gone
            }
            var changed = removed.isNotEmpty()
            for (path in removed) {
                CoreBridge.nativeSearchIndexRemove(path)
            }
            for (file in notes) {
                val stamp = longArrayOf(file.lastModified, file.size)
                val known = synchronized(fingerprints) { fingerprints[file.path] }
                if (known != null && known[0] == stamp[0] && known[1] == stamp[1]) continue
                val text = ignoringFailure { repository.readNote(file.path) } ?: continue
                CoreBridge.nativeSearchIndexAdd(file.path, text, file.lastModified, file.size)
                synchronized(fingerprints) { fingerprints[file.path] = stamp }
                changed = true
                yield()
            }
            CoreBridge.nativeSearchIndexFinish()
            indexed = true
            if (changed) persist()
        }
    }

    /**
     * Drops one path so the next refresh re-reads it. A save keeps the same
     * size often enough, and SAF modification stamps are second-granular.
     */
    fun invalidate(path: String) {
        if (!indexed) return
        synchronized(fingerprints) { fingerprints.remove(path) }
        CoreBridge.nativeSearchIndexRemove(path)
    }

    suspend fun search(query: String, limit: Int): List<SearchHit> {
        val needle = query.trim()
        if (needle.isEmpty() || limit <= 0) return emptyList()
        val files = ignoringFailure { repository.snapshot().files } ?: emptyList()
        if (!indexed) {
            // The first build reads every note. Answer this query from the
            // scan and keep building in the background.
            warmUp()
            return scan(files, needle, limit)
        }
        refresh(files)
        return ignoringFailure { indexedSearch(files, needle, limit) }
            ?: scan(files, needle, limit)
    }

    private fun warmUp() {
        if (!building.compareAndSet(false, true)) return
        scope.launch {
            try {
                refresh()
            } catch (cancelled: kotlinx.coroutines.CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                // A failed build just leaves the scan fallback in place.
            } finally {
                building.set(false)
            }
        }
    }

    private suspend fun indexedSearch(
        files: List<VaultFile>,
        needle: String,
        limit: Int,
    ): List<SearchHit> {
        val hits = ArrayList<SearchHit>(limit)
        val seen = HashSet<String>()
        // Name hits first, like the desktop; they cost no file reads.
        for (hit in nameHits(files, needle, limit)) {
            if (seen.add(hit.path)) hits.add(hit)
        }
        if (hits.size >= limit) return hits
        val candidates = CoreJson.searchCandidates(
            CoreBridge.nativeSearchIndexQuery(needle, limit),
        )
        for (candidate in candidates) {
            if (hits.size >= limit) break
            if (!seen.add(candidate.path)) continue
            val text = ignoringFailure { repository.readNote(candidate.path) } ?: continue
            val located = CoreJson.searchLocated(CoreBridge.nativeSearchLocate(needle, text))
            if (located.line <= 0 && located.snippet.isEmpty()) continue
            hits.add(SearchHit(candidate.path, candidate.name, located.line, located.snippet))
        }
        return hits
    }

    private fun nameHits(files: List<VaultFile>, needle: String, limit: Int): List<SearchHit> {
        val lower = needle.lowercase()
        val hits = ArrayList<SearchHit>()
        for (file in files) {
            if (hits.size >= limit) break
            if (!isMarkdownName(file.path)) continue
            val name = file.path.substringAfterLast('/')
            if (name.lowercase().contains(lower)) {
                hits.add(SearchHit(file.path, name, 0, ""))
            }
        }
        return hits
    }

    /** The pre-index behaviour: read every note and keep the first hit. */
    private suspend fun scan(
        files: List<VaultFile>,
        needle: String,
        limit: Int,
    ): List<SearchHit> {
        val lower = needle.lowercase()
        val hits = ArrayList<SearchHit>()
        for (file in files) {
            if (hits.size >= limit) break
            if (!isMarkdownName(file.path)) continue
            val name = file.path.substringAfterLast('/')
            val nameHit = name.lowercase().contains(lower)
            val text = ignoringFailure { repository.readNote(file.path) } ?: continue
            var lineHit: Pair<Int, String>? = null
            for ((lineIndex, line) in text.lineSequence().withIndex()) {
                if (line.lowercase().contains(lower)) {
                    lineHit = (lineIndex + 1) to line.trim().take(160)
                    break
                }
            }
            when {
                lineHit != null -> hits.add(SearchHit(file.path, name, lineHit.first, lineHit.second))
                nameHit -> hits.add(SearchHit(file.path, name, 0, ""))
            }
            yield()
        }
        return hits
    }

    private companion object {
        /** The session currently owning the native index. */
        val ACTIVE = java.util.concurrent.atomic.AtomicReference<SearchIndex?>(null)
    }
}
