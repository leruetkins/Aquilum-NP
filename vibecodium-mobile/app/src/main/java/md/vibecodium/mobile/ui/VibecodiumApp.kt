package md.vibecodium.mobile.ui

import android.graphics.BitmapFactory
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.invisibleToUser
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlin.math.roundToInt
import androidx.compose.ui.viewinterop.AndroidView
import md.vibecodium.mobile.AppSettingsJson
import md.vibecodium.mobile.CoreBridge
import md.vibecodium.mobile.CoreJson
import md.vibecodium.mobile.Drawer
import md.vibecodium.mobile.DrawerMode
import md.vibecodium.mobile.KnownVault
import md.vibecodium.mobile.VaultLocator
import md.vibecodium.mobile.DocStats
import md.vibecodium.mobile.EditorMode
import md.vibecodium.mobile.CodeColors
import md.vibecodium.mobile.LocalCodeColors
import md.vibecodium.mobile.LIVE_MODEL_CHAR_LIMIT
import md.vibecodium.mobile.MarkdownLine
import md.vibecodium.mobile.MarkdownSpan
import org.json.JSONArray
import org.json.JSONObject
import md.vibecodium.mobile.NativeEditorView
import md.vibecodium.mobile.NativeTreeView
import md.vibecodium.mobile.NoteItem
import md.vibecodium.mobile.SyncFileCategory
import md.vibecodium.mobile.Tab
import md.vibecodium.mobile.UiState
import md.vibecodium.mobile.VibecodiumController
import md.vibecodium.mobile.filterExplorerTree
import md.vibecodium.mobile.flattenExplorerTree
import md.vibecodium.mobile.indentAtCursor
import md.vibecodium.mobile.outdentAtCursor
import kotlinx.coroutines.flow.collect
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** `--navbar-height: var(--touch-size-l)` in the Obsidian phone stylesheet. */
private val NavbarHeight = 52.dp

/** `--view-header-height: var(--touch-size-m)` on a phone. */
private val ViewHeaderHeight = 44.dp

/** Top margin for the floating header pills so they hover below the status bar. */
private val ViewHeaderTopPadding = 8.dp

/** `lucide-folder-plus`, Obsidian's new-folder nav action. */
private val NewFolderIcon: ImageVector = ImageVector.Builder(
    name = "folder-plus",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(3f, 18f)
        lineTo(3f, 5f)
        curveTo(3f, 3.9f, 3.9f, 3f, 5f, 3f)
        lineTo(7.9f, 3f)
        curveTo(8.6f, 3f, 9.2f, 3.3f, 9.6f, 3.9f)
        lineTo(10.4f, 5.1f)
        curveTo(10.8f, 5.7f, 11.4f, 6f, 12.1f, 6f)
        lineTo(19f, 6f)
        curveTo(20.1f, 6f, 21f, 6.9f, 21f, 8f)
        lineTo(21f, 18f)
        curveTo(21f, 19.1f, 20.1f, 20f, 19f, 20f)
        lineTo(5f, 20f)
        curveTo(3.9f, 20f, 3f, 19.1f, 3f, 18f)
        close()
    }
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(12f, 10f)
        lineTo(12f, 16f)
        moveTo(9f, 13f)
        lineTo(15f, 13f)
    }
}.build()

/** `lucide-edit`, Obsidian's "new note" nav action. */
private val NewNoteIcon: ImageVector = ImageVector.Builder(
    name = "new-note",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(12f, 3f)
        lineTo(5f, 3f)
        curveTo(3.9f, 3f, 3f, 3.9f, 3f, 5f)
        lineTo(3f, 19f)
        curveTo(3f, 20.1f, 3.9f, 21f, 5f, 21f)
        lineTo(19f, 21f)
        curveTo(20.1f, 21f, 21f, 20.1f, 21f, 19f)
        lineTo(21f, 12f)
    }
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(18.4f, 2.6f)
        curveTo(19.2f, 1.8f, 20.4f, 1.8f, 21.2f, 2.6f)
        curveTo(22f, 3.4f, 22f, 4.6f, 21.2f, 5.4f)
        lineTo(12.2f, 14.4f)
        curveTo(12f, 14.6f, 11.7f, 14.8f, 11.4f, 14.9f)
        lineTo(8.5f, 15.7f)
        lineTo(9.3f, 12.8f)
        curveTo(9.4f, 12.5f, 9.6f, 12.2f, 9.8f, 12f)
        close()
    }
}.build()

/** `lucide-chevrons-up-down`, Obsidian's collapse/expand-all action. */
private val CollapseAllIcon: ImageVector = ImageVector.Builder(
    name = "chevrons-up-down",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(7f, 15f)
        lineTo(12f, 20f)
        lineTo(17f, 15f)
        moveTo(7f, 9f)
        lineTo(12f, 4f)
        lineTo(17f, 9f)
    }
}.build()

/** Material `sync`: two circular arrows. */
private val SyncArrowsIcon: ImageVector = ImageVector.Builder(
    name = "sync",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = SolidColor(Color.Black)) {
        moveTo(12f, 4f)
        verticalLineTo(1f)
        lineTo(8f, 5f)
        lineTo(12f, 9f)
        verticalLineTo(6f)
        curveTo(15.31f, 6f, 18f, 8.69f, 18f, 12f)
        curveTo(18f, 13.01f, 17.75f, 13.97f, 17.3f, 14.8f)
        lineTo(18.76f, 16.26f)
        curveTo(19.54f, 15.03f, 20f, 13.57f, 20f, 12f)
        curveTo(20f, 7.58f, 16.42f, 4f, 12f, 4f)
        close()
        moveTo(12f, 18f)
        curveTo(8.69f, 18f, 6f, 15.31f, 6f, 12f)
        curveTo(6f, 10.99f, 6.25f, 10.03f, 6.7f, 9.2f)
        lineTo(5.24f, 7.74f)
        curveTo(4.46f, 8.97f, 4f, 10.43f, 4f, 12f)
        curveTo(4f, 16.42f, 7.58f, 20f, 12f, 20f)
        verticalLineTo(23f)
        lineTo(16f, 19f)
        lineTo(12f, 15f)
        close()
    }
}.build()

/** `lucide-history`, the desktop client's recent-files icon. */
private val HistoryIcon: ImageVector = ImageVector.Builder(
    name = "history",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(3f, 12f)
        arcTo(9f, 9f, 0f, true, false, 12f, 3f)
        arcTo(9.75f, 9.75f, 0f, false, false, 5.26f, 5.74f)
        lineTo(3f, 8f)
        moveTo(3f, 3f)
        lineTo(3f, 8f)
        lineTo(8f, 8f)
        moveTo(12f, 7f)
        lineTo(12f, 12f)
        lineTo(16f, 14f)
    }
}.build()

/** `lucide-keyboard`, shown after placing the caret before the IME opens. */
private val KeyboardIcon: ImageVector = ImageVector.Builder(
    name = "keyboard",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(
        fill = null,
        stroke = SolidColor(Color.Black),
        strokeLineWidth = 1.75f,
        strokeLineCap = StrokeCap.Round,
    ) {
        moveTo(5f, 6f)
        lineTo(19f, 6f)
        quadTo(21f, 6f, 21f, 8f)
        lineTo(21f, 16f)
        quadTo(21f, 18f, 19f, 18f)
        lineTo(5f, 18f)
        quadTo(3f, 18f, 3f, 16f)
        lineTo(3f, 8f)
        quadTo(3f, 6f, 5f, 6f)
        close()
        moveTo(6f, 9f)
        lineTo(6.1f, 9f)
        moveTo(10f, 9f)
        lineTo(10.1f, 9f)
        moveTo(14f, 9f)
        lineTo(14.1f, 9f)
        moveTo(18f, 9f)
        lineTo(18.1f, 9f)
        moveTo(8f, 12f)
        lineTo(8.1f, 12f)
        moveTo(12f, 12f)
        lineTo(12.1f, 12f)
        moveTo(16f, 12f)
        lineTo(16.1f, 12f)
        moveTo(7f, 15f)
        lineTo(17f, 15f)
    }
}.build()

/** Obsidian sidebar-toggle: outer rect (1,2,22,20,r4) + inner rect (4,5,2,14,r2). */
private val SidebarToggleIcon: ImageVector = ImageVector.Builder(
    name = "sidebar-toggle",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(
        fill = null,
        stroke = SolidColor(Color.Black),
        strokeLineWidth = 1.75f,
    ) {
        moveTo(5f, 2f)
        lineTo(19f, 2f)
        quadTo(23f, 2f, 23f, 6f)
        lineTo(23f, 18f)
        quadTo(23f, 22f, 19f, 22f)
        lineTo(5f, 22f)
        quadTo(1f, 22f, 1f, 18f)
        lineTo(1f, 6f)
        quadTo(1f, 2f, 5f, 2f)
        close()
    }
    path(fill = SolidColor(Color.Black)) {
        moveTo(5f, 5f)
        curveTo(5.55f, 5f, 6f, 5.45f, 6f, 6f)
        lineTo(6f, 18f)
        curveTo(6f, 18.55f, 5.55f, 19f, 5f, 19f)
        curveTo(4.45f, 19f, 4f, 18.55f, 4f, 18f)
        lineTo(4f, 6f)
        curveTo(4f, 5.45f, 4.45f, 5f, 5f, 5f)
        close()
    }
}.build()

/** `lucide-book-open`, used for the reading-view action. */
private val BookOpenIcon: ImageVector = ImageVector.Builder(
    name = "book-open",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(12f, 7f)
        lineTo(12f, 21f)
    }
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(3f, 18f)
        curveTo(2.45f, 18f, 2f, 17.55f, 2f, 17f)
        lineTo(2f, 4f)
        curveTo(2f, 3.45f, 2.45f, 3f, 3f, 3f)
        lineTo(8f, 3f)
        curveTo(10.21f, 3f, 12f, 4.79f, 12f, 7f)
        curveTo(12f, 4.79f, 13.79f, 3f, 16f, 3f)
        lineTo(21f, 3f)
        curveTo(21.55f, 3f, 22f, 3.45f, 22f, 4f)
        lineTo(22f, 17f)
        curveTo(22f, 17.55f, 21.55f, 18f, 21f, 18f)
        lineTo(15f, 18f)
        curveTo(13.34f, 18f, 12f, 19.34f, 12f, 21f)
        curveTo(12f, 19.34f, 10.66f, 18f, 9f, 18f)
        close()
    }
}.build()

/** `lucide-edit-3`, used for the edit-view action. */
private val EditThreeIcon: ImageVector = ImageVector.Builder(
    name = "edit-3",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(13f, 21f)
        lineTo(21f, 21f)
    }
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(16.5f, 3.5f)
        curveTo(17.33f, 2.67f, 18.67f, 2.67f, 19.5f, 3.5f)
        curveTo(20.33f, 4.33f, 20.33f, 5.67f, 19.5f, 6.5f)
        lineTo(7f, 19f)
        lineTo(3f, 20f)
        lineTo(4f, 16f)
        close()
    }
}.build()

private val EditorFontSize = 15.sp
private val EditorLineHeight = 22.sp

