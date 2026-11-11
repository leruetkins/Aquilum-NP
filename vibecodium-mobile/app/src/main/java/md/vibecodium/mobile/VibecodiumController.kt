package md.vibecodium.mobile

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Holds the phone shell state, owns the active [VaultRepository] and every JNI
 * parse call. File work runs on Dispatchers.IO; state writes come back to the
 * main dispatcher.
 */
class VibecodiumController(context: Context) {
    private val appContext = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private var repository: VaultRepository? = null
    private var locator: VaultLocator? = null

    private val prefs = appContext.getSharedPreferences("vibecodium", Context.MODE_PRIVATE)
    private val syncRecords by lazy { SyncDatabase(appContext) }
    private val dropboxTokens by lazy { DropboxTokenStore(appContext) }
    private var syncJob: Job? = null
    private var syncServer: LoopbackServer? = null

    var state by mutableStateOf(UiState())
        private set

    private var saveJob: Job? = null
    private var derivedJob: Job? = null
    private var searchJob: Job? = null
    private var tabPersistJob: Job? = null

    /**
     * Cancellation is not a failure. A superseded search or a vault switch
     * cancels the running job; that must unwind through the coroutine
     * machinery instead of being reported as an error, otherwise the UI shows
     * "StandaloneCoroutine was canceled".
     */
    private fun Throwable.rethrowIfCancelled() {
        if (this is kotlinx.coroutines.CancellationException) throw this
    }

    init {
        state = state.copy(settings = SettingsStore.load(appContext))
        VaultLocatorStore.load(appContext)?.let { locator ->
            // Keep the launch spinner up until the repository and the saved
            // tabs are ready, instead of flashing the start and empty screens.
            state = state.copy(booting = true, loading = true)
            openLocator(locator)
        }
    }

    // ---------------------------------------------------------------- opening

    fun openSafVault(uri: Uri) {
        VaultRepositories.takePermission(appContext, uri)
        openLocator(VaultLocator.Scoped(uri.toString()))
    }

    fun openVault(path: String) {
        val trimmed = path.trim()
        if (trimmed.isEmpty()) {
            state = state.copy(message = "Enter a vault folder")
            return
        }
        openLocator(VaultLocator.Local(File(trimmed).absolutePath))
    }

    fun createDemoVault() {
        scope.launch {
            state = state.copy(loading = true, booting = true, message = null)
            closeRepository()
            try {
                val (repository, locator) = withContext(Dispatchers.IO) {
                    VaultRepositories.createDemo(appContext)
                }
                applyRepository(repository, locator)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = UiState(
                    settings = state.settings,
                    loading = false,
                    message = error.message ?: "Could not create the vault",
                )
            }
        }
    }

    private fun openLocator(locator: VaultLocator) {
        scope.launch {
            state = state.copy(loading = true, booting = true, message = null)
            closeRepository()
            try {
                val repository = withContext(Dispatchers.IO) {
                    VaultRepositories.open(appContext, locator)
                }
                applyRepository(repository, locator)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = UiState(
                    settings = state.settings,
                    loading = false,
                    message = error.message ?: "Could not open the vault",
                )
                VaultLocatorStore.clear(appContext)
            }
        }
    }

    private suspend fun applyRepository(repository: VaultRepository, locator: VaultLocator) {
        this.repository = repository
        this.locator = locator
        VaultLocatorStore.save(appContext, locator)

        val collapsedFolders = FolderStore.load(appContext, locator.value)
        val bookmarks = BookmarkStore.load(appContext, locator.value)
        // Publish the shell before touching the tree. The full snapshot is
        // filled in by [refreshLists] on IO; a large vault must not hold the
        // first frame behind a recursive walk.
        state = state.copy(
            vaultName = repository.name,
            vaultRoot = locator.value,
            vaultKind = if (repository.isScoped) "saf" else "file",
            notes = emptyList(),
            entries = emptyList(),
            explorerTree = emptyList(),
            folders = emptyList(),
            collapsedFolders = collapsedFolders,
            fileQuery = "",
            bookmarks = bookmarks,
            tabs = emptyList(),
            activeTab = -1,
            loading = false,
            drawer = null,
            tabSwitcher = false,
            editorMode = state.settings.defaultEditorMode,
            lines = emptyList(),
            markdownText = "",
            headings = emptyList(),
            stats = DocStats(0, 0, 0),
            history = emptyList(),
            historyIndex = -1,
            searchQuery = "",
            searchHits = emptyList(),
            deletePath = null,
            viewerPath = null,
            renamePath = null,
            renameValue = "",
            newFolderOpen = false,
            newFolderValue = "",
        )
        if (restoreTabs(repository)) persistTabs()
        // Saved tabs are now in memory. Only at this point may the shell show
        // the empty "new note" state when nothing was restored.
        state = state.copy(booting = false)
        refreshSyncState()
        scope.launch { refreshLists(repository) }
        // Warm the full-text index in the background; [search] answers from
        // the linear scan until it is ready.
        scope.launch {
            try {
                repository.warmSearchIndex()
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
            }
        }
    }

