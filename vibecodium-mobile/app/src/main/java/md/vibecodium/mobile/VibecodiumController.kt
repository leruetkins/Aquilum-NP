package md.vibecodium.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.os.SystemClock
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
import java.time.LocalDateTime

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
    private var syncWakeLock: PowerManager.WakeLock? = null
    private var syncProgressAt = 0L

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
        state = state.copy(
            settings = SettingsStore.load(appContext),
            knownVaults = VaultListStore.load(appContext),
        )
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

    fun openVaultChooser() {
        closeExplorerActions()
        state = state.copy(
            vaultChooserOpen = true,
            drawer = null,
            tabSwitcher = false,
            settingsOpen = false,
            settingsSection = null,
        )
    }

    fun closeVaultChooser() {
        state = state.copy(vaultChooserOpen = false)
    }

    fun openKnownVault(vault: KnownVault) {
        val current = state.vaultRoot
        closeVaultChooser()
        if (current != null && vault.locator.value == current) return
        openLocator(vault.locator)
    }

    fun forgetKnownVault(vault: KnownVault) {
        state = state.copy(knownVaults = VaultListStore.forget(appContext, vault.locator))
    }

    fun clearRecent() {
        val key = locatorKey()
        if (key.isNotEmpty()) RecentStore.clear(appContext, key)
        state = state.copy(recentFiles = emptyList())
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
                    knownVaults = state.knownVaults,
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
                    knownVaults = state.knownVaults,
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
        val knownVaults = VaultListStore.remember(appContext, locator, repository.name)

        val collapsedFolders = FolderStore.load(appContext, locator.value)
        val bookmarks = BookmarkStore.load(appContext, locator.value)
        val vaultSync = VaultSyncStore.load(appContext, locator.value)
        val recentFiles = RecentStore.load(appContext, locator.value)
        // Publish the shell before touching the tree. The full snapshot is
        // filled in by [refreshLists] on IO; a large vault must not hold the
        // first frame behind a recursive walk.
        state = state.copy(
            vaultName = repository.name,
            vaultRoot = locator.value,
            vaultKind = if (repository.isScoped) "saf" else "file",
            knownVaults = knownVaults,
            notes = emptyList(),
            entries = emptyList(),
            explorerTree = emptyList(),
            folders = emptyList(),
            collapsedFolders = collapsedFolders,
            vaultSync = vaultSync,
            recentFiles = recentFiles,
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
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
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

    /** Returns true when at least one saved tab was reopened. Only the active document is loaded into memory. */
    private suspend fun restoreTabs(repository: VaultRepository): Boolean {
        val saved = TabStore.load(appContext, locatorKey()) ?: return false
        if (saved.tabs.isEmpty()) return false

        var activeIndex = saved.active.coerceIn(0, saved.tabs.lastIndex)
        var activeSaved = saved.tabs[activeIndex]

        // Load content ONLY for the active document.
        var activeText = activeSaved.uri?.let { repository.readDocument(it) }
            ?: runCatching { repository.readNote(activeSaved.path) }.getOrNull()

        // If the active tab could not be read, look for the first readable tab in saved tabs
        if (activeText == null) {
            for ((idx, candidate) in saved.tabs.withIndex()) {
                if (idx == activeIndex) continue
                val text = candidate.uri?.let { repository.readDocument(it) }
                    ?: runCatching { repository.readNote(candidate.path) }.getOrNull()
                if (text != null) {
                    activeIndex = idx
                    activeSaved = candidate
                    activeText = text
                    break
                }
            }
        }

        if (activeText == null) return false

        val tabs = saved.tabs.mapIndexed { index, savedTab ->
            val isActive = (index == activeIndex)
            val text = if (isActive) activeText else ""
            Tab(
                path = savedTab.path,
                title = titleOf(savedTab.path),
                text = text,
                selection = if (isActive) savedTab.selection.coerceIn(0, text.length) else savedTab.selection,
                language = codeLanguageFor(savedTab.path),
                mode = savedTab.mode,
                uri = savedTab.uri ?: repository.uriFor(savedTab.path),
                isLoaded = isActive,
            )
        }

        state = state.copy(
            tabs = tabs,
            activeTab = activeIndex,
            editorMode = tabs[activeIndex].mode,
            recentFiles = RecentStore.remember(appContext, locatorKey(), tabs[activeIndex].path),
            history = listOf(tabs[activeIndex].path),
            historyIndex = 0,
        )
        refreshDerived(tabs[activeIndex].text, force = tabs[activeIndex].mode == EditorMode.Preview)
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
            if (state.sync.autoSync && state.sync.connected && !state.vaultSync.paused) {
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

    fun openNote(
        path: String,
        pushHistory: Boolean = true,
        modeOverride: EditorMode? = null,
        focusEditor: Boolean = false,
    ) {
        val existing = state.tabs.indexOfFirst { it.path == path }
        if (existing >= 0) {
            selectTab(
                index = existing,
                modeOverride = modeOverride,
                focusEditor = focusEditor,
                pushHistory = pushHistory,
            )
            return
        }
        val repository = repository ?: return
        scope.launch {
            try {
                val text = repository.readNote(path)
                val mode = modeOverride ?: state.settings.defaultEditorMode
                val tab = Tab(
                    path = path,
                    title = titleOf(path),
                    text = text,
                    language = codeLanguageFor(path),
                    mode = mode,
                    uri = repository.uriFor(path),
                )
                val tabs = state.tabs + tab
                state = state.copy(
                    tabs = tabs,
                    activeTab = tabs.lastIndex,
                    drawer = null,
                    tabSwitcher = false,
                    viewerPath = null,
                    editorMode = mode,
                    recentFiles = RecentStore.remember(appContext, locatorKey(), path),
                    focusRequestId = if (focusEditor) state.focusRequestId + 1 else state.focusRequestId,
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

    /**
     * Opens today's note, creating it with the configured template when it does
     * not exist yet. The name format and folder mirror the desktop settings.
     */
    fun openDailyNote() {
        val repository = repository ?: return
        val daily = state.settings.dailyNotes
        if (!daily.enabled) return
        val path = dailyNotePath(daily, LocalDateTime.now())
        scope.launch {
            try {
                if (readNoteOrNull(repository, path) != null) {
                    openNote(path)
                    return@launch
                }
                val template = daily.template.trim()
                val text = if (template.isEmpty() || template == path) {
                    ""
                } else {
                    readNoteOrNull(repository, template) ?: ""
                }
                val note = repository.createNote(path, text)
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                )
                openNote(note.path, modeOverride = EditorMode.Source, focusEditor = true)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not open the daily note")
            }
        }
    }

    private suspend fun readNoteOrNull(repository: VaultRepository, path: String): String? =
        try {
            repository.readNote(path)
        } catch (error: Throwable) {
            error.rethrowIfCancelled()
            null
        }

    fun newNote() = createNoteUnder("", open = true)

    fun consumeFocusRequest() {
        if (state.focusRequestId != 0L) {
            state = state.copy(focusRequestId = 0L)
        }
    }

    fun updateEditor(value: TextFieldValue) {
        val index = state.activeTab
        if (index !in state.tabs.indices) return
        val currentTab = state.tabs[index]
        if (!currentTab.isLoaded) return
        val adjusted = autoIndent(currentTab.text, value)
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
        if (!tab.isLoaded) return
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
        val tab = state.tabs.getOrNull(index)
        if (tab == null || !tab.isLoaded) return
        scope.launch {
            persistTab(index)
            persistTabs()
        }
    }

    private suspend fun persistTab(index: Int) {
        val repository = repository ?: return
        val tab = state.tabs.getOrNull(index) ?: return
        if (!tab.dirty || !tab.isLoaded) return
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

    fun selectTab(
        index: Int,
        modeOverride: EditorMode? = null,
        focusEditor: Boolean = false,
        pushHistory: Boolean = false,
    ) {
        if (index !in state.tabs.indices) {
            state = state.copy(drawer = null, tabSwitcher = false)
            return
        }
        saveActiveNow()
        val target = state.tabs[index]
        val mode = modeOverride ?: target.mode
        val focusReq = if (focusEditor) state.focusRequestId + 1 else state.focusRequestId

        if (!target.isLoaded) {
            loadTabContent(index, modeOverride, focusEditor, pushHistory)
            return
        }

        val tabs = if (mode != target.mode) {
            val list = state.tabs.toMutableList()
            list[index] = target.copy(mode = mode)
            list
        } else {
            state.tabs
        }
        state = state.copy(
            tabs = tabs,
            activeTab = index,
            editorMode = mode,
            recentFiles = RecentStore.remember(appContext, locatorKey(), target.path),
            drawer = null,
            tabSwitcher = false,
            viewerPath = null,
            focusRequestId = focusReq,
        )
        if (pushHistory) pushHistory(target.path)
        scope.launch {
            refreshDerived(
                target.text,
                force = mode == EditorMode.Preview,
            )
        }
        persistTabs()
    }

    /** Loads an unloaded tab's body into memory and triggers outline/stats derivation. */
    private fun loadTabContent(
        index: Int,
        modeOverride: EditorMode? = null,
        focusEditor: Boolean = false,
        pushHistory: Boolean = false,
    ) {
        val repository = repository ?: return
        val target = state.tabs.getOrNull(index) ?: return
        val mode = modeOverride ?: target.mode
        val focusReq = if (focusEditor) state.focusRequestId + 1 else state.focusRequestId

        scope.launch {
            try {
                val text = target.uri?.let { repository.readDocument(it) }
                    ?: repository.readNote(target.path)
                val loaded = target.copy(
                    text = text,
                    selection = target.selection.coerceIn(0, text.length),
                    mode = mode,
                    isLoaded = true,
                )
                val tabs = state.tabs.toMutableList()
                val cur = tabs.indexOfFirst { it.path == target.path }
                if (cur >= 0) {
                    tabs[cur] = loaded
                    state = state.copy(
                        tabs = tabs,
                        activeTab = cur,
                        editorMode = mode,
                        recentFiles = RecentStore.remember(appContext, locatorKey(), loaded.path),
                        drawer = null,
                        tabSwitcher = false,
                        viewerPath = null,
                        focusRequestId = focusReq,
                    )
                    if (pushHistory) pushHistory(loaded.path)
                    refreshDerived(
                        loaded.text,
                        force = mode == EditorMode.Preview,
                    )
                    persistTabs()
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    message = error.message ?: "Could not read the note",
                    drawer = null,
                    tabSwitcher = false,
                )
            }
        }
    }

    fun closeTab(index: Int) {
        if (index !in state.tabs.indices) return
        saveJob?.cancel()
        val closing = state.tabs[index]
        if (closing.dirty && closing.isLoaded) {
            val repository = repository
            if (repository != null) {
                scope.launch {
                    runCatching { repository.writeNote(closing.path, closing.text) }
                }
            }
        }
        val active = removeTabState(index)
        if (active >= 0) {
            val target = state.tabs[active]
            if (!target.isLoaded) {
                loadTabContent(active)
            } else {
                val mode = target.mode
                state = state.copy(editorMode = mode)
                scope.launch {
                    refreshDerived(
                        target.text,
                        force = mode == EditorMode.Preview,
                    )
                }
            }
        } else {
            state = state.copy(headings = emptyList(), stats = DocStats(0, 0, 0), lines = emptyList(), markdownText = "")
        }
        // Keep the tab list open so several tabs can be closed in a row; only
        // leave it when there is nothing left to show.
        if (state.tabs.isEmpty()) state = state.copy(tabSwitcher = false)
        persistTabs()
    }

    /** Closes every open tab; dirty documents are saved first. */
    fun closeAllTabs() {
        if (state.tabs.isEmpty()) {
            state = state.copy(tabSwitcher = false)
            return
        }
        saveJob?.cancel()
        val dirty = state.tabs.filter { it.dirty && it.isLoaded }
        val repository = repository
        state = state.copy(
            tabs = emptyList(),
            activeTab = -1,
            tabSwitcher = false,
            headings = emptyList(),
            stats = DocStats(0, 0, 0),
            lines = emptyList(),
            markdownText = "",
        )
        persistTabs()
        if (repository != null && dirty.isNotEmpty()) {
            scope.launch {
                for (tab in dirty) {
                    runCatching { repository.writeNote(tab.path, tab.text) }
                }
            }
        }
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
        state = state.copy(tabs = tabs, activeTab = active)
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
        val isFolder = isFolderPath(path)
        // A folder delete also closes every tab below it.
        val affected = state.tabs.indices
            .filter { index ->
                val tab = state.tabs[index]
                tab.path == path || tab.path.startsWith("$path/")
            }
            .sortedDescending()
        state = state.copy(deletePath = null)
        scope.launch {
            try {
                repository.deleteEntry(path, isFolder)
                val syncKey = locatorKey()
                if (syncKey.isNotEmpty()) syncRecords.removeTree(syncKey, path)
                for (index in affected) removeTabState(index)
                val active = state.activeTab
                if (active in state.tabs.indices) {
                    val target = state.tabs[active]
                    if (!target.isLoaded) {
                        loadTabContent(active)
                    } else {
                        refreshDerived(
                            target.text,
                            force = state.editorMode == EditorMode.Preview,
                        )
                    }
                } else {
                    state = state.copy(
                        headings = emptyList(),
                        stats = DocStats(0, 0, 0),
                        lines = emptyList(),
                        markdownText = "",
                    )
                }
                val bookmarks = state.bookmarks.filterNot {
                    it == path || it.startsWith("$path/")
                }.toSet()
                val history = state.history.filterNot {
                    it == path || it.startsWith("$path/")
                }
                val collapsed = state.collapsedFolders.filterNot {
                    it == path || it.startsWith("$path/")
                }.toSet()
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    bookmarks = bookmarks,
                    history = history,
                    collapsedFolders = collapsed,
                    recentFiles = RecentStore.removeTree(appContext, syncKey, path),
                )
                persistTabs()
                persistBookmarks()
                persistCollapsedFolders(collapsed)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not delete")
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

    // ------------------------------------------------------------ explorer rename

    /** Starts inline rename of a tree row; works for folders and files. */
    fun beginExplorerRename(path: String, isFolder: Boolean) {
        if (path.isBlank()) return
        state = state.copy(
            explorerRenamePath = path,
            explorerRenameValue = path.substringAfterLast('/'),
            explorerRenameFolder = isFolder,
            // The row must be visible for the editor to sit on it.
            fileQuery = "",
        )
    }

    fun dismissExplorerRename() {
        state = state.copy(
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
        )
    }

    /** Closes every explorer popup (inline rename, action menu, move picker). */
    private fun closeExplorerActions() {
        state = state.copy(
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
            explorerMenuPath = null,
            movePath = null,
        )
    }

    /** Mirrors the inline editor text so a tap in the tree can commit it. */
    fun updateExplorerRenameValue(value: String) {
        if (state.explorerRenamePath == null) return
        state = state.copy(explorerRenameValue = value)
    }

    fun confirmExplorerRename(value: String? = null) {
        val oldPath = state.explorerRenamePath ?: return
        val repository = repository ?: return
        val isFolder = state.explorerRenameFolder
        val typed = (value ?: state.explorerRenameValue).trim()
        if (typed.isEmpty()) {
            dismissExplorerRename()
            return
        }
        if (typed == "." || typed == ".." || typed.contains('/') || typed.contains('\\')) {
            state = state.copy(message = if (isFolder) "Invalid folder name" else "Invalid note name")
            return
        }
        val parent = oldPath.substringBeforeLast('/', "")
        val fileName = if (isFolder) {
            typed
        } else {
            val oldExtension = oldPath.substringAfterLast('.', "")
            val newHasExtension = typed.substringAfterLast('.', "").isNotEmpty()
            when {
                isMarkdownName(oldPath) -> if (isMarkdownName(typed)) typed else "$typed.md"
                oldExtension.isNotEmpty() && !newHasExtension -> "$typed.$oldExtension"
                else -> typed
            }
        }
        val newPath = if (parent.isEmpty()) fileName else "$parent/$fileName"
        if (newPath == oldPath) {
            dismissExplorerRename()
            return
        }
        // Close the editor before the IO round trip; otherwise IME "done" and
        // a tap outside could submit the same rename twice.
        dismissExplorerRename()
        scope.launch {
            try {
                if (isFolder) {
                    repository.renameFolder(oldPath, newPath)
                } else {
                    repository.renameNote(oldPath, newPath)
                }
                applyPathChange(oldPath, newPath)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Rename failed")
            }
        }
    }

    /** Rewrites every path-bearing piece of UI state after a rename or move. */
    private suspend fun applyPathChange(oldPath: String, newPath: String) {
        val repository = repository ?: return
        val syncKey = locatorKey()
        if (syncKey.isNotEmpty()) syncRecords.renameTree(syncKey, oldPath, newPath)
        val tabs = state.tabs.map { tab ->
            if (tab.path == oldPath || tab.path.startsWith("$oldPath/")) {
                val path = renamePathUnder(tab.path, oldPath, newPath)
                tab.copy(path = path, title = titleOf(path), language = codeLanguageFor(path))
            } else {
                tab
            }
        }
        val bookmarks = state.bookmarks.mapTo(mutableSetOf()) {
            renamePathUnder(it, oldPath, newPath)
        }
        val history = state.history.map { renamePathUnder(it, oldPath, newPath) }
        val collapsed = state.collapsedFolders.mapTo(mutableSetOf()) {
            renamePathUnder(it, oldPath, newPath)
        }
        val lists = loadFileLists(repository)
        state = state.copy(
            tabs = tabs,
            notes = lists.notes,
            entries = lists.entries,
            folders = lists.folders,
            explorerTree = lists.tree,
            bookmarks = bookmarks,
            history = history,
            collapsedFolders = collapsed,
            recentFiles = RecentStore.renameTree(appContext, syncKey, oldPath, newPath),
        )
        persistTabs()
        persistBookmarks()
        persistCollapsedFolders(collapsed)
        val active = state.activeTab
        if (active in tabs.indices && tabs[active].isLoaded) {
            refreshDerived(
                tabs[active].text,
                force = tabs[active].mode == EditorMode.Preview,
            )
        }
    }

    // ------------------------------------------------------------ explorer menu

    /** Opens the action sheet for a tree row (long press). */
    fun openExplorerMenu(path: String) {
        if (path.isBlank()) return
        val known = isFolderPath(path) || state.entries.any { it.path == path }
        if (!known) return
        dismissExplorerRename()
        state = state.copy(explorerMenuPath = path, movePath = null)
    }

    fun dismissExplorerMenu() {
        state = state.copy(explorerMenuPath = null)
    }

    fun menuNewNote(path: String) = createNoteUnder(parentFolderOf(path), open = true)

    fun menuNewFolder(path: String) = createFolderUnder(parentFolderOf(path))

    fun menuRename(path: String) {
        state = state.copy(explorerMenuPath = null)
        beginExplorerRename(path, isFolderPath(path))
    }

    fun menuDuplicate(path: String) {
        val repository = repository ?: return
        val isFolder = isFolderPath(path)
        val used = (state.folders + state.entries.map { it.path }).toHashSet()
        val target = uniquePath(duplicatePath(path), used)
        state = state.copy(explorerMenuPath = null)
        scope.launch {
            try {
                repository.copyEntry(path, target, isFolder)
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    message = "Copied to ${target.substringAfterLast('/')}",
                )
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not copy")
            }
        }
    }

    fun menuCopyPath(path: String) {
        state = state.copy(explorerMenuPath = null)
        val clipboard = appContext.getSystemService(ClipboardManager::class.java)
        if (clipboard == null) {
            state = state.copy(message = "Clipboard is unavailable")
            return
        }
        clipboard.setPrimaryClip(ClipData.newPlainText("Vault path", path))
        state = state.copy(message = "Path copied")
    }

    fun menuMove(path: String) {
        state = state.copy(explorerMenuPath = null, movePath = path)
    }

    fun dismissMove() {
        state = state.copy(movePath = null)
    }

    fun moveTo(targetFolder: String) {
        val path = state.movePath ?: return
        val repository = repository ?: return
        val isFolder = isFolderPath(path)
        val parent = path.substringBeforeLast('/', "")
        if (parent == targetFolder) {
            dismissMove()
            return
        }
        if (isFolder && (targetFolder == path || targetFolder.startsWith("$path/"))) {
            state = state.copy(message = "Cannot move a folder into itself")
            return
        }
        val newPath = movePathTo(path, targetFolder)
        if (isFolderPath(newPath) || state.entries.any { it.path == newPath }) {
            state = state.copy(message = "Already exists: $newPath")
            return
        }
        state = state.copy(movePath = null)
        scope.launch {
            try {
                repository.moveEntry(path, newPath, isFolder)
                applyPathChange(path, newPath)
                expandAncestors(targetFolder)
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Move failed")
            }
        }
    }

    fun menuDelete(path: String) {
        state = state.copy(explorerMenuPath = null, deletePath = path)
    }

    private fun isFolderPath(path: String): Boolean = state.folders.contains(path)

    private fun parentFolderOf(path: String): String =
        if (isFolderPath(path)) path else path.substringBeforeLast('/', "")

    /** File and folder names directly inside [parent]. */
    private fun childNames(parent: String): Set<String> {
        val prefix = if (parent.isEmpty()) "" else "$parent/"
        val used = HashSet<String>()
        for (folder in state.folders) {
            if (folder.startsWith(prefix)) {
                val rest = folder.substring(prefix.length)
                if (rest.isNotEmpty() && '/' !in rest) used.add(rest)
            }
        }
        for (entry in state.entries) {
            if (entry.path.startsWith(prefix)) {
                val rest = entry.path.substring(prefix.length)
                if (rest.isNotEmpty() && '/' !in rest) used.add(rest)
            }
        }
        return used
    }

    private fun uniqueUntitled(parent: String, extension: String = ""): String {
        val used = childNames(parent)
        var counter = 0
        var stem = "Untitled"
        while ("$stem$extension" in used) {
            counter += 1
            stem = "Untitled $counter"
        }
        return "$stem$extension"
    }

    private fun expandAncestors(path: String) {
        if (path.isEmpty()) return
        val collapsed = state.collapsedFolders.toMutableSet()
        var current = path
        while (current.isNotEmpty()) {
            collapsed.remove(current)
            current = current.substringBeforeLast('/', "")
        }
        state = state.copy(collapsedFolders = collapsed)
        persistCollapsedFolders(collapsed)
    }

    /** Creates "Untitled.md" inside [parent] and opens it. */
    private fun createNoteUnder(parent: String, open: Boolean) {
        val repository = repository ?: return
        scope.launch {
            try {
                val name = uniqueUntitled(parent, ".md")
                val path = if (parent.isEmpty()) name else "$parent/$name"
                repository.createNote(path, "")
                val lists = loadFileLists(repository)
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                )
                if (open) {
                    openNote(path, modeOverride = EditorMode.Source, focusEditor = true)
                    closeOverlays()
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not create a note")
            }
        }
    }

    // ------------------------------------------------------------- new folder

    /**
     * Creates the folder first and then renames it inline, like Obsidian: the
     * tree always has a real row for the editor and an abandoned rename leaves
     * an "Untitled" folder instead of discarding the action.
     */
    fun beginNewFolder() = createFolderUnder("")

    private fun createFolderUnder(parent: String) {
        val repository = repository ?: return
        scope.launch {
            try {
                val name = uniqueUntitled(parent)
                val path = if (parent.isEmpty()) name else "$parent/$name"
                repository.createFolder(path)
                val lists = loadFileLists(repository)
                val collapsed = state.collapsedFolders.toMutableSet().also { set ->
                    var current = parent
                    while (current.isNotEmpty()) {
                        set.remove(current)
                        current = current.substringBeforeLast('/', "")
                    }
                }
                state = state.copy(
                    notes = lists.notes,
                    entries = lists.entries,
                    folders = lists.folders,
                    explorerTree = lists.tree,
                    collapsedFolders = collapsed,
                    drawer = Drawer.Left,
                    drawerMode = DrawerMode.Files,
                    fileQuery = "",
                    explorerRenamePath = path,
                    explorerRenameValue = name,
                    explorerRenameFolder = true,
                )
                persistCollapsedFolders(collapsed)
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
                    recentFiles = RecentStore.rename(appContext, locatorKey(), oldPath, newPath),
                    renamePath = null,
                    renameValue = "",
                )
                persistTabs()
                persistBookmarks()
                if (index >= 0 && index == state.activeTab && tabs[index].isLoaded) {
                    refreshDerived(
                        state.tabs[index].text,
                        force = state.tabs[index].mode == EditorMode.Preview,
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
        val index = state.activeTab
        if (index in state.tabs.indices) {
            val tabs = state.tabs.toMutableList()
            tabs[index] = tabs[index].copy(mode = mode)
            state = state.copy(tabs = tabs, editorMode = mode)
            persistTabs()
        } else {
            state = state.copy(editorMode = mode)
        }
        if (mode == EditorMode.Preview && previous != EditorMode.Preview) {
            val tab = state.tabs.getOrNull(index) ?: return
            if (tab.isLoaded && tab.language == null && tab.text.isNotBlank() && state.lines.isEmpty()) {
                scope.launch { refreshDerived(tab.text, force = true) }
            }
        }
    }

    suspend fun loadAttachment(target: String, notePath: String): ByteArray? =
        repository?.readAttachment(target, notePath)

    // -------------------------------------------------------------------- sync

    fun syncRemoteFolder(): String = remoteRootFor(state.vaultName ?: "vibecodium")

    // ---------------------------------------------------------------- settings

    fun openSettings(section: String? = null) {
        closeExplorerActions()
        state = state.copy(
            settingsOpen = true,
            settingsSection = section,
            drawer = null,
            tabSwitcher = false,
        )
    }

    fun closeSettings() {
        val active = if (state.tabs.isNotEmpty() && state.activeTab !in state.tabs.indices) {
            0
        } else {
            state.activeTab
        }
        state = state.copy(settingsOpen = false, settingsSection = null, activeTab = active)
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

    fun setDailyNotesEnabled(enabled: Boolean) = updateSettings {
        it.copy(dailyNotes = it.dailyNotes.copy(enabled = enabled))
    }

    fun setDailyNoteFormat(format: String) = updateSettings {
        it.copy(dailyNotes = it.dailyNotes.copy(format = format.trim()))
    }

    fun setDailyNoteFolder(folder: String) = updateSettings {
        it.copy(dailyNotes = it.dailyNotes.copy(folder = normalizeDailyNoteFolder(folder)))
    }

    fun setDailyNoteTemplate(path: String) = updateSettings {
        it.copy(dailyNotes = it.dailyNotes.copy(template = path.trim()))
    }

    fun resetSettings() {
        updateSettings { AppSettings() }
        showMessage("Settings reset")
    }

    fun resetCollapsedFolders() {
        state = state.copy(collapsedFolders = emptySet())
        persistCollapsedFolders(emptySet())
        showMessage("Folder state reset")
    }

    fun setSyncPaused(paused: Boolean) = updateVaultSync { it.copy(paused = paused) }

    fun setPreventSleep(enabled: Boolean) = updateVaultSync { it.copy(preventSleep = enabled) }

    fun setSyncDeviceName(name: String) = updateVaultSync { it.copy(deviceName = name.take(64)) }

    fun setSyncCategory(category: SyncFileCategory, enabled: Boolean) =
        updateVaultSync { it.withCategory(category, enabled) }

    fun addSyncExcludedFolder(folder: String) {
        val clean = folder.trim().replace('\\', '/').trim('/')
        if (clean.isEmpty()) return
        updateVaultSync {
            if (clean in it.excludedFolders) it else it.copy(excludedFolders = it.excludedFolders + clean)
        }
    }

    fun removeSyncExcludedFolder(folder: String) =
        updateVaultSync { it.copy(excludedFolders = it.excludedFolders - folder) }

    private inline fun updateVaultSync(transform: (VaultSyncSettings) -> VaultSyncSettings) {
        val next = transform(state.vaultSync)
        state = state.copy(vaultSync = next)
        val key = locatorKey()
        if (key.isNotEmpty()) VaultSyncStore.save(appContext, key, next)
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
                if (state.vaultSync.paused) {
                    state = state.copy(
                        sync = state.sync.copy(message = "Connected to Dropbox. Sync is paused"),
                    )
                } else {
                    runSync()
                }
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
        if (state.vaultSync.paused) {
            state = state.copy(sync = state.sync.copy(message = "Sync is paused"))
            return
        }
        if (!dropboxTokens.connected) {
            state = state.copy(sync = state.sync.copy(message = "Connect Dropbox first"))
            return
        }
        syncJob?.cancel()
        syncJob = scope.launch { runSync() }
    }

    /** The sidebar sync button: start, stop, or rescan when not configured. */
    fun refreshOrSync() {
        when {
            state.sync.busy -> cancelSync()
            state.sync.connected && !state.vaultSync.paused -> syncNow()
            else -> refreshNotes()
        }
    }

    fun cancelSync() {
        val job = syncJob ?: return
        if (!job.isActive) {
            syncJob = null
            return
        }
        job.cancel()
        syncJob = null
        releaseSyncWakeLock()
        state = state.copy(
            sync = state.sync.copy(
                busy = false,
                progress = "",
                message = "Sync cancelled",
            ),
        )
    }

    private suspend fun runSync() {
        val repository = repository ?: return
        val key = locatorKey()
        if (key.isEmpty()) return
        if (state.vaultSync.paused) {
            state = state.copy(sync = state.sync.copy(busy = false, progress = "", message = "Sync is paused"))
            return
        }
        acquireSyncWakeLock()
        syncProgressAt = 0L
        state = state.copy(
            sync = state.sync.copy(
                busy = true,
                progress = "Saving…",
                progressKind = SyncProgressKind.Preparing,
                progressDone = 0,
                progressTotal = 0,
                message = "",
                localFiles = 0,
                remoteFiles = 0,
                planned = 0,
            ),
        )
        try {
            saveJob?.cancel()
            for (index in state.tabs.indices) {
                persistTab(index)
            }
            val provider = DropboxProvider(dropboxTokens)
            val account = runCatching { provider.accountName() }.getOrDefault("Dropbox")
            val remoteRoot = remoteRootFor(state.vaultName ?: "vibecodium")
            val syncSettings = state.vaultSync
            val engine = SyncEngine(
                repository,
                key,
                syncRecords,
                provider,
                filter = SyncFilter(
                    allowImages = syncSettings.allowImages,
                    allowAudio = syncSettings.allowAudio,
                    allowVideo = syncSettings.allowVideo,
                    allowPdf = syncSettings.allowPdf,
                    allowOther = syncSettings.allowOther,
                    excludedFolders = syncSettings.excludedFolders,
                ),
            ) { progress ->
                val now = SystemClock.elapsedRealtime()
                val countsChanged = progress.localFiles != state.sync.localFiles ||
                    progress.remoteFiles != state.sync.remoteFiles ||
                    progress.planned != state.sync.planned
                val finished = progress.total == 0 || progress.done == progress.total
                if (countsChanged || finished || now - syncProgressAt >= 100) {
                    syncProgressAt = now
                    val hasTotal = progress.total > 0 &&
                        progress.kind != SyncProgressKind.Preparing
                    // Keep the last labelled counter through count-less phases
                    // (planning, listing) so the sidebar never reverts to a
                    // bare "Syncing…" after numbers have started.
                    val kind = if (progress.kind != SyncProgressKind.Preparing) {
                        progress.kind
                    } else {
                        state.sync.progressKind
                    }
                    state = state.copy(
                        sync = state.sync.copy(
                            progress = formatSyncProgress(progress),
                            progressKind = kind,
                            progressDone = if (hasTotal) progress.done else state.sync.progressDone,
                            progressTotal = if (hasTotal) progress.total else state.sync.progressTotal,
                            localFiles = progress.localFiles,
                            remoteFiles = progress.remoteFiles,
                            planned = progress.planned,
                        ),
                    )
                }
            }
            // One SAF walk feeds both the planner and the post-sync list; the
            // engine receives the snapshot instead of scanning a second time.
            val snapshot = repository.snapshot(refresh = true)
            val outcome = engine.sync(remoteRoot, snapshot.files)

            val lists = loadFileLists(repository, refresh = false)
            val bookmarks = state.bookmarks.filterTo(mutableSetOf()) { path ->
                lists.notes.any { it.path == path }
            }
            val tabs = mutableListOf<Tab>()
            for (tab in state.tabs) {
                val existsLocally = lists.entries.any { it.path == tab.path }
                if (!existsLocally && !tab.dirty) {
                    val readable = runCatching { repository.readNote(tab.path) }.isSuccess
                    if (!readable) continue
                }
                if (!tab.isLoaded) {
                    tabs.add(tab)
                } else if (tab.dirty) {
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
                editorMode = if (active >= 0) tabs[active].mode else state.editorMode,
                sync = state.sync.copy(
                    connected = true,
                    accountName = account,
                    busy = false,
                    progress = "",
                    lastSync = outcome.lastSync,
                    message = describe(outcome),
                ),
            )
            if (active >= 0 && state.tabs[active].isLoaded) {
                refreshDerived(
                state.tabs[active].text,
                force = state.tabs[active].mode == EditorMode.Preview,
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
        } finally {
            releaseSyncWakeLock()
        }
    }

    private fun acquireSyncWakeLock() {
        if (!state.vaultSync.preventSleep || syncWakeLock != null) return
        val manager = appContext.getSystemService(Context.POWER_SERVICE) as? PowerManager ?: return
        syncWakeLock = manager
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vibecodium:sync")
            .apply {
                setReferenceCounted(false)
                acquire(10 * 60 * 1000L)
            }
    }

    private fun releaseSyncWakeLock() {
        syncWakeLock?.let { lock ->
            if (lock.isHeld) lock.release()
        }
        syncWakeLock = null
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

    private fun formatSyncProgress(progress: SyncProgress): String = when {
        progress.total > 0 -> "${progress.phase} ${progress.done}/${progress.total}"
        progress.planned > 0 ->
            "Found ${progress.localFiles} local · ${progress.remoteFiles} remote · ${progress.planned} to sync"

        progress.remoteFiles > 0 ->
            "${progress.phase}: ${progress.localFiles} local · ${progress.remoteFiles} remote"

        progress.localFiles > 0 -> "${progress.phase}: ${progress.localFiles} local"
        else -> progress.phase
    }

    // --------------------------------------------------------------- overlays

    fun toggleDrawer(drawer: Drawer) {
        if (state.drawer != null) closeExplorerActions()
        state = state.copy(
            drawer = if (state.drawer == drawer) null else drawer,
            tabSwitcher = false,
        )
    }

    fun setDrawerMode(mode: DrawerMode) {
        closeExplorerActions()
        state = state.copy(drawer = Drawer.Left, drawerMode = mode, tabSwitcher = false)
    }

    fun toggleTabSwitcher() {
        closeExplorerActions()
        state = state.copy(tabSwitcher = !state.tabSwitcher, drawer = null)
    }

    fun closeOverlays() {
        closeExplorerActions()
        state = state.copy(drawer = null, tabSwitcher = false)
    }

    fun back() {
        if (state.vaultChooserOpen) {
            state = state.copy(vaultChooserOpen = false)
            return
        }
        if (state.settingsOpen) {
            state = state.copy(settingsOpen = false, settingsSection = null)
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
        if (state.explorerMenuPath != null) {
            dismissExplorerMenu()
            return
        }
        if (state.movePath != null) {
            dismissMove()
            return
        }
        if (state.explorerRenamePath != null) {
            dismissExplorerRename()
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
        releaseSyncWakeLock()
        scope.launch {
            closeRepository()
            VaultLocatorStore.clear(appContext)
            state = UiState(settings = state.settings, knownVaults = state.knownVaults)
        }
    }

    /** Called from Activity.onStop: a small write on the main thread is cheaper
     *  than risking a lost edit when the process is killed. */
    fun flushBlocking() {
        val tab = state.tabs.getOrNull(state.activeTab)
        val repository = repository
        if (tab != null && tab.dirty && tab.isLoaded && repository != null) {
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
        releaseSyncWakeLock()
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