@Composable
fun VibecodiumApp(controller: VibecodiumController) {
    val state = controller.state
    val density = LocalDensity.current
    VibecodiumTheme {
        CompositionLocalProvider(
            LocalDensity provides Density(
                density.density,
                density.fontScale * state.settings.textScale,
            ),
        ) {
            BackHandler(
                enabled = state.vaultChooserOpen || state.settingsOpen || state.drawer != null ||
                    state.tabSwitcher || state.viewerPath != null || state.historyIndex > 0,
            ) {
                controller.back()
            }
            Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
                when {
                    state.booting -> BootScreen()
                    state.vaultName == null -> StartScreen(state, controller)
                    else -> PhoneShell(state, controller)
                }
                Box(
                    Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .windowInsetsBottomHeight(WindowInsets.navigationBars)
                        .background(MaterialTheme.colorScheme.background),
                )
                state.message?.let { message ->
                    MessageBar(message) { controller.dismissMessage() }
                }
                state.deletePath?.let { path ->
                    DeleteDialog(path, controller)
                }
                state.renamePath?.let {
                    RenameDialog(state, controller)
                }
                if (state.newFolderOpen) {
                    NewFolderDialog(state, controller)
                }
                if (state.vaultChooserOpen) {
                    VaultChooserDialog(state, controller)
                }
            }
        }
    }
}

@Composable
private fun BootScreen() {
    Box(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(34.dp),
            strokeWidth = 3.dp,
        )
    }
}

@Composable
private fun StartScreen(state: UiState, controller: VibecodiumController) {
    val folderPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) controller.openSafVault(uri)
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .systemBarsPadding()
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("vibecodium", fontSize = 30.sp, fontWeight = FontWeight.SemiBold)
        Text(
            "Rust core · Compose shell",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = 13.sp,
        )
        Spacer(Modifier.height(28.dp))
        if (state.knownVaults.isNotEmpty()) {
            OutlinedButton(
                onClick = { controller.openVaultChooser() },
                enabled = !state.loading,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Choose existing vault")
            }
            Spacer(Modifier.height(8.dp))
        }
        Button(
            onClick = { folderPicker.launch(null) },
            enabled = !state.loading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Open folder")
        }
        Spacer(Modifier.height(8.dp))
        OutlinedButton(
            onClick = { controller.createDemoVault() },
            enabled = !state.loading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Create demo vault")
        }
        if (state.loading) {
            Spacer(Modifier.height(16.dp))
            LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        Spacer(Modifier.height(16.dp))
        Text(
            "Folder access uses Android's Storage Access Framework. The read/write " +
                "permission is persisted, and the app edits the original files.",
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * The part of [UiState] the editor actually reads. Bundling it lets Compose
 * skip the editor when only an overlay (drawer, tabs, menu) changes.
 */
@Immutable
private data class EditorSlice(
    val tabs: List<Tab>,
    val activeTab: Int,
    val editorMode: EditorMode,
    val lines: List<MarkdownLine>,
    val markdownText: String,
    val bookmarks: Set<String>,
    val stats: DocStats,
    val settingsOpen: Boolean = false,
    val overlayOpen: Boolean = false,
    val focusRequestId: Long = 0L,
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PhoneShell(state: UiState, controller: VibecodiumController) {
    val editor = remember(controller) {
        derivedStateOf {
            val current = controller.state
            EditorSlice(
                tabs = current.tabs,
                activeTab = current.activeTab,
                editorMode = current.editorMode,
                lines = current.lines,
                markdownText = current.markdownText,
                bookmarks = current.bookmarks,
                stats = current.stats,
                settingsOpen = current.settingsOpen,
                overlayOpen = current.settingsOpen || current.renamePath != null ||
                    current.deletePath != null || current.newFolderOpen || current.vaultChooserOpen ||
                    current.tabSwitcher,
                focusRequestId = current.focusRequestId,
            )
        }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .systemBarsPadding(),
    ) {
        val drawerWidth = phoneDrawerWidth(maxWidth)
        val navbarEnabled = state.settings.navbarEnabled
        val imeVisible = WindowInsets.isImeVisible
        val focusManager = LocalFocusManager.current
        var chromeHidden by remember { mutableStateOf(false) }
        val lastAnchor = remember { floatArrayOf(0f) }
        val lastScrollVal = remember { floatArrayOf(0f) }
        val chromeProgress = animateFloatAsState(
            targetValue = if (chromeHidden) 0f else 1f,
            animationSpec = tween(160),
            label = "chrome",
        )
        val navProgress = animateFloatAsState(
            targetValue = if (navbarEnabled && !imeVisible && !chromeHidden) 1f else 0f,
            animationSpec = tween(160),
            label = "navbar",
        )
        LaunchedEffect(state.drawer) {
            if (state.drawer != null) focusManager.clearFocus()
        }
        LaunchedEffect(
            state.drawer,
            state.tabSwitcher,
            state.viewerPath,
            state.settingsOpen,
            editor.value.activeTab,
            editor.value.editorMode,
        ) {
            chromeHidden = false
            lastAnchor[0] = 0f
            lastScrollVal[0] = 0f
        }
        LaunchedEffect(state.settings.autoHideChrome) {
            if (!state.settings.autoHideChrome) chromeHidden = false
        }
        val onEditorScroll: (Float, Float) -> Unit = remember {
            { scroll, maxY ->
                if (!controller.state.settings.autoHideChrome) {
                    if (chromeHidden) chromeHidden = false
                    lastAnchor[0] = 0f
                    lastScrollVal[0] = scroll
                } else {
                    val anchor = lastAnchor[0]
                    val prev = lastScrollVal[0]
                    lastScrollVal[0] = scroll
                    if (maxY <= 0f || scroll <= 2f) {
                        if (chromeHidden) chromeHidden = false
                        lastAnchor[0] = 0f
                    } else {
                        if (scroll > prev && prev < anchor) {
                            lastAnchor[0] = prev
                        } else if (scroll < prev && prev > anchor) {
                            lastAnchor[0] = prev
                        }
                        val currentAnchor = lastAnchor[0]
                        val delta = scroll - currentAnchor
                        if (delta > 24f) {
                            if (!chromeHidden) chromeHidden = true
                            lastAnchor[0] = scroll
                        } else if (delta < -24f) {
                            if (chromeHidden) chromeHidden = false
                            lastAnchor[0] = scroll
                        }
                    }
                }
            }
        }
        Box(Modifier.fillMaxSize()) {
            EditorArea(
                editor.value,
                controller,
                imeVisible,
                navbarEnabled,
                chromeProgress,
                state.drawer != null,
                onEditorScroll,
            )

            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .graphicsLayer {
                        alpha = chromeProgress.value
                        translationY = -(56.dp + ViewHeaderTopPadding).toPx() * (1f - chromeProgress.value)
                    }
                    .semantics {
                        if (chromeProgress.value < 0.05f) invisibleToUser()
                    },
            ) {
                FloatingIconButton(
                    icon = SidebarToggleIcon,
                    description = "Open explorer",
                    tint = if (state.drawer == Drawer.Left) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
                    modifier = Modifier
                        .padding(start = 12.dp, top = ViewHeaderTopPadding)
                        .size(44.dp),
                ) { controller.toggleDrawer(Drawer.Left) }
            }

            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .graphicsLayer {
                        alpha = navProgress.value
                        translationY =
                            (NavbarHeight.toPx() + 24.dp.toPx()) * (1f - navProgress.value)
                    }
                    .semantics {
                        if (!navbarEnabled || imeVisible || navProgress.value < 0.05f) {
                            invisibleToUser()
                        }
                    },
            ) {
                MobileNavbar(
                    state,
                    controller,
                    Modifier
                        .padding(horizontal = 16.dp, vertical = 8.dp)
                        .fillMaxWidth()
                        .widthIn(max = 316.dp),
                )
            }

            if (state.drawer == Drawer.Left) {
                DrawerBackdrop {
                    focusManager.clearFocus()
                    controller.closeOverlays()
                }
            }
            // The drawer stays composed (just off-screen) so its native tree
            // SurfaceView already has a rendered surface when it opens. Adding
            // it on demand exposed the editor for one frame.
            LeftDrawer(
                state,
                controller,
                Modifier
                    .align(Alignment.CenterStart)
                    .width(drawerWidth)
                    .fillMaxHeight()
                    .offset(x = if (state.drawer == Drawer.Left) 0.dp else -drawerWidth)
                    .then(
                        if (state.drawer == Drawer.Left) {
                            Modifier
                        } else {
                            Modifier.semantics { invisibleToUser() }
                        },
                    ),
            )

            if (state.drawer == Drawer.Right) {
                DrawerBackdrop {
                    focusManager.clearFocus()
                    controller.closeOverlays()
                }
                RightDrawer(
                    state,
                    controller,
                    Modifier
                        .align(Alignment.CenterEnd)
                        .width(drawerWidth)
                        .fillMaxHeight(),
                )
            }

            if (state.tabSwitcher) {
                TabSwitcher(state, controller, Modifier.fillMaxSize())
            }

            if (state.viewerPath != null) {
                AttachmentViewer(state, controller, Modifier.fillMaxSize())
            }

            if (state.settingsOpen) {
                SettingsScreen(state, controller, Modifier.fillMaxSize())
            }
        }
    }
}

/**
 * Keeps IME inset reads local so only the editor subtree recomposes while the
 * keyboard animates; the drawers and navbar stay stable.
 */
@Composable
private fun EditorArea(
    editor: EditorSlice,
    controller: VibecodiumController,
    imeVisible: Boolean,
    navbarEnabled: Boolean,
    chromeProgress: State<Float>,
    drawerOpen: Boolean,
    onEditorScroll: (Float, Float) -> Unit,
) {
    val density = LocalDensity.current
    val imeBottomPx = WindowInsets.ime.getBottom(density)
    val idleBottomInsetPx = with(density) {
        if (navbarEnabled) (NavbarHeight + 12.dp).toPx() else 64.dp.toPx()
    }
    val bottomInsetPx = if (imeBottomPx > 0) imeBottomPx.toFloat() else idleBottomInsetPx
    val tab = editor.tabs.getOrNull(editor.activeTab)
        ?: editor.tabs.firstOrNull()?.also {
            LaunchedEffect(Unit) {
                controller.selectTab(0)
            }
        }
    if (tab == null) {
        EmptyEditor(controller)
    } else {
        EditorScreen(
            editor,
            tab,
            controller,
            imeVisible,
            navbarEnabled,
            bottomInsetPx,
            chromeProgress,
            drawerOpen,
            onEditorScroll,
        )
    }
}

/** `--mobile-sidebar-width: 84vw`, clamped to 300–450 px in the phone sheet. */
private fun phoneDrawerWidth(maxWidth: Dp): Dp {
    if (maxWidth <= 300.dp) return maxWidth
    return (maxWidth * 0.84f).coerceIn(300.dp, minOf(420.dp, maxWidth))
}

@Composable
private fun EditorScreen(
    state: EditorSlice,
    tab: Tab,
    controller: VibecodiumController,
    imeVisible: Boolean,
    navbarEnabled: Boolean,
    bottomInsetPx: Float,
    chromeProgress: State<Float>,
    drawerOpen: Boolean,
    onEditorScroll: (Float, Float) -> Unit,
) {
    val bottomInset = if (imeVisible || !navbarEnabled) 0.dp else NavbarHeight + 12.dp
    val preview = state.editorMode == EditorMode.Preview
    val topInsetPx = with(LocalDensity.current) { (ViewHeaderTopPadding + ViewHeaderHeight + 10.dp).toPx() }
    var editorView by remember { mutableStateOf<NativeEditorView?>(null) }
    var keyboardPrompt by remember { mutableStateOf(!preview) }
    val onViewReady: (NativeEditorView) -> Unit = remember { { editorView = it } }
    val onCaretPlaced: () -> Unit = remember { { keyboardPrompt = true } }
    LaunchedEffect(editorView, tab.path, state.activeTab, preview, state.focusRequestId) {
        val view = editorView ?: return@LaunchedEffect
        if (preview) {
            keyboardPrompt = false
            view.clearFocus()
            view.hideKeyboard()
        } else {
            withFrameMillis { }
            if (state.focusRequestId > 0L) {
                keyboardPrompt = false
                view.showKeyboard()
                controller.consumeFocusRequest()
            } else {
                keyboardPrompt = true
                view.focusEditor()
            }
        }
    }
    LaunchedEffect(drawerOpen) {
        if (drawerOpen) {
            editorView?.hideKeyboard()
        } else if (!preview) {
            editorView?.focusEditor()
        }
    }
    var prevImeVisible by remember { mutableStateOf(false) }
    LaunchedEffect(imeVisible) {
        if (prevImeVisible && !imeVisible && !preview) {
            keyboardPrompt = true
            editorView?.onKeyboardDismissed()
        }
        prevImeVisible = imeVisible
    }
    var prevOverlayOpen by remember { mutableStateOf(false) }
    LaunchedEffect(state.overlayOpen) {
        if (prevOverlayOpen && !state.overlayOpen) {
            editorView?.onOverlayDismissed()
        }
        prevOverlayOpen = state.overlayOpen
    }
    val lifecycleOwner = androidx.lifecycle.compose.LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == androidx.lifecycle.Lifecycle.Event.ON_RESUME) {
                if (!preview) {
                    keyboardPrompt = true
                    editorView?.hideKeyboard()
                }
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
    }
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        // Keep the native editor alive while preview is open so switching
        // back does not recreate its SurfaceView or reload the note.
        SourceEditor(
            state,
            controller,
            tab,
            topInsetPx,
            bottomInsetPx,
            preview,
            onEditorScroll,
            onViewReady,
            onCaretPlaced,
        )
        Box(
            Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .padding(top = ViewHeaderTopPadding)
                .graphicsLayer {
                    alpha = chromeProgress.value
                    translationY = -(ViewHeaderHeight + ViewHeaderTopPadding).toPx() * (1f - chromeProgress.value)
                }
                .semantics {
                    if (chromeProgress.value < 0.05f) invisibleToUser()
                },
        ) {
            ViewHeader(state, tab, controller)
        }
        if (!preview && keyboardPrompt && !imeVisible) {
            val bottomPadding = if (navbarEnabled) NavbarHeight + 16.dp else 16.dp
            Surface(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = 16.dp, bottom = bottomPadding)
                    .size(48.dp),
                shape = CircleShape,
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 6.dp,
            ) {
                IconButton(
                    onClick = {
                        editorView?.showKeyboard()
                    },
                    modifier = Modifier.fillMaxSize(),
                ) {
                    Icon(
                        KeyboardIcon,
                        contentDescription = "Show keyboard",
                        modifier = Modifier.size(24.dp),
                        tint = Color.White,
                    )
                }
            }
        }
    }
}