    /** Returns true when at least one saved tab was reopened. */
    private suspend fun restoreTabs(repository: VaultRepository): Boolean {
        val saved = TabStore.load(appContext, locatorKey()) ?: return false
        if (saved.tabs.isEmpty()) return false
        val tabs = mutableListOf<Tab>()
        for (savedTab in saved.tabs) {
            // A persisted SAF URI reopens the note without resolving every
            // path segment through the provider.
            val text = savedTab.uri?.let { repository.readDocument(it) }
                ?: runCatching { repository.readNote(savedTab.path) }.getOrNull()
                ?: continue
            tabs.add(
                Tab(
                    path = savedTab.path,
                    title = titleOf(savedTab.path),
                    text = text,
                    selection = savedTab.selection.coerceIn(0, text.length),
                    language = codeLanguageFor(savedTab.path),
                    uri = savedTab.uri ?: repository.uriFor(savedTab.path),
                ),
            )
        }
        if (tabs.isEmpty()) return false
        val active = saved.active.coerceIn(0, tabs.lastIndex)
        state = state.copy(
            tabs = tabs,
            activeTab = active,
            history = listOf(tabs[active].path),
            historyIndex = 0,
        )
        refreshDerived(tabs[active].text)
        return true
    }

    /** Completes the tree after the shell is already visible. */
    private suspend fun refreshLists(repository: VaultRepository) {
        try {
            val lists = loadFileLists(repository)
            if (this.repository !== repository) return
            val tabs = tabsWithUris(state.tabs)
            state = state.copy(
                notes = lists.notes,
                entries = lists.entries,
                folders = lists.folders,
                explorerTree = lists.tree,
                tabs = tabs,
            )
            if (state.tabs.isNotEmpty()) persistTabs()
            if (state.sync.autoSync && state.sync.connected) {
                delay(700)
                syncNow()
            }
        } catch (error: Throwable) {
            error.rethrowIfCancelled()
            if (this.repository === repository) {
                state = state.copy(message = error.message ?: "Could not read the file list")
            }
        }
    }

    private fun tabsWithUris(tabs: List<Tab>): List<Tab> {
        val repository = repository ?: return tabs
        return tabs.map { tab ->
            if (tab.uri != null) tab
            else repository.uriFor(tab.path)?.let { tab.copy(uri = it) } ?: tab
        }
    }

    private fun closeRepository() {
        repository?.close()
        runCatching { CoreBridge.nativeCloseVault() }
        repository = null
    }

    // ------------------------------------------------------------------ notes

    fun openNote(path: String, pushHistory: Boolean = true) {
        val existing = state.tabs.indexOfFirst { it.path == path }
        if (existing >= 0) {
            selectTab(existing)
            if (pushHistory) pushHistory(path)
            return
        }
        val repository = repository ?: return
        scope.launch {
            try {
                val text = repository.readNote(path)
                val tab = Tab(
                    path = path,
                    title = titleOf(path),
                    text = text,
                    language = codeLanguageFor(path),
                    uri = repository.uriFor(path),
                )
                val tabs = state.tabs + tab
                val mode = state.settings.defaultEditorMode
                state = state.copy(
                    tabs = tabs,
                    activeTab = tabs.lastIndex,
                    drawer = null,
                    tabSwitcher = false,
                    viewerPath = null,
                    editorMode = mode,
                )
                if (pushHistory) pushHistory(path)
                refreshDerived(text, force = mode == EditorMode.Preview)
                persistTabs()
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not read the note")
            }
        }
    }

