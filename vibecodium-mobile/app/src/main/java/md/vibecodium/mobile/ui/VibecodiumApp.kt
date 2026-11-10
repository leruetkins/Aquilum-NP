package md.vibecodium.mobile.ui

import android.graphics.BitmapFactory
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
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
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
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
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt
import androidx.compose.ui.viewinterop.AndroidView
import md.vibecodium.mobile.Drawer
import md.vibecodium.mobile.DrawerMode
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

/** The floating bottom navbar is temporarily disabled. */
private const val MobileNavbarEnabled = false

/** `--view-header-height: var(--touch-size-m)` on a phone. */
private val ViewHeaderHeight = 44.dp

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
    VibecodiumTheme {
        BackHandler(
            enabled = state.drawer != null || state.tabSwitcher || state.viewerPath != null ||
                state.historyIndex > 0,
        ) {
            controller.back()
        }
        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            when {
                state.booting -> BootScreen()
                state.vaultName == null -> StartScreen(state, controller)
                else -> PhoneShell(state, controller)
            }
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
        val imeVisible = WindowInsets.isImeVisible
        val focusManager = LocalFocusManager.current
        var chromeHidden by remember { mutableStateOf(false) }
        val lastEditorScroll = remember { floatArrayOf(0f) }
        val chromeProgress = animateFloatAsState(
            targetValue = if (chromeHidden) 0f else 1f,
            animationSpec = tween(160),
            label = "chrome",
        )
        val navProgress = animateFloatAsState(
            targetValue = if (MobileNavbarEnabled && !imeVisible && !chromeHidden) 1f else 0f,
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
            editor.value.activeTab,
            editor.value.editorMode,
        ) {
            chromeHidden = false
            lastEditorScroll[0] = 0f
        }
        val onEditorScroll: (Float, Float) -> Unit = { scroll, maxY ->
            val previous = lastEditorScroll[0]
            if (maxY <= 0f || scroll <= 2f) {
                chromeHidden = false
            } else if (scroll > previous + 24f) {
                chromeHidden = true
            } else if (scroll < previous - 24f) {
                chromeHidden = false
            }
            lastEditorScroll[0] = scroll
        }
        Box(Modifier.fillMaxSize()) {
            EditorArea(
                editor.value,
                controller,
                imeVisible,
                chromeHidden,
                state.drawer != null,
                onEditorScroll,
            )

            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .graphicsLayer {
                        alpha = chromeProgress.value
                        translationY = -56.dp.toPx() * (1f - chromeProgress.value)
                    }
                    .then(
                        if (chromeHidden) Modifier.semantics { invisibleToUser() } else Modifier,
                    ),
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
                        .padding(start = 12.dp)
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
                    .then(
                        if (MobileNavbarEnabled && !imeVisible && !chromeHidden) {
                            Modifier
                        } else {
                            Modifier.semantics { invisibleToUser() }
                        },
                    ),
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

            if (state.syncOpen) {
                SyncScreen(state, controller, Modifier.fillMaxSize())
            }

            if (state.viewerPath != null) {
                AttachmentViewer(state, controller, Modifier.fillMaxSize())
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
    chromeHidden: Boolean,
    drawerOpen: Boolean,
    onEditorScroll: (Float, Float) -> Unit,
) {
    val density = LocalDensity.current
    val imeBottomPx = WindowInsets.ime.getBottom(density)
    val navInsetPx = if (MobileNavbarEnabled) {
        with(density) { (NavbarHeight + 12.dp).toPx() }
    } else {
        0f
    }
    val bottomInsetPx = if (imeBottomPx > 0) imeBottomPx.toFloat() else navInsetPx
    if (editor.tabs.isEmpty()) {
        EmptyEditor(controller)
    } else {
        EditorScreen(
            editor,
            controller,
            imeVisible,
            bottomInsetPx,
            chromeHidden,
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
    controller: VibecodiumController,
    imeVisible: Boolean,
    bottomInsetPx: Float,
    chromeHidden: Boolean,
    drawerOpen: Boolean,
    onEditorScroll: (Float, Float) -> Unit,
) {
    val tab = state.tabs.getOrNull(state.activeTab) ?: return
    val bottomInset = if (imeVisible || !MobileNavbarEnabled) 0.dp else NavbarHeight + 12.dp
    val preview = state.editorMode == EditorMode.Preview
    val topInsetPx = with(LocalDensity.current) { (ViewHeaderHeight + 10.dp).toPx() }
    val headerProgress = animateFloatAsState(
        targetValue = if (chromeHidden) 0f else 1f,
        animationSpec = tween(160),
        label = "header",
    )
    var editorView by remember { mutableStateOf<NativeEditorView?>(null) }
    var keyboardPrompt by remember { mutableStateOf(false) }
    LaunchedEffect(tab.path, preview) {
        keyboardPrompt = false
    }
    LaunchedEffect(drawerOpen) {
        if (drawerOpen) editorView?.hideKeyboard()
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
            onViewReady = { editorView = it },
            onCaretPlaced = { keyboardPrompt = true },
        )
        if (preview) {
            Surface(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        awaitPointerEventScope {
                            while (true) {
                                awaitPointerEvent().changes.forEach { it.consume() }
                            }
                        }
                    },
                color = MaterialTheme.colorScheme.background,
            ) {
                if (tab.language != null) {
                    CodeViewer(
                        tab,
                        Modifier.fillMaxSize(),
                        ViewHeaderHeight + 10.dp,
                        bottomInset,
                        onEditorScroll,
                    )
                } else {
                    MarkdownPreview(
                        state,
                        controller,
                        tab,
                        ViewHeaderHeight,
                        bottomInset,
                        onEditorScroll,
                    )
                }
            }
        }
        Box(
            Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .graphicsLayer {
                    alpha = headerProgress.value
                    translationY = -ViewHeaderHeight.toPx() * (1f - headerProgress.value)
                }
                .then(
                    if (chromeHidden) Modifier.semantics { invisibleToUser() } else Modifier,
                )
                .background(MaterialTheme.colorScheme.background),
        ) {
            ViewHeader(state, tab, controller)
        }
        if (!preview && keyboardPrompt && !imeVisible) {
            Surface(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(top = 8.dp, end = 12.dp)
                    .size(44.dp)
                    .offset {
                        IntOffset(
                            0,
                            (ViewHeaderHeight.toPx() * headerProgress.value).roundToInt(),
                        )
                    },
                shape = CircleShape,
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 4.dp,
            ) {
                IconButton(
                    onClick = { editorView?.showKeyboard() },
                    modifier = Modifier.fillMaxSize(),
                ) {
                    Icon(
                        KeyboardIcon,
                        contentDescription = "Show keyboard",
                        modifier = Modifier.size(22.dp),
                        tint = MaterialTheme.colorScheme.onSurface,
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
            onClick = { onDismiss(); controller.openSync() },
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
            onClick = { onDismiss(); controller.closeVault() },
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
    Column(modifier.padding(top = topInset).background(colors.background)) {
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
        Row(Modifier.fillMaxWidth().weight(1f).verticalScroll(vertical)) {
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
                    .padding(end = 12.dp, bottom = 12.dp),
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
private fun LeftDrawer(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    var vaultMenuOpen by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    Surface(modifier = modifier, color = Color(0xFF282828), tonalElevation = 0.dp) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(58.dp)
                    .padding(start = 16.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.weight(1f)) {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clickable { vaultMenuOpen = true }
                            .padding(vertical = 4.dp),
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                state.vaultName ?: "Vault",
                                fontSize = 15.sp,
                                fontWeight = FontWeight.SemiBold,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Icon(
                                Icons.Default.KeyboardArrowDown,
                                contentDescription = "Vault options",
                                modifier = Modifier.size(20.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Text(
                            syncSubtitle(state),
                            fontSize = 11.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    DropdownMenu(expanded = vaultMenuOpen, onDismissRequest = { vaultMenuOpen = false }) {
                        DropdownMenuItem(
                            text = { Text("Refresh files") },
                            onClick = { vaultMenuOpen = false; controller.refreshNotes() },
                        )
                        DropdownMenuItem(
                            text = { Text("Open another vault") },
                            onClick = { vaultMenuOpen = false; controller.closeVault() },
                        )
                        DropdownMenuItem(
                            text = { Text("Close drawer") },
                            onClick = { vaultMenuOpen = false; controller.closeOverlays() },
                        )
                    }
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
                            DrawerMode.Bookmarks -> BookmarkedList(state, controller)
                            DrawerMode.Recent -> RecentList(state, controller)
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
        Row(
            Modifier
                .fillMaxWidth()
                .height(44.dp)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Default.Search,
                contentDescription = null,
                modifier = Modifier.size(22.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            BasicTextField(
                value = state.fileQuery,
                onValueChange = controller::setFileQuery,
                modifier = Modifier
                    .weight(1f)
                    .padding(horizontal = 9.dp)
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
                    modifier = Modifier.size(36.dp),
                ) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = "Clear filter",
                        modifier = Modifier.size(20.dp),
                    )
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
            Icons.Default.Refresh,
            "Refresh files",
            modifier = Modifier.weight(1f).fillMaxHeight(),
        ) {
            controller.refreshNotes()
        }
    }
}

@Composable
private fun CompactToolbarButton(
    icon: ImageVector,
    description: String,
    enabled: Boolean = true,
    tint: Color = MaterialTheme.colorScheme.onSurfaceVariant,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, modifier = modifier, enabled = enabled) {
        Icon(
            icon,
            contentDescription = description,
            modifier = Modifier.size(26.dp),
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
    LazyColumn(Modifier.fillMaxSize()) {
        items(notes, key = { it.path }) { note ->
            NoteRow(
                note,
                active = state.tabs.getOrNull(state.activeTab)?.path == note.path,
                bookmarked = true,
            ) {
                controller.openNote(note.path)
                controller.closeOverlays()
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
        }
    }
}

@Composable
private fun RecentList(state: UiState, controller: VibecodiumController) {
    val byPath = state.notes.associateBy { it.path }
    val recent = state.history.asReversed().distinct().mapNotNull { byPath[it] }.take(30)
    if (recent.isEmpty()) {
        Placeholder("Notes you open will appear here.")
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        items(recent, key = { it.path }) { note ->
            NoteRow(
                note,
                active = false,
                bookmarked = note.path in state.bookmarks,
            ) {
                controller.openNote(note.path)
                controller.closeOverlays()
            }
        }
    }
}

@Composable
private fun NoteRow(
    note: NoteItem,
    active: Boolean,
    bookmarked: Boolean,
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
            val folder = note.path.substringBeforeLast('/', "")
            if (folder.isNotEmpty()) {
                Text(
                    folder,
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
    Column(Modifier.fillMaxSize().padding(10.dp)) {
        OutlinedTextField(
            value = state.searchQuery,
            onValueChange = controller::search,
            placeholder = { Text("Search notes") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        when {
            state.searchQuery.isBlank() -> Text(
                "Type to search file names and note content.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            state.searchHits.isEmpty() -> Text(
                "No matches.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
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
                            .padding(vertical = 8.dp),
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

@Composable
private fun SyncScreen(state: UiState, controller: VibecodiumController, modifier: Modifier) {
    val sync = state.sync
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
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
                    "Sync",
                    modifier = Modifier.weight(1f),
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                IconButton(onClick = { controller.closeSync() }) {
                    Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (sync.connected) {
                    Text(
                        "Connected as ${sync.accountName.ifBlank { "Dropbox" }}",
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                    )
                    if (sync.lastSync > 0) {
                        Text("Last sync: ${dateTimeLabel(sync.lastSync)}", fontSize = 12.sp, color = muted)
                    }
                } else {
                    Text("Connect Dropbox to sync this vault.", fontSize = 14.sp)
                    OutlinedTextField(
                        value = sync.clientId,
                        onValueChange = controller::updateSyncClientId,
                        label = { Text("Dropbox app key") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        "The app uses the loopback redirect http://127.0.0.1:47219/ " +
                            "registered for this key. Provide your own app key if the default one " +
                            "is not available to you.",
                        fontSize = 11.sp,
                        color = muted,
                    )
                    Button(
                        onClick = { controller.connectDropbox() },
                        enabled = !sync.busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Connect Dropbox")
                    }
                }

                if (sync.connected) {
                    Button(
                        onClick = { controller.syncNow() },
                        enabled = !sync.busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Sync now")
                    }
                    OutlinedButton(
                        onClick = { controller.disconnectDropbox() },
                        enabled = !sync.busy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Disconnect")
                    }
                }

                if (sync.busy) {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(
                        sync.progress.ifBlank { "Working…" },
                        fontSize = 12.sp,
                        color = muted,
                    )
                }

                if (sync.message.isNotBlank()) {
                    val isError = sync.message.contains("fail", ignoreCase = true) ||
                        sync.message.contains("Refusing", ignoreCase = true) ||
                        sync.message.contains("not connected", ignoreCase = true)
                    Text(
                        sync.message,
                        fontSize = 12.sp,
                        color = if (isError) MaterialTheme.colorScheme.error else muted,
                    )
                }

                HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))

                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Sync on open", fontSize = 14.sp)
                        Text("Run a pass when this vault opens", fontSize = 11.sp, color = muted)
                    }
                    Switch(
                        checked = sync.autoSync,
                        onCheckedChange = controller::setAutoSync,
                        enabled = !sync.busy,
                    )
                }

                HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
                Text("What syncs", fontSize = 13.sp, fontWeight = FontWeight.Medium)
                Text(
                    "Markdown notes and attachments. Hidden service folders " +
                        "(.vault, .git, .trash, node_modules, target) stay local.",
                    fontSize = 12.sp,
                    color = muted,
                )
                Text(
                    "Conflicts keep the local version at the original path and preserve " +
                        "the remote version as \u201cname (conflict date).ext\u201d, locally and on Dropbox.",
                    fontSize = 12.sp,
                    color = muted,
                )
                Text(
                    "Deleting more than half of 10+ files aborts the pass instead of propagating a wipe.",
                    fontSize = 12.sp,
                    color = muted,
                )
            }
        }
    }
}

private fun syncSubtitle(state: UiState): String {
    val sync = state.sync
    return when {
        sync.busy -> "syncing…"
        sync.connected && sync.lastSync > 0 -> "synced ${timeLabel(sync.lastSync)}"
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