@Composable
private fun ViewHeader(state: EditorSlice, tab: Tab, controller: VibecodiumController) {
    val bookmarked = tab.path in state.bookmarks
    val preview = state.editorMode == EditorMode.Preview
    var menuOpen by remember { mutableStateOf(false) }
    var tabsMenuOpen by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .height(ViewHeaderHeight)
            .padding(start = 60.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The floating explorer button is rendered by PhoneShell at the
        // same top-left position for both empty and open states.
        BoxWithConstraints(
            Modifier
                .weight(1f)
                .fillMaxHeight(),
        ) {
            val titleWidth = maxWidth
            Surface(
                onClick = { tabsMenuOpen = true },
                modifier = Modifier.fillMaxSize(),
                shape = RoundedCornerShape(percent = 50),
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 4.dp,
            ) {
                Row(
                    Modifier
                        .fillMaxSize()
                        .padding(start = 16.dp, end = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = tab.title + if (tab.dirty) " •" else "",
                        modifier = Modifier.weight(1f),
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Icon(
                        Icons.Default.KeyboardArrowDown,
                        contentDescription = "Open tabs",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            // The menu follows the title button: same width, same corner
            // radius, and a small gap underneath it.
            DropdownMenu(
                expanded = tabsMenuOpen,
                onDismissRequest = { tabsMenuOpen = false },
                modifier = Modifier.width(titleWidth),
                offset = DpOffset(0.dp, 4.dp),
                shape = RoundedCornerShape(22.dp),
                containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 4.dp,
            ) {
                state.tabs.forEachIndexed { index, item ->
                    val active = index == state.activeTab
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .height(40.dp)
                            .clickable {
                                tabsMenuOpen = false
                                controller.selectTab(index)
                            }
                            .padding(start = 8.dp, end = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            Icons.Default.Check,
                            contentDescription = null,
                            modifier = Modifier.size(15.dp),
                            tint = if (active) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                Color.Transparent
                            },
                        )
                        Spacer(Modifier.width(10.dp))
                        Text(
                            item.title + if (item.dirty) " •" else "",
                            modifier = Modifier.weight(1f),
                            fontSize = 13.sp,
                            fontWeight = if (active) {
                                FontWeight.SemiBold
                            } else {
                                FontWeight.Normal
                            },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            color = if (active) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.onSurface
                            },
                        )
                        IconButton(
                            onClick = {
                                controller.closeTab(index)
                                if (state.tabs.size <= 1) tabsMenuOpen = false
                            },
                            modifier = Modifier.size(30.dp),
                        ) {
                            Icon(
                                Icons.Default.Close,
                                contentDescription = "Close tab",
                                modifier = Modifier.size(15.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
        Spacer(Modifier.width(8.dp))
        Surface(
            shape = RoundedCornerShape(percent = 50),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 3.dp,
            shadowElevation = 4.dp,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                HeaderActionButton(
                    icon = if (preview) EditThreeIcon else BookOpenIcon,
                    description = if (preview) "Edit view" else "Reading view",
                ) {
                    controller.setEditorMode(if (preview) EditorMode.Source else EditorMode.Preview)
                }
                Box {
                    HeaderActionButton(
                        icon = Icons.Default.MoreVert,
                        description = "More options",
                    ) { menuOpen = true }
                    NoteActionsMenu(
                        expanded = menuOpen,
                        onDismiss = { menuOpen = false },
                        tab = tab,
                        editorMode = state.editorMode,
                        bookmarked = bookmarked,
                        activeTab = state.activeTab,
                        controller = controller,
                    )
                }
            }
        }
    }
}

@Composable
private fun FloatingIconButton(
    icon: ImageVector,
    description: String,
    modifier: Modifier = Modifier
        .size(44.dp),
    tint: Color = MaterialTheme.colorScheme.onSurfaceVariant,
    onClick: () -> Unit,
) {
    Surface(
        modifier = modifier,
        shape = CircleShape,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        tonalElevation = 3.dp,
        shadowElevation = 4.dp,
    ) {
        IconButton(onClick = onClick, modifier = Modifier.fillMaxSize()) {
            Icon(
                icon,
                contentDescription = description,
                modifier = Modifier.size(24.dp),
                tint = tint,
            )
        }
    }
}

@Composable
private fun HeaderActionButton(
    icon: ImageVector,
    description: String,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
        Icon(
            icon,
            contentDescription = description,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun NoteActionsMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    tab: Tab,
    editorMode: EditorMode,
    bookmarked: Boolean,
    activeTab: Int,
    controller: VibecodiumController,
) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        DropdownMenuItem(
            text = { Text(if (editorMode == EditorMode.Source) "Reading view" else "Edit view") },
            onClick = {
                onDismiss()
                controller.setEditorMode(
                    if (editorMode == EditorMode.Source) EditorMode.Preview else EditorMode.Source,
                )
            },
        )
        DropdownMenuItem(
            text = { Text("Save note") },
            onClick = { onDismiss(); controller.saveActiveNow() },
        )
        DropdownMenuItem(
            text = { Text("Rename note") },
            onClick = { onDismiss(); controller.beginRename() },
        )
        DropdownMenuItem(
            text = { Text(if (bookmarked) "Remove bookmark" else "Toggle bookmark") },
            onClick = { onDismiss(); controller.toggleBookmarkActive() },
        )
        if (tab.language == null) {
            DropdownMenuItem(
                text = { Text("Outline") },
                onClick = { onDismiss(); controller.toggleDrawer(Drawer.Right) },
            )
        }
        DropdownMenuItem(
            text = { Text("Sync") },
            onClick = { onDismiss(); controller.openSettings("Sync") },
        )
        DropdownMenuItem(
            text = { Text("Close tab") },
            onClick = { onDismiss(); controller.closeTab(activeTab) },
        )
        DropdownMenuItem(
            text = { Text("Delete note") },
            onClick = { onDismiss(); controller.confirmDeleteActive() },
        )
        HorizontalDivider()
        DropdownMenuItem(
            text = { Text("Open another vault") },
            onClick = { onDismiss(); controller.openVaultChooser() },
        )
        DropdownMenuItem(
            text = { Text("About") },
            onClick = {
                onDismiss()
                controller.showMessage("vibecodium · Rust core 0.1.0")
            },
        )
    }
}

// --------------------------------------------------------------------- source

@Composable
private fun SourceEditor(
    state: EditorSlice,
    controller: VibecodiumController,
    tab: Tab,
    topInsetPx: Float,
    bottomInsetPx: Float,
    preview: Boolean,
    onScroll: (Float, Float) -> Unit,
    onViewReady: (NativeEditorView) -> Unit,
    onCaretPlaced: () -> Unit,
) {
    AndroidView(
        factory = { context ->
            NativeEditorView(
                context = context,
                onTextChanged = { text, selection ->
                    controller.updateNativeEditor(text, selection)
                },
                idleBottomInsetPx = bottomInsetPx,
            ).also(onViewReady)
        },
        modifier = Modifier.fillMaxSize(),
        update = { view ->
            view.setTopInset(topInsetPx)
            view.setBottomInset(bottomInsetPx)
            view.setOnScrollChanged(onScroll)
            view.setOnCaretPlaced(onCaretPlaced)
            view.setEditorPreview(preview)
            view.setEditor(tab.text, tab.selection, tab.language, tab.path)
        },
    )
}

// -------------------------------------------------------------------- preview

private sealed interface PreviewBlock {
    data class Line(val line: MarkdownLine) : PreviewBlock
    data class Code(val code: String, val language: String) : PreviewBlock
    data class Table(val rows: List<List<String>>) : PreviewBlock
}

private val TABLE_KINDS = setOf("table", "table_row", "table_separator")

private fun buildPreviewBlocks(lines: List<MarkdownLine>): List<PreviewBlock> {
    val blocks = mutableListOf<PreviewBlock>()
    var index = 0
    while (index < lines.size) {
        val line = lines[index]
        when (line.kind) {
            "code" -> {
                val language = line.language.orEmpty()
                val code = StringBuilder()
                while (index < lines.size && lines[index].kind == "code") {
                    if (code.isNotEmpty()) code.append('\n')
                    code.append(lines[index].content)
                    index += 1
                }
                blocks.add(PreviewBlock.Code(code.toString(), language))
            }

            "table" -> {
                val rows = mutableListOf<List<String>>()
                while (index < lines.size && lines[index].kind in TABLE_KINDS) {
                    if (lines[index].kind != "table_separator") {
                        rows.add(lines[index].cells)
                    }
                    index += 1
                }
                blocks.add(PreviewBlock.Table(rows))
            }

            "code_fence" -> index += 1

            else -> {
                blocks.add(PreviewBlock.Line(line))
                index += 1
            }
        }
    }
    return blocks
}

@Composable
private fun MarkdownPreview(
    state: EditorSlice,
    controller: VibecodiumController,
    tab: Tab,
    topInset: Dp,
    bottomInset: Dp,
    onScroll: (Float, Float) -> Unit,
) {
    val blocks = remember(state.lines) { buildPreviewBlocks(state.lines) }
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val listState = rememberLazyListState()
    LaunchedEffect(listState) {
        snapshotFlow {
            listState.firstVisibleItemIndex * 100_000L + listState.firstVisibleItemScrollOffset
        }.collect { value -> onScroll(value.toFloat(), 1f) }
    }
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(
            start = 16.dp,
            top = 10.dp + topInset,
            end = 16.dp,
            bottom = 10.dp + bottomInset,
        ),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (blocks.isEmpty()) {
            val preparing = state.lines.isEmpty() && tab.text.isNotBlank()
            item {
                Text(
                    if (preparing) "Preparing preview…" else "Nothing to preview yet.",
                    fontSize = 13.sp,
                    color = muted,
                )
            }
        }
        items(blocks) { block ->
            when (block) {
                is PreviewBlock.Code -> CodeBlock(block.code, block.language)
                is PreviewBlock.Table -> MarkdownTable(block.rows)
                is PreviewBlock.Line -> MarkdownLineView(block.line, controller, tab.path)
            }
        }
    }
}

@Composable
private fun MarkdownLineView(
    line: MarkdownLine,
    controller: VibecodiumController,
    notePath: String,
) {
    val accent = MaterialTheme.colorScheme.primary
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val codeColors = LocalCodeColors.current
    val annotated = annotatedFromSpans(line.content, line.spans, accent, codeColors)
    when (line.kind) {
        "heading" -> Text(
            annotated,
            fontSize = when (line.level) {
                1 -> 24.sp
                2 -> 21.sp
                3 -> 19.sp
                4 -> 17.sp
                5 -> 16.sp
                else -> 15.sp
            },
            fontWeight = FontWeight.SemiBold,
        )

        "paragraph" -> Text(annotated, fontSize = 15.sp, lineHeight = 22.sp)

        "quote" -> Row(Modifier.fillMaxWidth()) {
            Box(
                Modifier
                    .width(3.dp)
                    .heightIn(min = 20.dp)
                    .background(accent, RoundedCornerShape(2.dp)),
            )
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                line.callout?.let {
                    Text(it, fontSize = 11.sp, color = accent, fontWeight = FontWeight.SemiBold)
                }
                Text(
                    annotated,
                    fontSize = 14.sp,
                    lineHeight = 21.sp,
                    color = muted,
                )
            }
        }

        "task" -> Row(
            Modifier.fillMaxWidth().padding(start = (line.indent * 8).dp),
            verticalAlignment = Alignment.Top,
        ) {
            Text(if (line.checked == true) "☑" else "☐", color = accent, fontSize = 15.sp)
            Spacer(Modifier.width(6.dp))
            Text(annotated, fontSize = 15.sp, lineHeight = 22.sp)
        }

        "bullet" -> Row(
            Modifier.fillMaxWidth().padding(start = (line.indent * 8).dp),
            verticalAlignment = Alignment.Top,
        ) {
            Text("•", color = accent, fontSize = 15.sp)
            Spacer(Modifier.width(6.dp))
            Text(annotated, fontSize = 15.sp, lineHeight = 22.sp)
        }

        "numbered" -> Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            Text("${line.num.orEmpty()}. ", color = accent, fontSize = 15.sp)
            Text(annotated, fontSize = 15.sp, lineHeight = 22.sp)
        }

        "image" -> MarkdownImage(
            controller = controller,
            target = line.target.orEmpty(),
            alt = line.alt.orEmpty(),
            notePath = notePath,
        )

        "rule" -> HorizontalDivider(color = MaterialTheme.colorScheme.outline)

        "code_fence", "table", "table_row", "table_separator" -> Unit

        else -> Text(line.content, fontSize = 14.sp)
    }
}

private fun annotatedFromSpans(
    content: String,
    spans: List<MarkdownSpan>,
    accent: Color,
    codeColors: CodeColors,
): AnnotatedString {
    if (spans.isEmpty()) return AnnotatedString(content)
    return buildAnnotatedString {
        var offset = 0
        for (span in spans) {
            if (span.kind == "marker") continue
            val start = span.start.coerceIn(0, content.length)
            val end = span.end.coerceIn(start, content.length)
            val text = content.substring(start, end)
            val styleStart = offset
            append(text)
            offset += text.length
            val style = when (span.kind) {
                "bold" -> SpanStyle(fontWeight = FontWeight.Bold)
                "italic" -> SpanStyle(fontStyle = FontStyle.Italic)
                "bold_italic" -> SpanStyle(fontWeight = FontWeight.Bold, fontStyle = FontStyle.Italic)
                "inline_code" -> SpanStyle(
                    fontFamily = FontFamily.Monospace,
                    background = codeColors.background,
                    color = codeColors.normal,
                )
                "strikethrough" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                "link" -> SpanStyle(color = accent, textDecoration = TextDecoration.Underline)
                "underline" -> SpanStyle(textDecoration = TextDecoration.Underline)
                "highlight" -> SpanStyle(background = Color(0x33A882FF))
                "tag" -> SpanStyle(color = accent)
                "grammar" -> SpanStyle(fontStyle = FontStyle.Italic)
                else -> null
            }
            if (style != null) addStyle(style, styleStart, offset)
        }
    }
}

@Composable
private fun MarkdownImage(
    controller: VibecodiumController,
    target: String,
    alt: String,
    notePath: String,
) {
    val bytes by produceState<ByteArray?>(initialValue = null, key1 = target, key2 = notePath) {
        value = controller.loadAttachment(target, notePath)
    }
    when {
        target.isBlank() ->
            Text("Image target is empty", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)

        target.startsWith("http://") || target.startsWith("https://") ->
            Text("External image: $target", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)

        bytes == null ->
            Text("Image not found: $target", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)

        else -> {
            val data = bytes
            val bitmap = remember(data) {
                if (data == null) null else BitmapFactory.decodeByteArray(data, 0, data.size)
            }
            if (bitmap == null) {
                Text("Unsupported image: $target", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Image(
                    bitmap = bitmap.asImageBitmap(),
                    contentDescription = alt.ifBlank { target },
                    modifier = Modifier.fillMaxWidth().heightIn(max = 320.dp),
                    contentScale = ContentScale.FillWidth,
                )
            }
        }
    }
}

@Composable
private fun CodeBlock(code: String, language: String) {
    val colors = LocalCodeColors.current
    val clipboard = LocalClipboardManager.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.background)
            .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(8.dp)),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                language.ifBlank { "plain text" },
                modifier = Modifier.weight(1f),
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = { clipboard.setText(AnnotatedString(code)) }) {
                Text("Copy", fontSize = 11.sp)
            }
        }
        SelectionContainer {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(start = 12.dp, end = 12.dp, bottom = 12.dp),
            ) {
                Text(
                    text = annotateCode(code, language, colors),
                    fontFamily = FontFamily.Monospace,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                    softWrap = false,
                )
            }
        }
    }
}