    fun newNote() {
        val repository = repository ?: return
        val existing = state.notes.mapTo(HashSet()) { it.path }
        var counter = 1
        var path = "Untitled.md"
        while (path in existing) {
            counter += 1
            path = "Untitled $counter.md"
        }
        scope.launch {
            try {
                val note = repository.createNote(path, "")
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                )
                openNote(note.path)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not create a note")
            }
        }
    }

    fun updateEditor(value: TextFieldValue) {
        val index = state.activeTab
        if (index !in state.tabs.indices) return
        val adjusted = autoIndent(state.tabs[index].text, value)
        val tabs = state.tabs.toMutableList()
        tabs[index] = tabs[index].copy(
            text = adjusted.text,
            selection = adjusted.selection.start.coerceIn(0, adjusted.text.length),
            dirty = true,
        )
        state = state.copy(tabs = tabs)

        saveJob?.cancel()
        saveJob = scope.launch {
            delay(state.settings.autoSaveDelayMs.toLong())
            persistTab(index)
        }
        derivedJob?.cancel()
        derivedJob = scope.launch {
            delay(200)
            refreshDerived(adjusted.text)
        }
        schedulePersistTabs()
    }

    /** Text coming from the native SurfaceView editor. */
    fun updateNativeEditor(text: String, selection: Int) {
        val index = state.activeTab
        val tab = state.tabs.getOrNull(index) ?: return
        if (text == tab.text) {
            val tabs = state.tabs.toMutableList()
            tabs[index] = tab.copy(selection = selection.coerceIn(0, tab.text.length))
            state = state.copy(tabs = tabs)
            schedulePersistTabs()
            return
        }
        updateEditor(
            TextFieldValue(
                text = text,
                selection = TextRange(selection.coerceIn(0, text.length)),
            ),
        )
    }

    fun saveActiveNow() {
        saveJob?.cancel()
        val index = state.activeTab
        scope.launch {
            persistTab(index)
            persistTabs()
        }
    }

    private suspend fun persistTab(index: Int) {
        val repository = repository ?: return
        val tab = state.tabs.getOrNull(index) ?: return
        if (!tab.dirty) return
        try {
            repository.writeNote(tab.path, tab.text)
            val tabs = state.tabs.toMutableList()
            val current = tabs.getOrNull(index)
            if (current != null && current.path == tab.path && current.text == tab.text) {
                tabs[index] = current.copy(dirty = false)
                state = state.copy(tabs = tabs)
            }
        } catch (error: Throwable) {
            error.rethrowIfCancelled()
            state = state.copy(message = error.message ?: "Could not save the note")
        }
    }

    private suspend fun refreshDerived(text: String, force: Boolean = false) {
        val language = state.tabs.getOrNull(state.activeTab)?.language
        try {
            if (language != null) {
                val stats = withContext(Dispatchers.IO) {
                    CoreJson.stats(CoreBridge.nativeStats(text))
                }
                if (state.tabs.getOrNull(state.activeTab)?.text == text) {
                    state = state.copy(
                        headings = emptyList(),
                        stats = stats,
                        lines = emptyList(),
                        markdownText = "",
                    )
                }
                return
            }
            // One native crossing builds the line model once and derives the
            // outline from it; opening a note used to parse it three times.
            val tooLarge = !force && (
                text.length > LIVE_MODEL_CHAR_LIMIT ||
                    text.count { it == '\n' } > LIVE_MODEL_LINE_LIMIT
                )
            if (tooLarge) {
                val stats = withContext(Dispatchers.IO) {
                    CoreJson.stats(CoreBridge.nativeStats(text))
                }
                if (state.tabs.getOrNull(state.activeTab)?.text == text) {
                    state = state.copy(
                        headings = emptyList(),
                        stats = stats,
                        lines = emptyList(),
                        markdownText = "",
                    )
                }
                return
            }
            val raw = withContext(Dispatchers.IO) {
                CoreBridge.nativeAnalyze(text)
            }
            val analysis = withContext(Dispatchers.IO) {
                CoreJson.analyze(raw)
            }
            if (state.tabs.getOrNull(state.activeTab)?.text == text) {
                state = state.copy(
                    headings = analysis.headings,
                    stats = analysis.stats,
                    lines = analysis.lines,
                    markdownText = text,
                )
            }
        } catch (error: Throwable) {
            error.rethrowIfCancelled()
            // Derived data is cosmetic; a failed parse must not break editing.
        }
    }

    fun selectTab(index: Int) {
        if (index !in state.tabs.indices) {
            state = state.copy(drawer = null, tabSwitcher = false)
            return
        }
        saveActiveNow()
        state = state.copy(activeTab = index, drawer = null, tabSwitcher = false, viewerPath = null)
        scope.launch {
            refreshDerived(
                state.tabs[index].text,
                force = state.editorMode == EditorMode.Preview,
            )
        }
        persistTabs()
    }

    fun closeTab(index: Int) {
        if (index !in state.tabs.indices) return
        saveJob?.cancel()
        val closing = state.tabs[index]
        if (closing.dirty) {
            val repository = repository
            if (repository != null) {
                scope.launch {
                    runCatching { repository.writeNote(closing.path, closing.text) }
                }
            }
        }
        val active = removeTabState(index)
        if (active >= 0) {
            scope.launch {
                refreshDerived(
                    state.tabs[active].text,
                    force = state.editorMode == EditorMode.Preview,
                )
            }
        } else {
            state = state.copy(headings = emptyList(), stats = DocStats(0, 0, 0), lines = emptyList(), markdownText = "")
        }
        persistTabs()
    }

    private fun removeTabState(index: Int): Int {
        if (index !in state.tabs.indices) return state.activeTab
        val tabs = state.tabs.toMutableList().also { it.removeAt(index) }
        val active = when {
            tabs.isEmpty() -> -1
            index < state.activeTab -> state.activeTab - 1
            index == state.activeTab -> index.coerceAtMost(tabs.lastIndex)
            else -> state.activeTab
        }
        state = state.copy(tabs = tabs, activeTab = active, tabSwitcher = false)
        return active
    }

    fun confirmDeleteActive() {
        state.tabs.getOrNull(state.activeTab)?.let { tab ->
            state = state.copy(deletePath = tab.path)
        }
    }

    fun dismissDelete() {
        state = state.copy(deletePath = null)
    }

    fun deleteConfirmed() {
        val path = state.deletePath ?: return
        val repository = repository ?: return
        val index = state.tabs.indexOfFirst { it.path == path }
        state = state.copy(deletePath = null)
        scope.launch {
            try {
                repository.deleteNote(path)
                if (index >= 0) {
                    val active = removeTabState(index)
                    if (active >= 0) {
                        refreshDerived(
                state.tabs[active].text,
                force = state.editorMode == EditorMode.Preview,
            )
                    } else {
                        state = state.copy(
                            headings = emptyList(),
                            stats = DocStats(0, 0, 0),
                            lines = emptyList(),
                            markdownText = "",
                        )
                    }
                }
                val bookmarks = state.bookmarks.toMutableSet().also { it.remove(path) }
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    bookmarks = bookmarks,
                )
                persistTabs()
                persistBookmarks()
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not delete the note")
            }
        }
    }

    // ----------------------------------------------------------------- rename

    fun beginRename() {
        val tab = state.tabs.getOrNull(state.activeTab) ?: return
        state = state.copy(renamePath = tab.path, renameValue = tab.title)
    }

    fun updateRenameValue(value: String) {
        state = state.copy(renameValue = value)
    }

    fun dismissRename() {
        state = state.copy(renamePath = null, renameValue = "")
    }

    // ------------------------------------------------------------- new folder

    fun beginNewFolder() {
        state = state.copy(newFolderOpen = true, newFolderValue = "")
    }

    fun updateNewFolderValue(value: String) {
        state = state.copy(newFolderValue = value)
    }

    fun dismissNewFolder() {
        state = state.copy(newFolderOpen = false, newFolderValue = "")
    }

    fun confirmNewFolder() {
        val repository = repository ?: return
        val name = state.newFolderValue.trim()
        if (name.isEmpty() || name == "." || name == ".." || name.contains('/') || name.contains('\\')) {
            state = state.copy(message = "Invalid folder name")
            return
        }
        scope.launch {
            try {
                repository.createFolder(name)
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    newFolderOpen = false,
                    newFolderValue = "",
                )
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not create the folder")
            }
        }
    }

    fun confirmRename() {
        val oldPath = state.renamePath ?: return
        val repository = repository ?: return
        val name = state.renameValue.trim()
        if (name.isEmpty() || name == "." || name == ".." || name.contains('/') || name.contains('\\')) {
            state = state.copy(message = "Invalid note name")
            return
        }
        val oldExtension = oldPath.substringAfterLast('.', "")
        val newHasExtension = name.substringAfterLast('.', "").isNotEmpty()
        val fileName = when {
            isMarkdownName(oldPath) -> if (isMarkdownName(name)) name else "$name.md"
            oldExtension.isNotEmpty() && !newHasExtension -> "$name.$oldExtension"
            else -> name
        }
        val parent = oldPath.substringBeforeLast('/', "")
        val newPath = if (parent.isEmpty()) fileName else "$parent/$fileName"
        if (newPath == oldPath) {
            dismissRename()
            return
        }
        scope.launch {
            try {
                repository.renameNote(oldPath, newPath)
                val tabs = state.tabs.toMutableList()
                val index = tabs.indexOfFirst { it.path == oldPath }
                if (index >= 0) {
                    tabs[index] = tabs[index].copy(
                        path = newPath,
                        title = titleOf(newPath),
                        language = codeLanguageFor(newPath),
                    )
                }
                val bookmarks = state.bookmarks.toMutableSet()
                if (bookmarks.remove(oldPath)) bookmarks.add(newPath)
                val lists = loadFileLists(repository)
                state = state.copy(
                    tabs = tabs,
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    bookmarks = bookmarks,
                    renamePath = null,
                    renameValue = "",
                )
                persistTabs()
                persistBookmarks()
                if (index >= 0 && index == state.activeTab) {
                    refreshDerived(
                        state.tabs[index].text,
                        force = state.editorMode == EditorMode.Preview,
                    )
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Rename failed")
            }
        }
    }

    // -------------------------------------------------------------- bookmarks

    fun toggleBookmarkActive() {
        val path = state.tabs.getOrNull(state.activeTab)?.path ?: return
        toggleBookmark(path)
    }

    fun toggleBookmark(path: String) {
        val bookmarks = state.bookmarks.toMutableSet()
        if (!bookmarks.add(path)) {
            bookmarks.remove(path)
        }
        state = state.copy(bookmarks = bookmarks)
        persistBookmarks()
    }

    // ----------------------------------------------------------------- search

    fun search(query: String) {
        state = state.copy(searchQuery = query)
        searchJob?.cancel()
        val repository = repository
        if (query.isBlank() || repository == null) {
            state = state.copy(searchHits = emptyList())
            return
        }
        searchJob = scope.launch {
            delay(200)
            try {
                val hits = repository.search(query, 50)
                if (state.searchQuery == query) {
                    state = state.copy(searchHits = hits)
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Search failed")
            }
        }
    }

    // ------------------------------------------------------------------- mode

    fun setEditorMode(mode: EditorMode) {
        val previous = state.editorMode
        state = state.copy(editorMode = mode)
        if (mode == EditorMode.Preview && previous != EditorMode.Preview) {
            val tab = state.tabs.getOrNull(state.activeTab) ?: return
            if (tab.language == null && tab.text.isNotBlank() && state.lines.isEmpty()) {
                scope.launch { refreshDerived(tab.text, force = true) }
            }
        }
    }

    suspend fun loadAttachment(target: String, notePath: String): ByteArray? =
        repository?.readAttachment(target, notePath)

    // -------------------------------------------------------------------- sync

    fun openSync() {
        state = state.copy(syncOpen = true, settingsOpen = false)
    }

    // ---------------------------------------------------------------- settings

    fun openSettings() {
        state = state.copy(
            settingsOpen = true,
            drawer = null,
            tabSwitcher = false,
        )
    }

    fun closeSettings() {
        state = state.copy(settingsOpen = false)
    }

    fun setTextScale(value: Float) = updateSettings {
        it.copy(
            textScale = value.coerceIn(
                AppSettingsJson.MIN_TEXT_SCALE,
                AppSettingsJson.MAX_TEXT_SCALE,
            ),
        )
    }

    fun setAutoHideChrome(enabled: Boolean) = updateSettings { it.copy(autoHideChrome = enabled) }

    fun setNavbarEnabled(enabled: Boolean) = updateSettings { it.copy(navbarEnabled = enabled) }

    fun setDefaultEditorMode(mode: EditorMode) = updateSettings { it.copy(defaultEditorMode = mode) }

    fun setAutoSaveDelay(millis: Int) = updateSettings { it.copy(autoSaveDelayMs = millis) }

    fun resetSettings() {
        updateSettings { AppSettings() }
        showMessage("Settings reset")
    }

    fun resetCollapsedFolders() {
        state = state.copy(collapsedFolders = emptySet())
        persistCollapsedFolders(emptySet())
        showMessage("Folder state reset")
    }

    fun rebuildSearchIndex() {
        val repository = repository ?: return
        scope.launch {
            try {
                repository.warmSearchIndex()
                showMessage("Search index refreshed")
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                showMessage(error.message ?: "Could not refresh the search index")
            }
        }
    }

    private inline fun updateSettings(transform: (AppSettings) -> AppSettings) {
        val next = transform(state.settings)
        state = state.copy(settings = next)
        SettingsStore.save(appContext, next)
    }

    fun refreshNotes() {
        val repository = repository ?: return
        scope.launch {
            try {
                val lists = loadFileLists(repository, refresh = true)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                )
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not refresh the file list")
            }
        }
    }

    fun toggleFolder(path: String) {
        val collapsed = state.collapsedFolders.toMutableSet()
        if (!collapsed.add(path)) collapsed.remove(path)
        state = state.copy(collapsedFolders = collapsed)
        persistCollapsedFolders(collapsed)
    }

    fun setFileQuery(value: String) {
        state = state.copy(fileQuery = value)
    }

    /** One toolbar action: collapse every folder, or expand every folder. */
    fun toggleAllFolders() {
        val folders = state.folders.toSet()
        if (folders.isEmpty()) return
        val collapse = folders.any { it !in state.collapsedFolders }
        val collapsed = if (collapse) folders else emptySet()
        state = state.copy(collapsedFolders = collapsed)
        persistCollapsedFolders(collapsed)
    }

    private fun persistCollapsedFolders(collapsed: Set<String>) {
        val key = locatorKey()
        if (key.isNotEmpty()) {
            FolderStore.save(appContext, key, collapsed)
        }
    }

    fun openAttachment(path: String) {
        if (isCodePath(path)) {
            openNote(path)
            return
        }
        val extension = path.substringAfterLast('.', "").lowercase()
        if (extension in IMAGE_EXTENSIONS) {
            state = state.copy(
                viewerPath = path,
                drawer = null,
                tabSwitcher = false,
                syncOpen = false,
            )
        } else {
            state = state.copy(
                message = "Preview is not supported for ${extension.ifBlank { "this file" }} yet",
            )
        }
    }

    fun closeAttachment() {
        state = state.copy(viewerPath = null)
    }

    suspend fun loadFile(path: String): ByteArray? = repository?.readBytes(path)

    fun closeSync() {
        state = state.copy(syncOpen = false)
    }

    fun updateSyncClientId(value: String) {
        dropboxTokens.clientId = value
        state = state.copy(sync = state.sync.copy(clientId = dropboxTokens.clientId))
    }

    fun setAutoSync(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_SYNC_ON_OPEN, enabled).apply()
        state = state.copy(sync = state.sync.copy(autoSync = enabled))
    }

    fun connectDropbox() {
        if (state.sync.busy) return
        syncJob?.cancel()
        syncJob = scope.launch {
            state = state.copy(
                sync = state.sync.copy(busy = true, progress = "Opening Dropbox…", message = ""),
            )
            val server = LoopbackServer()
            syncServer = server
            try {
                server.start()
                val pkce = DropboxAuth.newPkce()
                val url = DropboxAuth.authUrl(dropboxTokens.clientId, pkce)
                openBrowser(url)
                val code = server.awaitCode(pkce.state, 180_000)
                val tokens = DropboxAuth.exchange(dropboxTokens.clientId, code, pkce.verifier)
                dropboxTokens.save(tokens)
                state = state.copy(
                    sync = state.sync.copy(
                        connected = true,
                        busy = true,
                        progress = "Connected. Starting sync…",
                        message = "",
                    ),
                )
                val account = runCatching { DropboxProvider(dropboxTokens).accountName() }
                    .getOrDefault("Dropbox")
                state = state.copy(
                    sync = state.sync.copy(
                        accountName = account,
                        busy = false,
                        progress = "",
                        message = "Connected to Dropbox",
                    ),
                )
                runSync()
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    sync = state.sync.copy(
                        busy = false,
                        progress = "",
                        message = error.message ?: "Dropbox sign-in failed",
                    ),
                )
            } finally {
                server.close()
                syncServer = null
            }
        }
    }

    fun disconnectDropbox() {
        dropboxTokens.clear()
        state = state.copy(
            sync = state.sync.copy(
                connected = false,
                accountName = "",
                busy = false,
                progress = "",
                message = "Dropbox disconnected",
            ),
        )
    }

    fun syncNow() {
        if (state.sync.busy) return
        if (!dropboxTokens.connected) {
            state = state.copy(sync = state.sync.copy(message = "Connect Dropbox first"))
            return
        }
        syncJob?.cancel()
        syncJob = scope.launch { runSync() }
    }

    private suspend fun runSync() {
        val repository = repository ?: return
        val key = locatorKey()
        if (key.isEmpty()) return
        state = state.copy(sync = state.sync.copy(busy = true, progress = "Saving…", message = ""))
        try {
            saveJob?.cancel()
            for (index in state.tabs.indices) {
                persistTab(index)
            }
            val provider = DropboxProvider(dropboxTokens)
            val account = runCatching { provider.accountName() }.getOrDefault("Dropbox")
            val remoteRoot = remoteRootFor(state.vaultName ?: "vibecodium")
            val engine = SyncEngine(repository, key, syncRecords, provider) { progress ->
                state = state.copy(
                    sync = state.sync.copy(
                        progress = if (progress.total > 0) {
                            "${progress.phase} ${progress.done}/${progress.total}"
                        } else {
                            progress.phase
                        },
                    ),
                )
            }
            // Sync must see files changed or created outside the app, so drop
            // the directory snapshot before scanning.
            repository.invalidate()
            val outcome = engine.sync(remoteRoot)

            val lists = loadFileLists(repository, refresh = true)
            val bookmarks = state.bookmarks.filterTo(mutableSetOf()) { path ->
                lists.notes.any { it.path == path }
            }
            val tabs = mutableListOf<Tab>()
            for (tab in state.tabs) {
                if (lists.entries.none { it.path == tab.path }) continue
                if (tab.dirty) {
                    tabs.add(tab)
                } else {
                    val text = runCatching { repository.readNote(tab.path) }.getOrNull() ?: tab.text
                    tabs.add(
                        tab.copy(
                            text = text,
                            selection = tab.selection.coerceIn(0, text.length),
                        ),
                    )
                }
            }
            val active = if (tabs.isEmpty()) -1 else state.activeTab.coerceIn(0, tabs.lastIndex)
            state = state.copy(
                notes = lists.notes,
                entries = lists.entries,
                folders = lists.folders,
                explorerTree = lists.tree,
                bookmarks = bookmarks,
                tabs = tabs,
                activeTab = active,
                sync = state.sync.copy(
                    connected = true,
                    accountName = account,
                    busy = false,
                    progress = "",
                    lastSync = outcome.lastSync,
                    message = describe(outcome),
                ),
            )
            if (active >= 0) {
                refreshDerived(
                state.tabs[active].text,
                force = state.editorMode == EditorMode.Preview,
            )
            } else {
                state = state.copy(
                    headings = emptyList(),
                    stats = DocStats(0, 0, 0),
                    lines = emptyList(),
                    markdownText = "",
                )
            }
            persistTabs()
            persistBookmarks()
        } catch (error: Throwable) {
            error.rethrowIfCancelled()
            state = state.copy(
                sync = state.sync.copy(
                    busy = false,
                    progress = "",
                    message = error.message ?: "Sync failed",
                ),
            )
        }
    }

    private fun refreshSyncState() {
        val key = locatorKey()
        val stored = if (key.isEmpty()) null else syncRecords.state(key)
        state = state.copy(
            sync = SyncUiState(
                connected = dropboxTokens.connected,
                accountName = dropboxTokens.accountName,
                clientId = dropboxTokens.clientId,
                lastSync = stored?.lastSync ?: 0L,
                autoSync = prefs.getBoolean(KEY_SYNC_ON_OPEN, false),
            ),
        )
    }

    private fun openBrowser(url: String) {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        appContext.startActivity(intent)
    }

    private fun remoteRootFor(name: String): String {
        val clean = name.trim()
            .replace(Regex("[\\\\/:*?\"<>|]"), "_")
            .trim('_')
        return "/" + clean.ifEmpty { "vibecodium" }
    }

    private fun describe(outcome: SyncOutcome): String {
        val parts = mutableListOf("${outcome.uploaded} up", "${outcome.downloaded} down")
        if (outcome.conflicts > 0) parts.add("${outcome.conflicts} conflicts")
        val deleted = outcome.deletedLocal + outcome.deletedRemote
        if (deleted > 0) parts.add("$deleted deleted")
        if (outcome.skipped > 0) parts.add("${outcome.skipped} skipped")
        return "Synced: " + parts.joinToString(", ")
    }

    // --------------------------------------------------------------- overlays

    fun toggleDrawer(drawer: Drawer) {
        state = state.copy(
            drawer = if (state.drawer == drawer) null else drawer,
            tabSwitcher = false,
        )
    }

    fun setDrawerMode(mode: DrawerMode) {
        state = state.copy(drawer = Drawer.Left, drawerMode = mode, tabSwitcher = false)
    }

    fun toggleTabSwitcher() {
        state = state.copy(tabSwitcher = !state.tabSwitcher, drawer = null)
    }

    fun closeOverlays() {
        state = state.copy(drawer = null, tabSwitcher = false)
    }

    fun back() {
        if (state.settingsOpen) {
            state = state.copy(settingsOpen = false)
            return
        }
        if (state.syncOpen) {
            state = state.copy(syncOpen = false)
            return
        }
        if (state.viewerPath != null) {
            state = state.copy(viewerPath = null)
            return
        }
        if (state.tabSwitcher) {
            state = state.copy(tabSwitcher = false)
            return
        }
        if (state.drawer != null) {
            state = state.copy(drawer = null)
            return
        }
        if (state.historyIndex > 0) {
            val next = state.historyIndex - 1
            state = state.copy(historyIndex = next)
            openNote(state.history[next], pushHistory = false)
        }
    }

    fun forward() {
        if (state.historyIndex >= 0 && state.historyIndex < state.history.lastIndex) {
            val next = state.historyIndex + 1
            state = state.copy(historyIndex = next)
            openNote(state.history[next], pushHistory = false)
        }
    }

    private fun pushHistory(path: String) {
        val trimmed = state.history.take(state.historyIndex + 1)
        if (trimmed.lastOrNull() == path) return
        val history = trimmed + path
        state = state.copy(history = history, historyIndex = history.lastIndex)
    }

    // -------------------------------------------------------------- lifecycle

    fun closeVault() {
        flushBlocking()
        syncJob?.cancel()
        syncServer?.close()
        scope.launch {
            closeRepository()
            VaultLocatorStore.clear(appContext)
            state = UiState(settings = state.settings)
        }
    }

    /** Called from Activity.onStop: a small write on the main thread is cheaper
     *  than risking a lost edit when the process is killed. */
    fun flushBlocking() {
        val tab = state.tabs.getOrNull(state.activeTab)
        val repository = repository
        if (tab != null && tab.dirty && repository != null) {
            runBlocking { runCatching { repository.writeNote(tab.path, tab.text) } }
        }
        persistTabs()
    }

    fun showMessage(message: String) {
        state = state.copy(message = message)
    }

    fun dismissMessage() {
        state = state.copy(message = null)
    }

    fun dispose() {
        flushBlocking()
        syncJob?.cancel()
        syncServer?.close()
        scope.cancel()
        closeRepository()
    }

    // -------------------------------------------------------------- internals

    private fun locatorKey(): String = locator?.value ?: state.vaultRoot ?: ""

    private fun persistBookmarks() {
        val key = locatorKey()
        if (key.isNotEmpty()) {
            BookmarkStore.save(appContext, key, state.bookmarks)
        }
    }

    private fun persistTabs() {
        val key = locatorKey()
        if (key.isNotEmpty()) {
            val tabs = tabsWithUris(state.tabs)
            if (tabs != state.tabs) {
                state = state.copy(tabs = tabs)
            }
            TabStore.save(appContext, key, tabs, state.activeTab)
        }
    }

    private fun schedulePersistTabs() {
        tabPersistJob?.cancel()
        tabPersistJob = scope.launch {
            delay(400)
            persistTabs()
        }
    }

    private data class FileLists(
        val notes: List<NoteItem>,
        val entries: List<ExplorerEntry>,
        val folders: List<String>,
        val tree: List<ExplorerNode>,
    )

    private suspend fun loadFileLists(
        repository: VaultRepository,
        refresh: Boolean = false,
    ): FileLists {
        if (refresh) repository.invalidate()
        // One snapshot serves files and folders. For SAF it is also the cache
        // behind `listNotes`, so the vault is traversed only once.
        val snapshot = repository.snapshot()
        val notes = repository.listNotes()
        val entries = loadEntries(snapshot.files)
        val folders = snapshot.folders.filterNot { isServicePath(it) }
        val tree = withContext(Dispatchers.Default) { buildExplorerTree(entries, folders) }
        return FileLists(
            notes = notes,
            entries = entries,
            folders = folders,
            tree = tree,
        )
    }

    private fun loadEntries(files: List<VaultFile>): List<ExplorerEntry> =
        files
            .filterNot { isServicePath(it.path) }
            .map { file ->
                ExplorerEntry(
                    path = file.path,
                    name = file.path.substringAfterLast('/'),
                    isNote = isMarkdownName(file.path),
                    extension = file.path.substringAfterLast('.', "").lowercase(),
                    isCode = codeLanguageFor(file.path) != null,
                )
            }
            .sortedBy { it.path.lowercase() }

    private fun titleOf(path: String): String {
        val name = path.substringAfterLast('/')
        return name.substringBeforeLast('.', name)
    }

    private companion object {
        const val KEY_SYNC_ON_OPEN = "sync_on_open"
        val IMAGE_EXTENSIONS = setOf("png", "jpg", "jpeg", "gif", "webp", "bmp")
    }
}
