package md.vibecodium.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
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
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import java.io.File
import java.time.LocalDateTime

/** Hard cap on in-page matches, matching the desktop find bar. */
private const val MAX_FIND_MATCHES = 19_999

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
    private val themeManager by lazy { ThemeManager(appContext) }
    private val syncRecords by lazy { SyncDatabase(appContext) }
    private val dropboxTokens by lazy { DropboxTokenStore(appContext) }
    private var syncJob: Job? = null
    private var syncServer: LoopbackServer? = null

    var state by mutableStateOf(UiState())
        private set

    /**
     * True while a toolbar popup is open. Read by the shell so the bottom
     * seam strip appears in the same frame as the page cover, instead of
     * waiting for the popup window's first draw.
     */
    var popupCover by mutableStateOf(false)

    private var saveJob: Job? = null
    private var derivedJob: Job? = null
    private var searchJob: Job? = null
    private var findJob: Job? = null
    private var tabPersistJob: Job? = null
    private var themeJob: Job? = null
    private var nativePaletteKey: Pair<Boolean, ThemePalette>? = null
    private var syncWakeLock: PowerManager.WakeLock? = null
    private var syncProgressAt = 0L

    /**
     * Tab path whose first rendered frame closes the open drawer or tab list.
     * Until it arrives the editor surface still holds the previous note's
     * pixels, so closing earlier would flash that note for a couple of frames.
     */
    private var pendingFramePath: String? = null
    private var pendingFrameReady: (() -> Unit)? = null

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
        val settings = SettingsStore.load(appContext)
        val tablet = appContext.resources.configuration.smallestScreenWidthDp >= 600
        val lightConfig = if (settings.lightThemeName.isNotBlank()) {
            themeManager.readConfig(settings.lightThemeName, dark = false)
        } else {
            null
        }
        val darkConfig = if (settings.darkThemeName.isNotBlank()) {
            themeManager.readConfig(settings.darkThemeName, dark = true)
        } else {
            null
        }
        state = state.copy(
            settings = settings,
            knownVaults = VaultListStore.load(appContext),
            themeLightPalette = lightConfig
                ?.palette(dark = false, isTablet = tablet)
                ?.withAccent(settings.accentColor)
                ?: ThemePalette.defaultLight(),
            themeDarkPalette = darkConfig
                ?.palette(dark = true, isTablet = tablet)
                ?.withAccent(settings.accentColor)
                ?: ThemePalette.defaultDark(),
            themeWarnings = ThemeManager.collapseWarnings(
                lightConfig?.warnings.orEmpty() + darkConfig?.warnings.orEmpty(),
            ),
        )
        // Publish the parsed palette before the first Compose frame and before
        // the native surfaces attach. Only cold/missing configs need a parse.
        applyNativePalette()
        // Warm start: the configs already contain both palettes. Only rebuild
        // when a selected theme has no valid artifact yet; the installed list
        // is loaded lazily when Settings/Appearance opens.
        val needsThemeRebuild =
            (settings.lightThemeName.isNotBlank() && lightConfig == null) ||
                (settings.darkThemeName.isNotBlank() && darkConfig == null)
        if (needsThemeRebuild) refreshThemeState()
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

    fun removeRecent(path: String) {
        val key = locatorKey()
        if (key.isEmpty()) return
        val recent = RecentStore.forget(appContext, key, path)
        state = state.copy(recentFiles = recent)
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
            editorMode = state.settings.defaultEditorMode,
            lines = emptyList(),
            markdownText = "",
            headings = emptyList(),
            stats = DocStats(0, 0, 0),
            history = emptyList(),
            historyIndex = -1,
            searchQuery = "",
            searchResults = emptyList(),
            searchCollapsedPaths = emptySet(),
            find = FindUiState(),
            deletePath = null,
            viewerPath = null,
            renamePath = null,
            renameValue = "",
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
            openAfterRenamePath = null,
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
        /** Char offset to select after the tab's first frame, for search hits. */
        revealOffset: Int? = null,
        revealLength: Int = 0,
    ) {
        if (state.find.open) closeFindBar()
        val existing = state.tabs.indexOfFirst { it.path == path }
        if (existing >= 0) {
            val targetOffset = revealOffset
            selectTab(
                index = existing,
                modeOverride = modeOverride ?: if (targetOffset != null) EditorMode.Source else null,
                focusEditor = focusEditor,
                pushHistory = pushHistory,
                onReady = if (targetOffset != null) {
                    { activeEditorView?.revealOffset(targetOffset, revealLength) }
                } else {
                    null
                },
            )
            return
        }
        val repository = repository ?: return
        scope.launch {
            try {
                val text = repository.readNote(path)
                val mode = modeOverride
                    ?: if (revealOffset != null) EditorMode.Source
                    else state.settings.defaultEditorMode
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
                    // The drawer closes once the editor has drawn this note.
                    viewerPath = null,
                    editorMode = mode,
                    recentFiles = RecentStore.remember(appContext, locatorKey(), path),
                    focusRequestId = if (focusEditor) state.focusRequestId + 1 else state.focusRequestId,
                )
                awaitTabFrame(path) {
                    revealOffset?.let { activeEditorView?.revealOffset(it, revealLength) }
                }
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
        if (state.find.open && adjusted.text != currentTab.text) {
            scanFind()
        }
        schedulePersistTabs()
    }

    /** Text coming from the native SurfaceView editor. */
    /** The editor view currently on screen, used by the bottom toolbar. */
    private var activeEditorView: NativeEditorView? = null

    fun attachEditorView(view: NativeEditorView?) {
        activeEditorView = view
    }

    fun toggleActiveTableSource() {
        activeEditorView?.toggleTableSource()
    }

    fun undoEditor() {
        activeEditorView?.undo()
    }

    fun redoEditor() {
        activeEditorView?.redo()
    }

    fun setEditorHistoryState(canUndo: Boolean, canRedo: Boolean) {
        if (state.editorCanUndo == canUndo && state.editorCanRedo == canRedo) return
        state = state.copy(editorCanUndo = canUndo, editorCanRedo = canRedo)
    }

    fun updateNativeEditor(text: String, selection: Int) {        val index = state.activeTab
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
        /**
         * Runs once the switched tab's first frame is on screen. The tab list
         * keeps its popup open until then, so neither the previous note nor a
         * blank editor flashes between the switch and the new render.
         */
        onReady: (() -> Unit)? = null,
    ) {
        if (state.find.open) closeFindBar()
        if (index !in state.tabs.indices) {
            state = state.copy(drawer = null)
            onReady?.invoke()
            return
        }
        saveActiveNow()
        val target = state.tabs[index]
        val mode = modeOverride ?: target.mode
        val focusReq = if (focusEditor) state.focusRequestId + 1 else state.focusRequestId

        if (!target.isLoaded) {
            // Switch right away so the tab row turns active and the menu can
            // close; the body loads behind the editor spinner.
            state = state.copy(
                activeTab = index,
                editorMode = mode,
                recentFiles = RecentStore.remember(appContext, locatorKey(), target.path),
                viewerPath = null,
                focusRequestId = focusReq,
                drawer = null,
            )
            loadTabContent(index, modeOverride, focusEditor, pushHistory, onReady)
            return
        }

        val sameTab = index == state.activeTab
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
            // Kept open until the editor has drawn the new tab.
            drawer = if (sameTab) null else state.drawer,
            viewerPath = null,
            focusRequestId = focusReq,
        )
        if (sameTab) onReady?.invoke() else awaitTabFrame(target.path, onReady)
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
        onReady: (() -> Unit)? = null,
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
                    // The user may have picked another tab while this body was
                    // loading; only the still-active tab owns the editor state.
                    val stillActive = state.tabs.getOrNull(state.activeTab)?.path == loaded.path
                    state = if (stillActive) {
                        state.copy(
                            tabs = tabs,
                            activeTab = cur,
                            editorMode = mode,
                            recentFiles = RecentStore.remember(appContext, locatorKey(), loaded.path),
                            // Kept open until the editor has drawn the new tab.
                            viewerPath = null,
                            focusRequestId = focusReq,
                            drawer = null,
                        )
                    } else {
                        state.copy(tabs = tabs)
                    }
                    if (pushHistory) pushHistory(loaded.path)
                    if (stillActive) awaitTabFrame(loaded.path, onReady)
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
                )
                onReady?.invoke()
            }
        }
    }

    /** Called by the editor view once [path]'s first frame is on screen. */
    fun onTabFrameShown(path: String) {
        if (pendingFramePath != path) return
        finishPendingFrame()
    }

    private fun awaitTabFrame(path: String, onReady: (() -> Unit)?) {
        pendingFramePath = path
        pendingFrameReady = onReady
        // Fallback for surfaces that never draw (not attached yet): never
        // leave the drawer or the tab list hanging on a missing callback.
        scope.launch {
            delay(600)
            if (pendingFramePath == path) finishPendingFrame()
        }
    }

    private fun finishPendingFrame() {
        val ready = pendingFrameReady
        pendingFramePath = null
        pendingFrameReady = null
        state = state.copy(drawer = null)
        ready?.invoke()
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
        persistTabs()
    }

    /** Closes every open tab; dirty documents are saved first. */
    fun closeAllTabs() {
        if (state.tabs.isEmpty()) return
        saveJob?.cancel()
        val dirty = state.tabs.filter { it.dirty && it.isLoaded }
        val repository = repository
        state = state.copy(
            tabs = emptyList(),
            activeTab = -1,
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
            openAfterRenamePath = null,
            // The row must be visible for the editor to sit on it.
            fileQuery = "",
        )
    }

    fun dismissExplorerRename() {
        state = state.copy(
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
            openAfterRenamePath = null,
        )
    }

    /** Closes every explorer popup (inline rename, action menu, move picker). */
    private fun closeExplorerActions() {
        state = state.copy(
            explorerRenamePath = null,
            explorerRenameValue = "",
            explorerRenameFolder = false,
            openAfterRenamePath = null,
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
        // Captured before dismissing the editor, which clears it.
        val openAfter = state.openAfterRenamePath
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
            // The default name was accepted as is: go straight to the note.
            if (openAfter != null) openRenamedNote(openAfter)
            return
        }
        // Check the target against the tree before touching the disk: the
        // rename would otherwise fail only after the editor is gone, and the
        // SAF file path has no existence check of its own. Keeping the editor
        // open lets the user pick another name.
        if (state.entries.any { it.path == newPath } || newPath in state.folders) {
            state = state.copy(
                message = "Already exists: ${newPath.substringAfterLast('/')}",
            )
            return
        }
        // Close the editor before the IO round trip; otherwise IME "done" and
        // a tap outside could submit the same rename twice.
        dismissExplorerRename()
        // Paint the new name right away. Waiting for the disk round trip let
        // the row fall back to its old name for a moment and then re-sort,
        // which read as the whole tree rebuilding itself.
        applyExplorerRenameLocally(oldPath, newPath)
        scope.launch {
            try {
                if (isFolder) {
                    repository.renameFolder(oldPath, newPath)
                } else {
                    repository.renameNote(oldPath, newPath)
                }
                applyPathChange(oldPath, newPath)
                if (openAfter != null) openRenamedNote(renamePathUnder(openAfter, oldPath, newPath))
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                // Roll the optimistic rows back to what is actually on disk.
                val restored = runCatching { loadFileLists(repository, refresh = true) }.getOrNull()
                state = state.copy(
                    message = error.message ?: "Rename failed",
                    entries = restored?.entries ?: state.entries,
                    folders = restored?.folders ?: state.folders,
                    explorerTree = restored?.tree ?: state.explorerTree,
                    collapsedFolders = restored?.let { live ->
                        state.collapsedFolders.filterTo(mutableSetOf()) { it in live.folders }
                    } ?: state.collapsedFolders,
                )
            }
        }
    }

    /**
     * Rewrites the explorer rows before the rename reaches the disk. The tree
     * built here matches the one [applyPathChange] loads afterwards, so the
     * native list is repainted once instead of twice.
     */
    private fun applyExplorerRenameLocally(oldPath: String, newPath: String) {
        val entries = state.entries.map { entry ->
            val path = renamePathUnder(entry.path, oldPath, newPath)
            if (path == entry.path) {
                entry
            } else {
                entry.copy(
                    path = path,
                    name = path.substringAfterLast('/'),
                    isNote = isMarkdownName(path),
                    extension = path.substringAfterLast('.', "").lowercase(),
                    isCode = codeLanguageFor(path) != null,
                )
            }
        }
        val folders = state.folders.map { renamePathUnder(it, oldPath, newPath) }
        // Collapsed rows must follow the rename too, or a renamed folder
        // flashes open until the disk reload arrives.
        val collapsed = state.collapsedFolders.mapTo(mutableSetOf()) {
            renamePathUnder(it, oldPath, newPath)
        }
        state = state.copy(
            entries = entries,
            folders = folders,
            explorerTree = buildExplorerTree(entries, folders),
            collapsedFolders = collapsed,
        )
    }

    /**
     * Opens a note that was created from the explorer row menu once its inline
     * rename is done, so naming it is the last step before writing.
     */
    private fun openRenamedNote(path: String) {
        openNote(path, modeOverride = EditorMode.Source, focusEditor = true)
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

    fun menuNewNote(path: String) {
        // The sheet must go first: creating a note or a folder is async, and
        // the folder flow then opens the inline rename editor underneath it.
        state = state.copy(explorerMenuPath = null)
        // Rename before editing: the note stays closed until the user has
        // named it in the tree.
        createNoteUnder(parentFolderOf(path), open = false, renameInline = true)
    }

    fun menuNewFolder(path: String) {
        state = state.copy(explorerMenuPath = null)
        createFolderUnder(parentFolderOf(path))
    }

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

    /**
     * Creates "Untitled.md" inside [parent]. With [renameInline] the note is
     * not opened: the tree lands on its new row and starts the same inline
     * rename the new-folder flow uses, so the user can name it first.
     */
    private fun createNoteUnder(
        parent: String,
        open: Boolean,
        renameInline: Boolean = false,
    ) {
        val repository = repository ?: return
        scope.launch {
            try {
                val name = uniqueUntitled(parent, ".md")
                val path = if (parent.isEmpty()) name else "$parent/$name"
                repository.createNote(path, "")
                val lists = loadFileLists(repository)
                if (renameInline) {
                    val collapsed = expandedForNewEntry(parent)
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
                        explorerRenameFolder = false,
                        // Open it as soon as the name is confirmed.
                        openAfterRenamePath = path,
                    )
                    persistCollapsedFolders(collapsed)
                } else {
                    state = state.copy(
                        notes = lists.notes,
                        entries = lists.entries,
                        folders = lists.folders,
                        explorerTree = lists.tree,
                    )
                    if (open) {
                        // The drawer closes from openNote() once the note is loaded.
                        openNote(path, modeOverride = EditorMode.Source, focusEditor = true)
                    }
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Could not create a note")
            }
        }
    }

    /** Collapsed set with [parent] and its ancestors opened for a new row. */
    private fun expandedForNewEntry(parent: String): MutableSet<String> =
        state.collapsedFolders.toMutableSet().also { set ->
            var current = parent
            while (current.isNotEmpty()) {
                set.remove(current)
                current = current.substringBeforeLast('/', "")
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
                val collapsed = expandedForNewEntry(parent)
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
                    openAfterRenamePath = null,
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
            state = state.copy(searchResults = emptyList())
            return
        }
        searchJob = scope.launch {
            delay(200)
            try {
                val results = repository.search(query, SEARCH_FILE_LIMIT)
                if (state.searchQuery == query) {
                    state = state.copy(searchResults = results)
                }
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(message = error.message ?: "Search failed")
            }
        }
    }

    fun toggleSearchCollapsed(path: String) {
        val collapsed = state.searchCollapsedPaths
        state = state.copy(
            searchCollapsedPaths = if (path in collapsed) collapsed - path else collapsed + path,
        )
    }

    fun setAllSearchCollapsed(collapsed: Boolean) {
        state = state.copy(
            searchCollapsedPaths = if (collapsed) {
                state.searchResults.mapTo(HashSet()) { it.path }
            } else {
                emptySet()
            },
        )
    }

    /** Opens a search result and reveals its first match when it has one. */
    fun openSearchResult(result: SearchFileResult, match: SearchMatch?) {
        openNote(
            path = result.path,
            modeOverride = EditorMode.Source,
            revealOffset = match?.offset,
            revealLength = match?.length ?: 0,
        )
    }

    // ------------------------------------------------------------- in-page find

    fun openFindBar(replace: Boolean = false) {
        val source = state.editorMode == EditorMode.Source
        state = state.copy(
            find = state.find.copy(
                open = true,
                replaceOpen = replace && source,
            ),
        )
        scanFind()
    }

    fun closeFindBar() {
        findJob?.cancel()
        findJob = null
        activeEditorView?.clearFindHighlights()
        state = state.copy(find = FindUiState())
        activeEditorView?.focusEditor()
    }

    fun updateFindQuery(value: String) {
        state = state.copy(find = state.find.copy(query = value))
        scanFind()
    }

    fun updateFindReplace(value: String) {
        state = state.copy(find = state.find.copy(replace = value))
    }

    fun toggleFindCase() {
        state = state.copy(find = state.find.copy(caseSensitive = !state.find.caseSensitive))
        scanFind()
    }

    fun toggleFindWholeWord() {
        state = state.copy(find = state.find.copy(wholeWord = !state.find.wholeWord))
        scanFind()
    }

    fun findNext() = stepFind(1)

    fun findPrevious() = stepFind(-1)

    fun toggleFindReplace() {
        if (state.editorMode != EditorMode.Source) return
        state = state.copy(find = state.find.copy(replaceOpen = !state.find.replaceOpen))
    }

    private fun stepFind(delta: Int) {
        val find = state.find
        if (find.matches.isEmpty()) return
        val size = find.matches.size
        val next = Math.floorMod(find.active + delta, size)
        state = state.copy(find = find.copy(active = next))
        applyFindActive(reveal = true)
    }

    private fun scanFind() {
        findJob?.cancel()
        val find = state.find
        if (!find.open) return
        val tab = state.tabs.getOrNull(state.activeTab) ?: return
        val path = tab.path
        val text = tab.text
        val query = find.query
        if (query.isEmpty()) {
            activeEditorView?.clearFindHighlights()
            state = state.copy(
                find = find.copy(
                    matches = emptyList(),
                    active = -1,
                    truncated = false,
                    scanning = false,
                ),
            )
            return
        }
        state = state.copy(find = find.copy(scanning = true))
        findJob = scope.launch {
            val result = withContext(Dispatchers.Default) {
                scanFindText(
                    text = text,
                    query = query,
                    caseSensitive = find.caseSensitive,
                    wholeWord = find.wholeWord,
                )
            }
            val current = state.find
            val currentTab = state.tabs.getOrNull(state.activeTab)
            if (currentTab?.path != path || current.query != query ||
                current.caseSensitive != find.caseSensitive ||
                current.wholeWord != find.wholeWord
            ) {
                return@launch
            }
            state = state.copy(
                find = current.copy(
                    matches = result.first,
                    active = if (result.first.isEmpty()) -1 else 0,
                    truncated = result.second,
                    scanning = false,
                ),
            )
            applyFindActive(reveal = true)
        }
    }

    private fun applyFindActive(reveal: Boolean) {
        val find = state.find
        val match = find.matches.getOrNull(find.active)
        val view = activeEditorView
        if (match == null) {
            view?.clearFindHighlights()
            return
        }
        view?.setFindHighlights(find.matches.map { it.start until it.end }, find.active)
        if (reveal) {
            view?.revealOffset(match.start, match.end - match.start, focusEditor = false)
        }
    }

    fun replaceCurrentFind() {
        if (state.editorMode != EditorMode.Source || state.find.scanning) return
        val find = state.find
        val match = find.matches.getOrNull(find.active) ?: return
        activeEditorView?.replaceRange(match.start, match.end, find.replace)
        scanFind()
    }

    fun replaceAllFind() {
        if (state.editorMode != EditorMode.Source || state.find.scanning) return
        val find = state.find
        val tab = state.tabs.getOrNull(state.activeTab) ?: return
        if (find.matches.isEmpty()) return
        val text = tab.text
        val replacement = find.replace
        val builder = StringBuilder(text.length)
        var cursor = 0
        var replaced = 0
        for (match in find.matches) {
            if (match.start < cursor) continue
            builder.append(text, cursor, match.start)
            builder.append(replacement)
            cursor = match.end
            replaced++
        }
        builder.append(text, cursor, text.length)
        if (replaced == 0) return
        updateEditor(TextFieldValue(builder.toString(), TextRange(0)))
        scanFind()
    }

    private suspend fun scanFindText(
        text: String,
        query: String,
        caseSensitive: Boolean,
        wholeWord: Boolean,
    ): Pair<List<FindMatch>, Boolean> {
        if (query.isEmpty() || text.length < query.length) {
            return emptyList<FindMatch>() to false
        }
        val lineStarts = ArrayList<Int>()
        lineStarts.add(0)
        for (index in text.indices) {
            if (text[index] == '\n') lineStarts.add(index + 1)
        }
        fun lineAt(offset: Int): Int {
            val found = lineStarts.binarySearch(offset)
            return (if (found >= 0) found else -found - 2).coerceAtLeast(0) + 1
        }
        fun wordChar(character: Char): Boolean =
            character.isLetterOrDigit() || character == '_'

        val matches = ArrayList<FindMatch>()
        var search = 0
        var truncated = false
        while (search <= text.length - query.length) {
            if (matches.size % 256 == 0) yield()
            val start = text.indexOf(query, search, ignoreCase = !caseSensitive)
            if (start < 0) break
            val end = start + query.length
            val wholeWordBlocked = wholeWord && (
                (start > 0 && wordChar(text[start - 1])) ||
                    (end < text.length && wordChar(text[end]))
                )
            if (wholeWordBlocked) {
                search = start + 1
                continue
            }
            matches.add(FindMatch(start, end, lineAt(start)))
            if (matches.size >= MAX_FIND_MATCHES) {
                truncated = true
                break
            }
            search = end
        }
        return matches to truncated
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
        refreshInstalledThemes()
        state = state.copy(
            settingsOpen = true,
            settingsSection = section,
            drawer = null,
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

    fun setEditorFontSize(fontDp: Int) = updateSettings {
        it.copy(
            editorFontDp = fontDp.coerceIn(
                AppSettingsJson.MIN_EDITOR_FONT_DP,
                AppSettingsJson.MAX_EDITOR_FONT_DP,
            ),
        )
    }

    fun setAutoHideChrome(enabled: Boolean) = updateSettings { it.copy(autoHideChrome = enabled) }

    fun setCodeFontSize(fontDp: Int) = updateSettings {
        it.copy(
            codeFontDp = fontDp.coerceIn(
                AppSettingsJson.MIN_CODE_FONT_DP,
                AppSettingsJson.MAX_CODE_FONT_DP,
            ),
        )
    }

    fun setTextFontFace(face: EditorFontFace) = updateSettings { it.copy(textFontFace = face) }

    fun setCodeFontFace(face: EditorFontFace) = updateSettings { it.copy(codeFontFace = face) }

    fun setHeadingFontSize(level: Int, size: Int) = updateHeading(level) {
        it.copy(fontSize = size.coerceIn(6, 64))
    }

    fun setHeadingColor(level: Int, colorName: String) = updateHeading(level) {
        it.copy(colorName = colorName)
    }

    fun toggleHeadingBold(level: Int) = updateHeading(level) { it.copy(bold = !it.bold) }

    fun toggleHeadingItalic(level: Int) = updateHeading(level) { it.copy(italic = !it.italic) }

    private inline fun updateHeading(level: Int, transform: (HeadingStyle) -> HeadingStyle) {
        updateSettings {
            it.copy(headings = it.headings.with(level, transform(it.headings.style(level))))
        }
    }

    fun setNavbarEnabled(enabled: Boolean) = updateSettings { it.copy(navbarEnabled = enabled) }

    fun setDefaultEditorMode(mode: EditorMode) = updateSettings { it.copy(defaultEditorMode = mode) }

    fun setAutoSaveDelay(millis: Int) = updateSettings { it.copy(autoSaveDelayMs = millis) }

    fun setHighlightCurrentLine(enabled: Boolean) = updateSettings {
        it.copy(highlightCurrentLine = enabled)
    }

    fun setHighlightWrappedLineInGutter(enabled: Boolean) = updateSettings {
        it.copy(highlightWrappedLineInGutter = enabled)
    }

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
        refreshThemeState()
        showMessage("Settings reset")
    }

    // ------------------------------------------------------------- appearance

    fun setThemeMode(mode: ThemeMode) {
        updateSettings { it.copy(themeMode = mode) }
        refreshThemeState()
    }

    /** Flips between explicit light and dark modes using the two theme slots. */
    fun toggleThemeMode() {
        setThemeMode(if (effectiveDark()) ThemeMode.Light else ThemeMode.Dark)
    }

    fun setThemeName(slot: ThemeSlot, name: String) {
        val clean = state.installedThemes.firstOrNull { it.name == name }?.name ?: ""
        updateSettings {
            when (slot) {
                ThemeSlot.Light -> it.copy(lightThemeName = clean)
                ThemeSlot.Dark -> it.copy(darkThemeName = clean)
            }
        }
        refreshThemeState()
    }

    fun setAccentColor(argb: Int?) {
        updateSettings { it.copy(accentColor = argb) }
        refreshThemeState()
    }

    fun clearThemeError() {
        if (state.themeError != null) state = state.copy(themeError = null)
    }

    fun refreshThemeState(loadInstalled: Boolean = false) {
        val settings = state.settings
        val configuration = appContext.resources.configuration
        val tablet = configuration.smallestScreenWidthDp >= 600
        themeJob?.cancel()
        themeJob = scope.launch {
            val lightDeferred = async(Dispatchers.IO) {
                themeManager.ensureConfig(settings.lightThemeName, dark = false)
            }
            val darkDeferred = async(Dispatchers.IO) {
                themeManager.ensureConfig(settings.darkThemeName, dark = true)
            }
            val lightConfig = lightDeferred.await()
            val darkConfig = darkDeferred.await()
            val installed = if (loadInstalled) {
                withContext(Dispatchers.IO) { themeManager.installed() }
            } else {
                state.installedThemes
            }
            state = state.copy(
                themeLightPalette = lightConfig
                    .palette(dark = false, isTablet = tablet)
                    .withAccent(settings.accentColor),
                themeDarkPalette = darkConfig
                    .palette(dark = true, isTablet = tablet)
                    .withAccent(settings.accentColor),
                themeWarnings = ThemeManager.collapseWarnings(
                    lightConfig.warnings + darkConfig.warnings,
                ),
                installedThemes = installed,
            )
            applyNativePalette()
        }
    }

    fun refreshInstalledThemes() {
        scope.launch {
            val installed = withContext(Dispatchers.IO) { themeManager.installed() }
            state = state.copy(installedThemes = installed)
        }
    }

    fun loadThemeCatalog(force: Boolean = false) {
        if (state.themeCatalogLoading) return
        state = state.copy(themeCatalogLoading = true, themeError = null)
        scope.launch {
            try {
                val entries = themeManager.catalog(force)
                val installed = withContext(Dispatchers.IO) { themeManager.installed() }
                val installedByName = installed.associateBy { it.name }
                val updates = state.themeUpdates
                val merged = entries.map { entry ->
                    entry.copy(
                        installedVersion = installedByName[entry.name]?.version,
                        updateVersion = updates[entry.name],
                    )
                }
                state = state.copy(
                    themeCatalog = merged,
                    installedThemes = installed,
                    themeCatalogLoading = false,
                )
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    themeCatalogLoading = false,
                    themeError = error.message ?: "Could not load community themes",
                )
            }
        }
    }

    fun installTheme(name: String, slot: ThemeSlot) {
        if (state.themeBusy != null) return
        state = state.copy(themeError = null)
        scope.launch {
            try {
                val entry = state.themeCatalog.firstOrNull { it.name == name }
                    ?: themeManager.catalog(false).firstOrNull { it.name == name }
                    ?: error("Theme “$name” was not found in the catalog")
                state = state.copy(themeBusy = name, themeError = null)
                val theme = withContext(Dispatchers.IO) { themeManager.install(entry) }
                val installed = withContext(Dispatchers.IO) { themeManager.installed() }
                updateSettings {
                    when (slot) {
                        ThemeSlot.Light -> it.copy(lightThemeName = entry.name)
                        ThemeSlot.Dark -> it.copy(darkThemeName = entry.name)
                    }
                }
                state = state.copy(
                    installedThemes = installed,
                    themeBusy = null,
                    themeUpdates = state.themeUpdates - entry.name,
                    themeCatalog = state.themeCatalog.map { item ->
                        if (item.name == entry.name) {
                            item.copy(installedVersion = theme.version, updateVersion = null)
                        } else {
                            item
                        }
                    },
                )
                refreshThemeState()
                showMessage("Installed theme “${entry.name}”")
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    themeBusy = null,
                    themeError = error.message ?: "Could not install the theme",
                )
            }
        }
    }

    fun uninstallTheme(name: String) {
        if (state.themeBusy != null) return
        scope.launch {
            try {
                state = state.copy(themeBusy = name, themeError = null)
                withContext(Dispatchers.IO) { themeManager.remove(name) }
                val installed = withContext(Dispatchers.IO) { themeManager.installed() }
                if (state.settings.lightThemeName == name || state.settings.darkThemeName == name) {
                    updateSettings {
                        it.copy(
                            lightThemeName = if (it.lightThemeName == name) "" else it.lightThemeName,
                            darkThemeName = if (it.darkThemeName == name) "" else it.darkThemeName,
                        )
                    }
                }
                state = state.copy(
                    installedThemes = installed,
                    themeBusy = null,
                    themeUpdates = state.themeUpdates - name,
                    themeCatalog = state.themeCatalog.map { item ->
                        if (item.name == name) item.copy(installedVersion = null, updateVersion = null) else item
                    },
                )
                refreshThemeState()
                showMessage("Removed theme “$name”")
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    themeBusy = null,
                    themeError = error.message ?: "Could not remove the theme",
                )
            }
        }
    }

    fun checkThemeUpdates() {
        if (state.themeCheckingUpdates || state.installedThemes.isEmpty()) return
        state = state.copy(themeCheckingUpdates = true, themeError = null)
        scope.launch {
            try {
                val entries = state.themeCatalog.ifEmpty { themeManager.catalog(false) }
                val installed = withContext(Dispatchers.IO) { themeManager.installed() }
                val updates = themeManager.updates(installed, entries)
                state = state.copy(
                    installedThemes = installed,
                    themeCatalog = entries.map { entry ->
                        entry.copy(
                            installedVersion = installed.firstOrNull { it.name == entry.name }?.version,
                            updateVersion = updates[entry.name],
                        )
                    },
                    themeUpdates = updates,
                    themeCheckingUpdates = false,
                )
                showMessage(
                    if (updates.isEmpty()) "No theme updates found"
                    else "Found ${updates.size} theme update${if (updates.size == 1) "" else "s"}",
                )
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    themeCheckingUpdates = false,
                    themeError = error.message ?: "Could not check for theme updates",
                )
            }
        }
    }

    fun updateTheme(name: String) {
        if (state.themeBusy != null) return
        val entry = state.themeCatalog.firstOrNull { it.name == name } ?: return
        state = state.copy(themeBusy = name, themeError = null)
        scope.launch {
            try {
                val theme = withContext(Dispatchers.IO) { themeManager.install(entry) }
                val installed = withContext(Dispatchers.IO) { themeManager.installed() }
                state = state.copy(
                    installedThemes = installed,
                    themeBusy = null,
                    themeUpdates = state.themeUpdates - name,
                    themeCatalog = state.themeCatalog.map { item ->
                        if (item.name == name) {
                            item.copy(installedVersion = theme.version, updateVersion = null)
                        } else {
                            item
                        }
                    },
                )
                if (state.settings.lightThemeName == name || state.settings.darkThemeName == name) {
                    refreshThemeState()
                }
                showMessage("Updated theme “$name”")
            } catch (error: Throwable) {
                error.rethrowIfCancelled()
                state = state.copy(
                    themeBusy = null,
                    themeError = error.message ?: "Could not update the theme",
                )
            }
        }
    }

    fun applyNativePalette(dark: Boolean = effectiveDark()) {
        val palette = if (dark) state.themeDarkPalette else state.themeLightPalette
        val previous = nativePaletteKey
        if (previous != null && previous.first == dark && previous.second === palette) return
        nativePaletteKey = dark to palette
        runCatching { CoreBridge.nativeThemeSetPalette(palette.toNativeJson()) }
        NativeSurfaceView.refreshTheme()
    }

    private fun effectiveDark(): Boolean = when (state.settings.themeMode) {
        ThemeMode.Dark -> true
        ThemeMode.Light -> false
        ThemeMode.System -> {
            val mode = appContext.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
            mode == Configuration.UI_MODE_NIGHT_YES
        }
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
        )
    }

    fun setDrawerMode(mode: DrawerMode) {
        closeExplorerActions()
        state = state.copy(drawer = Drawer.Left, drawerMode = mode)
    }

    /** Closes the outline and centers the active editor on a heading line. */
    fun revealHeading(heading: Heading) {
        val tab = state.tabs.getOrNull(state.activeTab) ?: return
        var offset = 0
        var line = 0
        val targetLine = (heading.line - 1).coerceAtLeast(0)
        while (line < targetLine) {
            val newline = tab.text.indexOf('\n', offset)
            if (newline < 0) break
            offset = newline + 1
            line++
        }
        closeOverlays()
        activeEditorView?.revealOffset(offset, 0, focusEditor = false)
    }

    fun closeOverlays() {
        closeExplorerActions()
        pendingFramePath = null
        pendingFrameReady = null
        state = state.copy(drawer = null)
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