@Composable
private fun MarkdownTable(rows: List<List<String>>) {
    if (rows.isEmpty()) return
    Column(
        Modifier
            .fillMaxWidth()
            .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(6.dp)),
    ) {
        rows.forEachIndexed { rowIndex, row ->
            Row(Modifier.fillMaxWidth()) {
                row.forEach { cell ->
                    Text(
                        cell,
                        modifier = Modifier.weight(1f).padding(6.dp),
                        fontSize = 13.sp,
                        fontWeight = if (rowIndex == 0) FontWeight.SemiBold else FontWeight.Normal,
                    )
                }
            }
            if (rowIndex != rows.lastIndex) {
                HorizontalDivider(color = MaterialTheme.colorScheme.outline, thickness = 0.5.dp)
            }
        }
    }
}

@Composable
private fun CodeViewer(
    tab: Tab,
    modifier: Modifier,
    topInset: Dp,
    bottomInset: Dp,
    onScroll: (Float, Float) -> Unit,
) {
    val language = tab.language ?: return
    val colors = LocalCodeColors.current
    val clipboard = LocalClipboardManager.current
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()
    val lineCount = tab.text.count { it == '\n' } + 1
    LaunchedEffect(vertical) {
        snapshotFlow { vertical.value to vertical.maxValue }
            .collect { (value, max) -> onScroll(value.toFloat(), max.toFloat()) }
    }
    Column(
        modifier
            .background(colors.background)
            .verticalScroll(vertical),
    ) {
        Spacer(Modifier.height(topInset))
        Row(
            Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                language,
                modifier = Modifier.weight(1f),
                fontSize = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = { clipboard.setText(AnnotatedString(tab.text)) }) {
                Text("Copy", fontSize = 11.sp)
            }
        }
        Row(Modifier.fillMaxWidth()) {
            Column(
                Modifier.padding(start = 8.dp, end = 10.dp, bottom = 12.dp + bottomInset),
                horizontalAlignment = Alignment.End,
            ) {
                for (line in 1..lineCount) {
                    Text(
                        line.toString(),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        lineHeight = 22.sp,
                        color = colors.comment,
                        textAlign = TextAlign.End,
                    )
                }
            }
            Box(
                Modifier
                    .weight(1f)
                    .horizontalScroll(horizontal)
                    .padding(end = 12.dp, bottom = 12.dp + bottomInset),
            ) {
                SelectionContainer {
                    Text(
                        annotateCode(tab.text, language, colors),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 14.sp,
                        lineHeight = 22.sp,
                        softWrap = false,
                    )
                }
            }
        }
    }
}

// --------------------------------------------------------------------- navbar

@Composable
private fun MobileNavbar(
    state: UiState,
    controller: VibecodiumController,
    modifier: Modifier = Modifier,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val tab = state.tabs.getOrNull(state.activeTab) ?: Tab(path = "", title = "", text = "")
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(percent = 50),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 6.dp,
        shadowElevation = 10.dp,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(NavbarHeight)
                .padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NavButton(
                icon = Icons.Default.ArrowBack,
                label = "Back",
                enabled = state.historyIndex > 0,
            ) { controller.back() }
            NavButton(
                icon = Icons.Default.ArrowForward,
                label = "Forward",
                enabled = state.historyIndex in 0 until state.history.lastIndex,
            ) { controller.forward() }
            NavButton(
                icon = Icons.Default.Search,
                label = "Search",
            ) { controller.setDrawerMode(DrawerMode.Search) }
            NavButton(
                icon = Icons.Default.Add,
                label = "New note",
            ) { controller.newNote() }
            NavButton(
                icon = Icons.Default.List,
                label = "Tabs",
                active = state.tabSwitcher,
                badge = state.tabs.size,
            ) { controller.toggleTabSwitcher() }

            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .clickable { menuOpen = true },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Default.Menu,
                    contentDescription = "Menu",
                    modifier = Modifier.size(22.dp),
                    tint = MaterialTheme.colorScheme.onBackground,
                )
                NoteActionsMenu(
                    expanded = menuOpen,
                    onDismiss = { menuOpen = false },
                    tab = tab,
                    editorMode = state.editorMode,
                    bookmarked = tab.path.isNotEmpty() && tab.path in state.bookmarks,
                    activeTab = state.activeTab,
                    controller = controller,
                )
            }
        }
    }
}

@Composable
private fun RowScope.NavButton(
    icon: ImageVector,
    label: String,
    enabled: Boolean = true,
    active: Boolean = false,
    badge: Int? = null,
    onClick: () -> Unit,
) {
    val tint = when {
        !enabled -> MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.35f)
        active -> MaterialTheme.colorScheme.primary
        else -> MaterialTheme.colorScheme.onBackground
    }
    Box(
        Modifier
            .weight(1f)
            .fillMaxHeight()
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, contentDescription = label, tint = tint, modifier = Modifier.size(22.dp))
            if (badge != null && badge > 0) {
                Spacer(Modifier.width(2.dp))
                Text(
                    badge.toString(),
                    fontSize = 10.sp,
                    color = tint,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        }
    }
}

@Composable
private fun RowScope.ModeTab(
    label: String,
    icon: ImageVector,
    active: Boolean,
    onClick: () -> Unit,
) {
    val color = if (active) {
        MaterialTheme.colorScheme.primary
    } else {
        MaterialTheme.colorScheme.onSurfaceVariant
    }
    Column(
        Modifier.weight(1f).fillMaxHeight().clickable(onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = label, tint = color, modifier = Modifier.size(18.dp))
        Text(label, fontSize = 10.sp, color = color)
    }
}

@Composable
private fun BoxScope.DrawerBackdrop(onDismiss: () -> Unit) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.5f))
            .clickable(onClick = onDismiss),
    )
}

@Composable
private fun VaultChooserDialog(state: UiState, controller: VibecodiumController) {
    val picker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            controller.openSafVault(uri)
            controller.closeVaultChooser()
        }
    }
    Dialog(
        onDismissRequest = { controller.closeVaultChooser() },
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(
            modifier = Modifier
                .padding(24.dp)
                .widthIn(max = 440.dp)
                .fillMaxWidth(),
            shape = RoundedCornerShape(20.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 6.dp,
            shadowElevation = 8.dp,
        ) {
            Column(Modifier.padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "Vaults",
                        modifier = Modifier.weight(1f),
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    IconButton(onClick = { controller.closeVaultChooser() }) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = "Close",
                            modifier = Modifier.size(20.dp),
                        )
                    }
                }
                if (state.knownVaults.isEmpty()) {
                    Text(
                        "No saved vaults yet. Open a folder to add one.",
                        modifier = Modifier.padding(vertical = 16.dp),
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = 320.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        items(state.knownVaults, key = { it.locator.value }) { vault ->
                            VaultChooserRow(
                                vault = vault,
                                current = vault.locator.value == state.vaultRoot,
                                onOpen = { controller.openKnownVault(vault) },
                                onForget = { controller.forgetKnownVault(vault) },
                            )
                        }
                    }
                }
                Spacer(Modifier.height(16.dp))
                Button(
                    onClick = { picker.launch(null) },
                    enabled = !state.loading,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(Icons.Default.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Open folder")
                }
                Spacer(Modifier.height(8.dp))
                OutlinedButton(
                    onClick = {
                        controller.createDemoVault()
                        controller.closeVaultChooser()
                    },
                    enabled = !state.loading,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Create demo vault")
                }
            }
        }
    }
}

@Composable
private fun VaultChooserRow(
    vault: KnownVault,
    current: Boolean,
    onOpen: () -> Unit,
    onForget: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(
                if (current) {
                    MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)
                } else {
                    MaterialTheme.colorScheme.surfaceContainerLowest
                },
            )
            .clickable(onClick = onOpen)
            .padding(start = 12.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (vault.locator is VaultLocator.Scoped) Icons.Default.List else Icons.Default.Home,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (current) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(
                vault.name,
                fontSize = 14.sp,
                fontWeight = if (current) FontWeight.SemiBold else FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                if (vault.locator is VaultLocator.Scoped) "SAF folder" else vault.locator.value,
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (current) {
            Icon(
                Icons.Default.Check,
                contentDescription = "Current vault",
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        } else {
            IconButton(
                onClick = onForget,
                modifier = Modifier.size(36.dp),
            ) {
                Icon(
                    Icons.Default.Close,
                    contentDescription = "Remove from list",
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun LeftDrawer(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    val focusManager = LocalFocusManager.current
    Surface(modifier = modifier, color = Color(0xFF282828), tonalElevation = 0.dp) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 58.dp)
                    .padding(start = 16.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.weight(1f)) {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clickable { controller.openVaultChooser() }
                            .padding(vertical = 6.dp),
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                state.vaultName ?: "Vault",
                                fontSize = 15.sp,
                                lineHeight = 20.sp,
                                fontWeight = FontWeight.SemiBold,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Icon(
                                Icons.Default.KeyboardArrowDown,
                                contentDescription = "Choose vault",
                                modifier = Modifier.size(20.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Text(
                            syncSubtitle(state),
                            fontSize = 11.sp,
                            lineHeight = 14.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                IconButton(
                    onClick = {
                        focusManager.clearFocus()
                        controller.openSettings()
                    },
                    modifier = Modifier.size(44.dp),
                ) {
                    Icon(
                        Icons.Default.Settings,
                        contentDescription = "Settings",
                        modifier = Modifier.size(24.dp),
                    )
                }
                IconButton(
                    onClick = {
                        focusManager.clearFocus()
                        controller.closeOverlays()
                    },
                    modifier = Modifier.size(44.dp),
                ) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = "Close",
                        modifier = Modifier.size(24.dp),
                    )
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Box(Modifier.weight(1f).fillMaxWidth()) {
                // Keep the native tree composed while another section is open.
                // Recreating its SurfaceView on every tab switch tore the
                // surface down and flashed the editor through the drawer.
                val overlayMode = state.drawerMode != DrawerMode.Files
                Box(
                    Modifier
                        .fillMaxSize()
                        .then(
                            if (overlayMode) {
                                Modifier.semantics { invisibleToUser() }
                            } else {
                                Modifier
                            },
                        ),
                ) {
                    TreeList(state, controller)
                }
                if (overlayMode) {
                    Surface(
                        modifier = Modifier
                            .fillMaxSize()
                            .pointerInput(Unit) {
                                // Swallow touches so they cannot reach the
                                // still-attached native tree underneath.
                                awaitPointerEventScope {
                                    while (true) {
                                        awaitPointerEvent().changes.forEach { it.consume() }
                                    }
                                }
                            },
                        color = Color(0xFF282828),
                        tonalElevation = 0.dp,
                    ) {
                        when (state.drawerMode) {
                            DrawerMode.Search -> SearchPanel(state, controller)
                            DrawerMode.Bookmarks -> key(state.bookmarks, state.notes) {
                                BookmarkedList(state, controller)
                            }
                            DrawerMode.Recent -> key(state.recentFiles, state.notes) {
                                RecentList(state, controller)
                            }
                            DrawerMode.Files -> Unit
                        }
                    }
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(48.dp)
                    .padding(horizontal = 8.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                for (mode in DrawerMode.entries) {
                    val active = state.drawerMode == mode
                    Box(
                        Modifier
                            .weight(1f)
                            .fillMaxHeight()
                            .clip(RoundedCornerShape(10.dp))
                            .background(
                                if (active) {
                                    MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)
                                } else {
                                    Color.Transparent
                                },
                            )
                            .clickable {
                                focusManager.clearFocus()
                                controller.setDrawerMode(mode)
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            drawerModeIcon(mode),
                            contentDescription = drawerModeLabel(mode),
                            modifier = Modifier.size(26.dp),
                            tint = if (active) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.onSurfaceVariant
                            },
                        )
                    }
                }
            }
        }
    }
}

private fun drawerModeLabel(mode: DrawerMode): String = when (mode) {
    DrawerMode.Files -> "Files"
    DrawerMode.Search -> "Search"
    DrawerMode.Bookmarks -> "Bookmarks"
    DrawerMode.Recent -> "Recent"
}

private fun drawerModeIcon(mode: DrawerMode): ImageVector = when (mode) {
    DrawerMode.Files -> Icons.Default.Home
    DrawerMode.Search -> Icons.Default.Search
    DrawerMode.Bookmarks -> Icons.Default.Star
    DrawerMode.Recent -> HistoryIcon
}

@Composable
private fun RightDrawer(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    Surface(modifier = modifier, color = MaterialTheme.colorScheme.surface, tonalElevation = 8.dp) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(52.dp)
                    .padding(start = 14.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Outline",
                    modifier = Modifier.weight(1f),
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                IconButton(onClick = { controller.closeOverlays() }) {
                    Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            if (state.headings.isEmpty()) {
                Placeholder("No headings in the active note.")
            } else {
                LazyColumn(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .padding(vertical = 6.dp),
                ) {
                    items(state.headings) { heading ->
                        Text(
                            heading.text,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(
                                    start = ((heading.level - 1) * 12).dp + 12.dp,
                                    end = 12.dp,
                                    top = 4.dp,
                                    bottom = 4.dp,
                                ),
                            fontSize = if (heading.level <= 2) 14.sp else 13.sp,
                            fontWeight = if (heading.level == 1) FontWeight.SemiBold else FontWeight.Normal,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Text(
                "${state.stats.words} words · ${state.stats.chars} chars · ${state.stats.lines} lines",
                modifier = Modifier.fillMaxWidth().padding(12.dp),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun TabSwitcher(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    Surface(modifier = modifier, color = MaterialTheme.colorScheme.background) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(52.dp)
                    .padding(start = 14.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Tabs (${state.tabs.size})",
                    modifier = Modifier.weight(1f),
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                IconButton(onClick = { controller.newNote() }) {
                    Icon(Icons.Default.Add, contentDescription = "New note", modifier = Modifier.size(20.dp))
                }
                IconButton(onClick = { controller.toggleTabSwitcher() }) {
                    Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            LazyColumn(Modifier.fillMaxSize().padding(8.dp)) {
                itemsIndexed(state.tabs) { index, tab ->
                    val active = index == state.activeTab
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .background(
                                if (active) MaterialTheme.colorScheme.primary.copy(alpha = 0.15f)
                                else Color.Transparent,
                                RoundedCornerShape(8.dp),
                            )
                            .clickable { controller.selectTab(index) }
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                tab.title + if (tab.dirty) " •" else "",
                                fontSize = 14.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                tab.path,
                                fontSize = 11.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (tab.path in state.bookmarks) {
                            Icon(
                                Icons.Default.Star,
                                contentDescription = "Bookmarked",
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        }
                        if (active) {
                            Icon(
                                Icons.Default.Check,
                                contentDescription = null,
                                modifier = Modifier.size(16.dp),
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        }
                        IconButton(onClick = { controller.closeTab(index) }) {
                            Icon(
                                Icons.Default.Close,
                                contentDescription = "Close tab",
                                modifier = Modifier.size(16.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun TreeList(state: UiState, controller: VibecodiumController) {
    var filterFocused by remember { mutableStateOf(false) }
    val focusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    Column(Modifier.fillMaxSize()) {
        Box(
            Modifier.pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        if (event.changes.any { it.changedToDown() }) {
                            focusManager.clearFocus()
                        }
                    }
                }
            },
        ) {
            ExplorerToolbar(state, controller)
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
        Box(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 5.dp),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(36.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(
                        if (filterFocused) Color(0xFF1B1B1B) else Color(0xFF222222),
                    )
                    .border(
                        width = if (filterFocused) 1.5.dp else 1.dp,
                        color = if (filterFocused) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            Color(0xFF383838)
                        },
                        shape = RoundedCornerShape(8.dp),
                    )
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                    ) {
                        focusRequester.requestFocus()
                    }
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Default.Search,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = if (filterFocused) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                )
                BasicTextField(
                    value = state.fileQuery,
                    onValueChange = controller::setFileQuery,
                    modifier = Modifier
                        .weight(1f)
                        .padding(horizontal = 8.dp)
                        .focusRequester(focusRequester)
                        .onFocusChanged { filterFocused = it.isFocused },
                    singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(
                        fontSize = 14.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                    ),
                    cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                    decorationBox = { innerTextField ->
                        Box(Modifier.fillMaxWidth()) {
                            if (!filterFocused && state.fileQuery.isEmpty()) {
                                Text(
                                    "Quick filter...",
                                    fontSize = 14.sp,
                                    color = Color(0xFF8A8A8A),
                                )
                            }
                            innerTextField()
                        }
                    },
                )
                if (state.fileQuery.isNotEmpty()) {
                    IconButton(
                        onClick = { controller.setFileQuery("") },
                        modifier = Modifier.size(28.dp),
                    ) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = "Clear filter",
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
        if (state.entries.isEmpty() && state.folders.isEmpty()) {
            Placeholder("This vault has no files yet.")
            return@Column
        }
        val filtered = remember(state.explorerTree, state.fileQuery) {
            filterExplorerTree(state.explorerTree, state.fileQuery)
        }
        val collapsed = if (state.fileQuery.isBlank()) state.collapsedFolders else emptySet()
        val rows = remember(filtered, collapsed) {
            flattenExplorerTree(filtered, collapsed)
        }
        if (rows.isEmpty()) {
            Placeholder("No matching files.")
            return@Column
        }
        val activePath = state.tabs.getOrNull(state.activeTab)?.path
        val treeJson = remember(rows, activePath, state.collapsedFolders) {
            val array = JSONArray()
            for (row in rows) {
                array.put(
                    JSONObject()
                        .put(
                            "name",
                            if (row.node.isFolder) {
                                row.node.name
                            } else {
                                row.node.name.substringBeforeLast('.', row.node.name)
                            },
                        )
                        .put("path", row.node.path)
                        .put("folder", row.node.isFolder)
                        .put("depth", row.depth)
                        .put("expanded", row.node.path !in state.collapsedFolders)
                        .put("has_children", row.node.children.isNotEmpty())
                        .put("note", row.node.entry?.isNote == true)
                        .put("extension", row.node.entry?.extension.orEmpty()),
                )
            }
            JSONObject()
                .put("rows", array)
                .put("active", activePath ?: JSONObject.NULL)
                .toString()
        }
        AndroidView(
            factory = { context ->
                NativeTreeView(
                    context,
                    onAction = { action -> handleTreeAction(action, controller) },
                    onTouchStart = { focusManager.clearFocus() },
                )
            },
            modifier = Modifier.weight(1f).fillMaxWidth(),
            update = { it.setRows(treeJson); it.setDrawerVisible(state.drawer == Drawer.Left) },
        )
    }
}

private fun handleTreeAction(action: String, controller: VibecodiumController) {
    val value = runCatching { JSONObject(action) }.getOrNull() ?: return
    val path = value.optString("path")
    if (path.isEmpty()) return
    when (value.optString("kind")) {
        "toggle" -> controller.toggleFolder(path)
        "open_note" -> {
            controller.openNote(path)
            controller.closeOverlays()
        }
        "open_file" -> controller.openAttachment(path)
    }
}

/** Explorer nav header, ordered like Obsidian's `nav-buttons-container`. */
@Composable
private fun ExplorerToolbar(state: UiState, controller: VibecodiumController) {
    val allCollapsed = state.folders.isNotEmpty() &&
        state.folders.all { it in state.collapsedFolders }
    val syncLabel = when {
        state.sync.busy -> "Stop sync"
        state.sync.connected && !state.vaultSync.paused -> "Sync"
        else -> "Refresh files"
    }
    val syncAngle = if (state.sync.busy) {
        val transition = rememberInfiniteTransition(label = "sync-rotation")
        val angle by transition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(900, easing = LinearEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "sync-angle",
        )
        angle
    } else {
        0f
    }
    Row(
        Modifier
            .fillMaxWidth()
            .height(48.dp)
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CompactToolbarButton(
            NewNoteIcon,
            "New note",
            modifier = Modifier.weight(1f).fillMaxHeight(),
        ) { controller.newNote() }
        CompactToolbarButton(
            NewFolderIcon,
            "New folder",
            modifier = Modifier.weight(1f).fillMaxHeight(),
        ) { controller.beginNewFolder() }
        CompactToolbarButton(
            CollapseAllIcon,
            if (allCollapsed) "Expand all folders" else "Collapse all folders",
            enabled = state.folders.isNotEmpty(),
            modifier = Modifier.weight(1f).fillMaxHeight(),
        ) { controller.toggleAllFolders() }
        CompactToolbarButton(
            SyncArrowsIcon,
            syncLabel,
            rotation = syncAngle,
            modifier = Modifier.weight(1f).fillMaxHeight(),
        ) {
            controller.refreshOrSync()
        }
    }
}

@Composable
private fun CompactToolbarButton(
    icon: ImageVector,
    description: String,
    enabled: Boolean = true,
    tint: Color = MaterialTheme.colorScheme.onSurfaceVariant,
    rotation: Float = 0f,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, modifier = modifier, enabled = enabled) {
        Icon(
            icon,
            contentDescription = description,
            modifier = Modifier
                .size(26.dp)
                .graphicsLayer { rotationZ = rotation },
            tint = if (enabled) tint else tint.copy(alpha = 0.35f),
        )
    }
}

@Composable
private fun BookmarkedList(state: UiState, controller: VibecodiumController) {
    val notes = state.notes.filter { it.path in state.bookmarks }
    if (notes.isEmpty()) {
        Placeholder("No bookmarked notes yet.\nUse the star in the note header.")
        return
    }
    val listState = rememberLazyListState()
    Box(Modifier.fillMaxSize()) {
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(notes, key = { it.path }) { note ->
                NoteRow(
                    note,
                    active = state.tabs.getOrNull(state.activeTab)?.path == note.path,
                    bookmarked = true,
                    detail = note.path,
                ) {
                    controller.openNote(note.path)
                    controller.closeOverlays()
                }
                HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
            }
        }
        ThinScrollbar(
            listState,
            Modifier
                .align(Alignment.CenterEnd)
                .fillMaxHeight()
                .padding(end = 2.dp)
                .width(3.dp),
        )
    }
}

@Composable
private fun RecentList(state: UiState, controller: VibecodiumController) {
    val byPath = state.notes.associateBy { it.path }
    val recent = state.recentFiles
        .mapNotNull { entry -> byPath[entry.path]?.let { it to entry.openedAt } }
        .take(30)
    if (recent.isEmpty()) {
        Placeholder("Notes you open will appear here.")
        return
    }
    val listState = rememberLazyListState()
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(40.dp)
                .padding(horizontal = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "Recent",
                modifier = Modifier.weight(1f),
                fontSize = 12.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = { controller.clearRecent() }) {
                Text("Clear all", fontSize = 12.sp)
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
                items(recent, key = { it.first.path }) { (note, openedAt) ->
                    NoteRow(
                        note,
                        active = false,
                        bookmarked = note.path in state.bookmarks,
                        detail = if (openedAt > 0) dateTimeLabel(openedAt) else "—",
                    ) {
                        controller.openNote(note.path)
                        controller.closeOverlays()
                    }
                }
            }
            ThinScrollbar(
                listState,
                Modifier
                    .align(Alignment.CenterEnd)
                    .fillMaxHeight()
                    .padding(end = 2.dp)
                    .width(3.dp),
            )
        }
    }
}

@Composable
private fun ThinScrollbar(state: LazyListState, modifier: Modifier = Modifier) {
    val layout = state.layoutInfo
    val total = layout.totalItemsCount
    val visible = layout.visibleItemsInfo
    if (total <= 0 || visible.isEmpty() || total <= visible.size) return
    val first = visible.first().index.coerceAtLeast(0)
    val maxFirst = (total - visible.size).coerceAtLeast(1)
    val progress = (first.toFloat() / maxFirst).coerceIn(0f, 1f)
    val fraction = (visible.size.toFloat() / total).coerceIn(0.08f, 1f)
    BoxWithConstraints(modifier) {
        val thumbHeight = (maxHeight * fraction).coerceAtLeast(24.dp)
        val top = (maxHeight - thumbHeight) * progress
        Box(
            Modifier
                .offset(y = top)
                .fillMaxWidth()
                .height(thumbHeight)
                .background(
                    MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.35f),
                    RoundedCornerShape(2.dp),
                ),
        )
    }
}

@Composable
private fun NoteRow(
    note: NoteItem,
    active: Boolean,
    bookmarked: Boolean,
    detail: String? = null,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(
                if (active) MaterialTheme.colorScheme.primary.copy(alpha = 0.15f)
                else Color.Transparent,
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Default.List,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(16.dp),
        )
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(
                note.basename,
                fontSize = 14.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val subtitle = detail ?: note.path.substringBeforeLast('/', "")
            if (subtitle.isNotEmpty()) {
                Text(
                    subtitle,
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (bookmarked) {
            Icon(
                Icons.Default.Star,
                contentDescription = "Bookmarked",
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun SearchPanel(state: UiState, controller: VibecodiumController) {
    var queryFocused by remember { mutableStateOf(false) }
    val focusRequester = remember { FocusRequester() }
    Column(Modifier.fillMaxSize()) {
        // Same field as the explorer's quick filter (`TreeList`): 36 dp tall,
        // 8 dp corners, #222222 on #383838, primary accent while focused.
        Box(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 5.dp),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(36.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(
                        if (queryFocused) Color(0xFF1B1B1B) else Color(0xFF222222),
                    )
                    .border(
                        width = if (queryFocused) 1.5.dp else 1.dp,
                        color = if (queryFocused) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            Color(0xFF383838)
                        },
                        shape = RoundedCornerShape(8.dp),
                    )
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                    ) {
                        focusRequester.requestFocus()
                    }
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    Icons.Default.Search,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = if (queryFocused) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                )
                BasicTextField(
                    value = state.searchQuery,
                    onValueChange = controller::search,
                    modifier = Modifier
                        .weight(1f)
                        .padding(horizontal = 8.dp)
                        .focusRequester(focusRequester)
                        .onFocusChanged { queryFocused = it.isFocused },
                    singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(
                        fontSize = 14.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                    ),
                    cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                    decorationBox = { innerTextField ->
                        Box(Modifier.fillMaxWidth()) {
                            if (!queryFocused && state.searchQuery.isEmpty()) {
                                Text(
                                    "Search notes...",
                                    fontSize = 14.sp,
                                    color = Color(0xFF8A8A8A),
                                )
                            }
                            innerTextField()
                        }
                    },
                )
                if (state.searchQuery.isNotEmpty()) {
                    IconButton(
                        onClick = { controller.search("") },
                        modifier = Modifier.size(28.dp),
                    ) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = "Clear search",
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
        when {
            state.searchQuery.isBlank() -> Text(
                "Type to search file names and note content.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            )

            state.searchHits.isEmpty() -> Text(
                "No matches.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            )

            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(state.searchHits, key = { "${it.path}:${it.line}" }) { hit ->
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clickable {
                                controller.openNote(hit.path)
                                controller.closeOverlays()
                            }
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text(
                            hit.name,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (hit.snippet.isNotBlank()) {
                            Text(
                                "${hit.line}: ${hit.snippet}",
                                fontSize = 11.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
                }
            }
        }
    }
}

@Composable
private fun EmptyEditor(controller: VibecodiumController) {
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("No tabs open", fontSize = 16.sp, fontWeight = FontWeight.Medium)
        Spacer(Modifier.height(12.dp))
        Button(onClick = { controller.newNote() }) {
            Text("New note")
        }
        Spacer(Modifier.height(8.dp))
        OutlinedButton(onClick = { controller.toggleDrawer(Drawer.Left) }) {
            Text("Open a note")
        }
    }
}

@Composable
private fun Placeholder(text: String) {
    Box(Modifier.fillMaxSize().padding(20.dp), contentAlignment = Alignment.Center) {
        Text(
            text,
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun BoxScope.MessageBar(message: String, onDismiss: () -> Unit) {
    Surface(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(16.dp)
            .fillMaxWidth(),
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 6.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(message, modifier = Modifier.weight(1f), fontSize = 13.sp)
            TextButton(onClick = onDismiss) {
                Text("OK")
            }
        }
    }
}

@Composable
private fun DeleteDialog(path: String, controller: VibecodiumController) {
    AlertDialog(
        onDismissRequest = { controller.dismissDelete() },
        title = { Text("Delete note?") },
        text = { Text(path) },
        confirmButton = {
            TextButton(onClick = { controller.deleteConfirmed() }) {
                Text("Delete")
            }
        },
        dismissButton = {
            TextButton(onClick = { controller.dismissDelete() }) {
                Text("Cancel")
            }
        },
    )
}

@Composable
private fun RenameDialog(state: UiState, controller: VibecodiumController) {
    AlertDialog(
        onDismissRequest = { controller.dismissRename() },
        title = { Text("Rename note") },
        text = {
            OutlinedTextField(
                value = state.renameValue,
                onValueChange = controller::updateRenameValue,
                singleLine = true,
                label = { Text("Name") },
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { controller.confirmRename() }) {
                Text("Rename")
            }
        },
        dismissButton = {
            TextButton(onClick = { controller.dismissRename() }) {
                Text("Cancel")
            }
        },
    )
}

@Composable
private fun NewFolderDialog(state: UiState, controller: VibecodiumController) {
    AlertDialog(
        onDismissRequest = { controller.dismissNewFolder() },
        title = { Text("New folder") },
        text = {
            OutlinedTextField(
                value = state.newFolderValue,
                onValueChange = controller::updateNewFolderValue,
                singleLine = true,
                label = { Text("Folder name") },
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { controller.confirmNewFolder() }) {
                Text("Create")
            }
        },
        dismissButton = {
            TextButton(onClick = { controller.dismissNewFolder() }) {
                Text("Cancel")
            }
        },
    )
}

private enum class SettingsSection(val title: String, val icon: ImageVector) {
    Appearance("Appearance", Icons.Default.Settings),
    Editor("Editor", Icons.Default.Edit),
    Files("Files & links", Icons.Default.List),
    Sync("Sync", Icons.Default.Refresh),
    Advanced("Advanced", Icons.Default.Build),
    About("About", Icons.Default.Info),
}

@Composable
private fun SettingsScreen(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    // Section navigation is local: changing a settings section must not
    // replace the app-wide UiState and recompose the drawer/editor behind it.
    var sectionName by rememberSaveable { mutableStateOf(state.settingsSection ?: "") }
    LaunchedEffect(state.settingsSection) {
        sectionName = state.settingsSection ?: ""
    }
    val section = SettingsSection.entries.firstOrNull { it.name == sectionName }
    BackHandler(enabled = section != null) { sectionName = "" }
    Surface(modifier = modifier, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxSize()) {
            SettingsHeader(
                title = section?.title ?: "Settings",
                showBack = section != null,
                onBack = { sectionName = "" },
                onClose = { controller.closeSettings() },
            )
            AnimatedContent(
                targetState = section,
                transitionSpec = {
                    if (targetState != null) {
                        (slideInHorizontally(tween(140)) { it } + fadeIn(tween(90))) togetherWith
                            (slideOutHorizontally(tween(140)) { -it / 4 } + fadeOut(tween(90)))
                    } else {
                        (slideInHorizontally(tween(140)) { -it / 4 } + fadeIn(tween(90))) togetherWith
                            (slideOutHorizontally(tween(140)) { it } + fadeOut(tween(90)))
                    }
                },
                label = "settings-section",
                modifier = Modifier.fillMaxSize(),
            ) { current ->
                if (current == null) {
                    SettingsRoot(onOpen = { sectionName = it.name })
                } else {
                    SettingsSectionContent(current, state, controller)
                }
            }
        }
    }
}

@Composable
private fun SettingsHeader(
    title: String,
    showBack: Boolean,
    onBack: () -> Unit,
    onClose: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(52.dp)
            .padding(start = 4.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (showBack) {
            IconButton(onClick = onBack, modifier = Modifier.size(44.dp)) {
                Icon(
                    Icons.Default.ArrowBack,
                    contentDescription = "Back",
                    modifier = Modifier.size(20.dp),
                )
            }
        } else {
            Spacer(Modifier.width(12.dp))
        }
        Text(
            title,
            modifier = Modifier
                .weight(1f)
                .padding(start = 4.dp),
            fontSize = 15.sp,
            fontWeight = FontWeight.SemiBold,
        )
        IconButton(onClick = onClose, modifier = Modifier.size(44.dp)) {
            Icon(
                Icons.Default.Close,
                contentDescription = "Close settings",
                modifier = Modifier.size(20.dp),
            )
        }
    }
    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
}

@Composable
private fun SettingsRoot(onOpen: (SettingsSection) -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        SettingsGroup("Options") {
            SettingsSection.entries.forEachIndexed { index, section ->
                SettingsNavRow(
                    icon = section.icon,
                    title = section.title,
                    showDivider = index != SettingsSection.entries.lastIndex,
                ) { onOpen(section) }
            }
        }
    }
}

@Composable
private fun SettingsSectionContent(
    section: SettingsSection,
    state: UiState,
    controller: VibecodiumController,
) {
    val settings = state.settings
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        when (section) {
            SettingsSection.Appearance -> {
                SettingsGroup("Text") {
                    SettingsSliderRow(
                        title = "Text scale",
                        description = "Scales interface text. The native editor keeps its own font size.",
                        value = settings.textScale,
                        range = AppSettingsJson.MIN_TEXT_SCALE..AppSettingsJson.MAX_TEXT_SCALE,
                        format = { "${(it * 100).roundToInt()}%" },
                        onCommit = controller::setTextScale,
                    )
                }
                SettingsGroup("Interface") {
                    SettingsSwitchRow(
                        title = "Auto-hide header",
                        description = "Hide the header and floating buttons while scrolling",
                        checked = settings.autoHideChrome,
                        onChange = controller::setAutoHideChrome,
                        showDivider = true,
                    )
                    SettingsSwitchRow(
                        title = "Bottom navbar",
                        description = "Experimental floating navigation bar",
                        checked = settings.navbarEnabled,
                        onChange = controller::setNavbarEnabled,
                        showDivider = false,
                    )
                }
            }

            SettingsSection.Editor -> {
                SettingsGroup("View") {
                    SettingsChoiceRow(
                        title = "Default view",
                        description = "Used when a note is opened in a new tab",
                        value = settings.defaultEditorMode,
                        options = listOf(
                            "Editing" to EditorMode.Source,
                            "Reading" to EditorMode.Preview,
                        ),
                        onSelect = controller::setDefaultEditorMode,
                    )
                }
                SettingsGroup("Saving") {
                    SettingsChoiceRow(
                        title = "Autosave delay",
                        description = "How long to wait after the last edit",
                        value = settings.autoSaveDelayMs,
                        options = listOf(
                            "300 ms" to 300,
                            "600 ms" to 600,
                            "1.2 s" to 1200,
                        ),
                        onSelect = controller::setAutoSaveDelay,
                    )
                }
            }

            SettingsSection.Files -> {
                SettingsGroup("Vault") {
                    SettingsInfoRow("Name", state.vaultName ?: "—")
                    SettingsInfoRow(
                        "Storage",
                        if (state.vaultKind == "saf") "SAF folder" else "Local folder",
                        showDivider = state.vaultRoot != null,
                    )
                    state.vaultRoot?.let {
                        SettingsInfoRow("Location", "Vault folder", detail = it, showDivider = false)
                    }
                }
                SettingsGroup("Actions") {
                    SettingsActionRow(
                        title = "Refresh files",
                        description = "Rescan the vault tree",
                        onClick = { controller.refreshNotes() },
                        showDivider = true,
                    )
                    SettingsActionRow(
                        title = "Reset folder state",
                        description = "Expand every folder again",
                        onClick = { controller.resetCollapsedFolders() },
                        showDivider = false,
                    )
                }
            }

            SettingsSection.Sync -> SyncSettingsContent(state, controller)

            SettingsSection.Advanced -> {
                SettingsGroup("Preferences") {
                    SettingsActionRow(
                        title = "Reset settings",
                        description = "Restore every app preference to its default",
                        onClick = { controller.resetSettings() },
                        showDivider = true,
                    )
                    SettingsActionRow(
                        title = "Refresh search index",
                        description = "Rebuild the full-text index for this vault",
                        onClick = { controller.rebuildSearchIndex() },
                        showDivider = false,
                    )
                }
            }

            SettingsSection.About -> {
                val coreVersion = remember {
                    runCatching { CoreJson.data(CoreBridge.nativeVersion()).optString("version") }
                        .getOrDefault("0.1.0")
                }
                SettingsGroup("App") {
                    SettingsInfoRow(
                        "vibecodium",
                        "0.1.0",
                        detail = "Rust core · Compose shell",
                    )
                    SettingsInfoRow("Rust core", coreVersion)
                    SettingsInfoRow("Platform", "Android · Compose", showDivider = false)
                }
                SettingsGroup("Vault") {
                    SettingsInfoRow(
                        "Name",
                        state.vaultName ?: "No vault open",
                        showDivider = state.vaultRoot != null,
                    )
                    state.vaultRoot?.let {
                        SettingsInfoRow("Location", "Vault folder", detail = it, showDivider = false)
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsGroup(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            title,
            modifier = Modifier.padding(start = 16.dp, bottom = 8.dp),
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(MaterialTheme.colorScheme.background),
            content = content,
        )
    }
}

@Composable
private fun SettingsNavRow(
    icon: ImageVector,
    title: String,
    showDivider: Boolean,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(14.dp))
        Text(
            title,
            modifier = Modifier.weight(1f),
            fontSize = 14.sp,
            fontWeight = FontWeight.Medium,
        )
        Icon(
            Icons.Default.ArrowForward,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 52.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun SettingsSwitchRow(
    title: String,
    description: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    showDivider: Boolean,
    enabled: Boolean = true,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .alpha(if (enabled) 1f else 0.5f)
            .heightIn(min = 60.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 14.sp, fontWeight = FontWeight.Medium)
            if (description.isNotBlank()) {
                Text(
                    description,
                    modifier = Modifier.padding(top = 2.dp),
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled)
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun SettingsSliderRow(
    title: String,
    description: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    format: (Float) -> String,
    onCommit: (Float) -> Unit,
) {
    var draft by remember(value) { mutableStateOf(value) }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(title, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                if (description.isNotBlank()) {
                    Text(
                        description,
                        modifier = Modifier.padding(top = 2.dp),
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                format(draft),
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Slider(
            value = draft,
            onValueChange = { draft = it },
            onValueChangeFinished = { onCommit(draft) },
            valueRange = range,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 2.dp),
        )
    }
}

@Composable
private fun <T> SettingsChoiceRow(
    title: String,
    description: String,
    value: T,
    options: List<Pair<String, T>>,
    onSelect: (T) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val label = options.firstOrNull { it.second == value }?.first ?: ""
    Box(Modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { open = true }
                .heightIn(min = 56.dp)
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                if (description.isNotBlank()) {
                    Text(
                        description,
                        modifier = Modifier.padding(top = 2.dp),
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                label,
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.width(4.dp))
            Icon(
                Icons.Default.KeyboardArrowDown,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { (optionLabel, optionValue) ->
                val selected = optionValue == value
                DropdownMenuItem(
                    text = { Text(optionLabel) },
                    trailingIcon = {
                        if (selected) {
                            Icon(
                                Icons.Default.Check,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                            )
                        }
                    },
                    onClick = {
                        open = false
                        onSelect(optionValue)
                    },
                )
            }
        }
    }
}

@Composable
private fun SettingsActionRow(
    title: String,
    description: String,
    onClick: () -> Unit,
    showDivider: Boolean,
    enabled: Boolean = true,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .alpha(if (enabled) 1f else 0.5f)
            .clickable(enabled = enabled, onClick = onClick)
            .heightIn(min = 56.dp)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                title,
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.primary,
            )
            if (description.isNotBlank()) {
                Text(
                    description,
                    modifier = Modifier.padding(top = 2.dp),
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun SettingsInfoRow(
    title: String,
    value: String,
    detail: String = "",
    showDivider: Boolean = true,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                title,
                modifier = Modifier.weight(1f),
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
            )
            Text(
                value,
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (detail.isNotBlank()) {
            Text(
                detail,
                modifier = Modifier.padding(top = 2.dp),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun ColumnScope.SyncSettingsContent(state: UiState, controller: VibecodiumController) {
    val sync = state.sync
    val vaultSync = state.vaultSync
    SettingsGroup("Sync") {
        SyncRunCard(
            busy = sync.busy,
            connected = sync.connected,
            paused = vaultSync.paused,
            progress = sync.progress,
            onSync = { controller.syncNow() },
            onStop = { controller.cancelSync() },
        )
    }

    SettingsGroup("Connection") {
        SettingsInfoRow(
            "Status",
            when {
                sync.busy -> when {
                    sync.progressTotal > 0 -> "${sync.progressDone}/${sync.progressTotal}"
                    sync.localFiles > 0 -> "${sync.localFiles} files"
                    else -> "Syncing"
                }
                vaultSync.paused -> "Paused"
                sync.connected -> "Connected"
                else -> "Not connected"
            },
            detail = if (sync.busy) "Syncing" else sync.accountName,
        )
        SettingsInfoRow(
            "Remote folder",
            controller.syncRemoteFolder(),
            detail = "Dropbox folder for this vault",
        )
        SettingsInfoRow(
            "Files",
            if (sync.localFiles == 0 && sync.remoteFiles == 0) {
                "—"
            } else {
                "${sync.localFiles} local · ${sync.remoteFiles} remote"
            },
        )
        SettingsInfoRow(
            "To sync",
            if (sync.planned == 0) "—" else sync.planned.toString(),
        )
        SettingsInfoRow(
            "Last sync",
            if (sync.lastSync > 0) dateTimeLabel(sync.lastSync) else "Never",
            showDivider = sync.message.isNotBlank(),
        )
        if (sync.message.isNotBlank()) {
            SettingsInfoRow("Last result", sync.message, showDivider = false)
        }
    }

    SettingsGroup("Dropbox account") {
        if (sync.connected) {
            SettingsActionRow(
                title = "Disconnect Dropbox",
                description = "Remove the stored Dropbox token from this device",
                enabled = !sync.busy,
                onClick = { controller.disconnectDropbox() },
                showDivider = false,
            )
        } else {
            SettingsActionRow(
                title = "Connect Dropbox",
                description = "Sign in and start syncing this vault",
                enabled = !sync.busy,
                onClick = { controller.connectDropbox() },
                showDivider = false,
            )
        }
    }

    if (!sync.connected) {
        SettingsGroup("Setup") {
            SettingsTextFieldRow(
                title = "Dropbox app key",
                description = "Loopback redirect: http://127.0.0.1:47219/",
                value = sync.clientId,
                placeholder = "Dropbox app key",
                onValueChange = controller::updateSyncClientId,
                showDivider = false,
            )
        }
    }

    SettingsGroup("Automatic") {
        SettingsSwitchRow(
            title = "Sync on open",
            description = "Run a pass when this vault opens",
            checked = sync.autoSync,
            onChange = controller::setAutoSync,
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Pause sync",
            description = "Block manual and automatic passes",
            checked = vaultSync.paused,
            onChange = controller::setSyncPaused,
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Prevent sleep",
            description = "Keep the device awake while a sync pass runs",
            checked = vaultSync.preventSleep,
            onChange = controller::setPreventSleep,
            showDivider = false,
        )
    }

    SettingsGroup("Selective sync") {
        SettingsSwitchRow(
            title = "Notes",
            description = "Markdown notes always sync",
            checked = true,
            onChange = {},
            enabled = false,
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Images",
            description = "PNG, JPEG, GIF, WebP, SVG",
            checked = vaultSync.allowImages,
            onChange = { controller.setSyncCategory(SyncFileCategory.Images, it) },
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Audio",
            description = "MP3, WAV, FLAC, OGG, M4A",
            checked = vaultSync.allowAudio,
            onChange = { controller.setSyncCategory(SyncFileCategory.Audio, it) },
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Video",
            description = "MP4, MOV, WebM, MKV",
            checked = vaultSync.allowVideo,
            onChange = { controller.setSyncCategory(SyncFileCategory.Video, it) },
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "PDFs",
            description = "Portable document files",
            checked = vaultSync.allowPdf,
            onChange = { controller.setSyncCategory(SyncFileCategory.Pdf, it) },
            showDivider = true,
        )
        SettingsSwitchRow(
            title = "Other files",
            description = "Code, documents and extensions not listed above",
            checked = vaultSync.allowOther,
            onChange = { controller.setSyncCategory(SyncFileCategory.Other, it) },
            showDivider = false,
        )
    }

    SettingsGroup("Excluded folders") {
        SettingsFolderEditor(
            folders = vaultSync.excludedFolders,
            onAdd = controller::addSyncExcludedFolder,
            onRemove = controller::removeSyncExcludedFolder,
        )
    }

    SettingsGroup("Advanced") {
        SettingsTextFieldRow(
            title = "Device name",
            description = "Stored with this vault's sync profile",
            value = vaultSync.deviceName,
            placeholder = "Android device",
            onValueChange = controller::setSyncDeviceName,
            showDivider = true,
        )
        SettingsInfoRow(
            "Conflict handling",
            "Conflict copy",
            detail = "Automatic merge is not available for Dropbox sync",
        )
        SettingsInfoRow(
            "Version history",
            "Dropbox",
            detail = "Use the Dropbox app to restore previous versions",
        )
        SettingsInfoRow(
            "Deleted files",
            "Dropbox",
            detail = "Use the Dropbox app to restore deleted files",
        )
        SettingsInfoRow(
            "Vault size",
            "Dropbox",
            detail = "Storage usage is shown in the Dropbox account",
            showDivider = false,
        )
    }

    SettingsGroup("Vault configuration") {
        SettingsInfoRow(
            "App settings",
            "Local only",
            detail = "Plugins, hotkeys and themes are not synchronised by this build",
            showDivider = false,
        )
    }
}

/**
 * The manual sync control and its progress share one fixed-height card. The
 * progress slot exists in both states, so starting or stopping a pass never
 * moves the rest of the settings page.
 */
@Composable
private fun SyncRunCard(
    busy: Boolean,
    connected: Boolean,
    paused: Boolean,
    progress: String,
    onSync: () -> Unit,
    onStop: () -> Unit,
) {
    val enabled = busy || (connected && !paused)
    val title = if (busy) "Stop sync" else "Sync now"
    val description = when {
        busy -> progress.ifBlank { "Working…" }
        paused -> "Sync is paused"
        !connected -> "Connect Dropbox first"
        else -> "Run a sync pass now"
    }
    val accent = when {
        busy -> MaterialTheme.colorScheme.error
        enabled -> MaterialTheme.colorScheme.primary
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 76.dp)
            .alpha(if (enabled) 1f else 0.5f)
            .clickable(enabled = enabled) { if (busy) onStop() else onSync() }
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Medium,
                    color = accent,
                )
                Text(
                    description,
                    modifier = Modifier.padding(top = 2.dp),
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(12.dp))
            Icon(
                if (busy) Icons.Default.Close else Icons.Default.Refresh,
                contentDescription = if (busy) "Stop sync" else "Sync now",
                modifier = Modifier.size(22.dp),
                tint = accent,
            )
        }
        Spacer(Modifier.height(10.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .height(4.dp),
        ) {
            if (busy) {
                LinearProgressIndicator(Modifier.fillMaxSize())
            }
        }
    }
}

@Composable
private fun SettingsTextFieldRow(
    title: String,
    description: String,
    value: String,
    placeholder: String,
    onValueChange: (String) -> Unit,
    showDivider: Boolean,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Text(title, fontSize = 14.sp, fontWeight = FontWeight.Medium)
        if (description.isNotBlank()) {
            Text(
                description,
                modifier = Modifier.padding(top = 2.dp),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            placeholder = { Text(placeholder, fontSize = 13.sp) },
            singleLine = true,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 8.dp),
        )
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun SettingsFolderEditor(
    folders: List<String>,
    onAdd: (String) -> Unit,
    onRemove: (String) -> Unit,
) {
    var draft by remember { mutableStateOf("") }
    Column(Modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                placeholder = { Text("folder/path", fontSize = 13.sp) },
                singleLine = true,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            IconButton(
                onClick = {
                    if (draft.isNotBlank()) {
                        onAdd(draft)
                        draft = ""
                    }
                },
                enabled = draft.isNotBlank(),
                modifier = Modifier.size(44.dp),
            ) {
                Icon(
                    Icons.Default.Add,
                    contentDescription = "Add excluded folder",
                    modifier = Modifier.size(20.dp),
                )
            }
        }
        if (folders.isEmpty()) {
            Text(
                "No folders excluded",
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            folders.forEachIndexed { index, folder ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 44.dp)
                        .padding(start = 16.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(folder, modifier = Modifier.weight(1f), fontSize = 13.sp)
                    IconButton(
                        onClick = { onRemove(folder) },
                        modifier = Modifier.size(40.dp),
                    ) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = "Remove $folder",
                            modifier = Modifier.size(16.dp),
                        )
                    }
                }
                if (index != folders.lastIndex) {
                    HorizontalDivider(
                        modifier = Modifier.padding(start = 16.dp),
                        color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
                    )
                }
            }
        }
    }
}

private fun syncSubtitle(state: UiState): String {
    val sync = state.sync
    return when {
        sync.busy -> if (sync.progressTotal > 0) {
            "Syncing ${sync.progressDone}/${sync.progressTotal}"
        } else {
            "Syncing…"
        }
        sync.connected && sync.lastSync > 0 -> "Synced ${timeLabel(sync.lastSync)}"
        sync.connected -> "Dropbox connected"
        state.vaultKind == "saf" -> "SAF vault"
        else -> "vibecodium"
    }
}

private fun timeLabel(millis: Long): String =
    SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date(millis))

private fun dateTimeLabel(millis: Long): String =
    SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault()).format(Date(millis))

@Composable
private fun AttachmentViewer(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    val path = state.viewerPath ?: return
    val bytes by produceState<ByteArray?>(initialValue = null, key1 = path) {
        value = controller.loadFile(path)
    }
    Surface(modifier = modifier, color = Color.Black) {
        Box(Modifier.fillMaxSize()) {
            val data = bytes
            if (data == null) {
                Text(
                    "Cannot load ${path.substringAfterLast('/')}",
                    color = Color.White,
                    modifier = Modifier.align(Alignment.Center),
                )
            } else {
                val bitmap = remember(data) {
                    BitmapFactory.decodeByteArray(data, 0, data.size)
                }
                if (bitmap == null) {
                    Text(
                        "Unsupported image: ${path.substringAfterLast('/')}",
                        color = Color.White,
                        modifier = Modifier.align(Alignment.Center),
                    )
                } else {
                    Image(
                        bitmap = bitmap.asImageBitmap(),
                        contentDescription = path,
                        modifier = Modifier.fillMaxSize().padding(8.dp),
                        contentScale = ContentScale.Fit,
                    )
                }
            }
            IconButton(
                onClick = { controller.closeAttachment() },
                modifier = Modifier.align(Alignment.TopEnd).padding(8.dp),
            ) {
                Icon(
                    Icons.Default.Close,
                    contentDescription = "Close",
                    tint = Color.White,
                    modifier = Modifier.size(24.dp),
                )
            }
        }
    }
}
