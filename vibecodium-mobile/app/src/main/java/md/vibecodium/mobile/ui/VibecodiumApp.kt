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
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowRight
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameMillis
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
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
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
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
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
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
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
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
import md.vibecodium.mobile.EditorFontFace
import md.vibecodium.mobile.HeadingsSettings
import md.vibecodium.mobile.HeadingColors
import md.vibecodium.mobile.HeadingStyle
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
import md.vibecodium.mobile.SyncProgressKind
import md.vibecodium.mobile.Tab
import md.vibecodium.mobile.ThemeEntry
import md.vibecodium.mobile.ThemeMode
import md.vibecodium.mobile.ThemeSlot
import md.vibecodium.mobile.UiState
import md.vibecodium.mobile.VibecodiumController
import md.vibecodium.mobile.SearchFileResult
import md.vibecodium.mobile.SearchMatch
import md.vibecodium.mobile.FindUiState
import md.vibecodium.mobile.ExplorerRow
import md.vibecodium.mobile.expandDailyNoteFormat
import md.vibecodium.mobile.explorerDisplayName
import md.vibecodium.mobile.filterExplorerTree
import md.vibecodium.mobile.filteredAncestorsForQuery
import md.vibecodium.mobile.flattenExplorerTree
import md.vibecodium.mobile.indentAtCursor
import md.vibecodium.mobile.outdentAtCursor
import kotlinx.coroutines.flow.collect
import java.text.SimpleDateFormat
import java.time.LocalDateTime
import java.util.Date
import java.util.Locale

/** `--navbar-height: var(--touch-size-l)` in the Obsidian phone stylesheet. */
private val NavbarHeight = 52.dp

/** `--view-header-height: var(--touch-size-m)` on a phone. */
private val ViewHeaderHeight = 44.dp

/** Top margin for the floating header pills so they hover below the status bar. */
private val ViewHeaderTopPadding = 0.dp

/** Second floating action in the header gutter, right under the explorer toggle. */
private val NoteActionTop = 48.dp

/** Pitch between stacked floating actions: 44 dp button plus a 4 dp gap. */
private val NoteActionStep = 48.dp

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

/** `lucide-calendar`, the daily-note action. */
private val CalendarIcon: ImageVector = ImageVector.Builder(
    name = "calendar",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(5f, 4f)
        curveTo(3.9f, 4f, 3f, 4.9f, 3f, 6f)
        lineTo(3f, 20f)
        curveTo(3f, 21.1f, 3.9f, 22f, 5f, 22f)
        lineTo(19f, 22f)
        curveTo(20.1f, 22f, 21f, 21.1f, 21f, 20f)
        lineTo(21f, 6f)
        curveTo(21f, 4.9f, 20.1f, 4f, 19f, 4f)
        close()
        moveTo(16f, 2f)
        lineTo(16f, 6f)
        moveTo(8f, 2f)
        lineTo(8f, 6f)
        moveTo(3f, 10f)
        lineTo(21f, 10f)
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

/** Material `undo`, matching the desktop toolbar action. */
private val UndoIcon: ImageVector = ImageVector.Builder(
    name = "undo",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).addPath(
    pathData = addPathNodes(
        "M12.5 8c-2.65 0-5.05.99-6.9 2.6L2 7v9h9l-3.62-3.62c1.39-1.16 3.16-1.88 " +
            "5.12-1.88 3.54 0 6.55 2.31 7.6 5.5l2.37-.78C21.08 11.03 17.15 8 12.5 8z",
    ),
    fill = SolidColor(Color.Black),
).build()

/** Material `redo`, the counterpart of [UndoIcon]. */
private val RedoIcon: ImageVector = ImageVector.Builder(
    name = "redo",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).addPath(
    pathData = addPathNodes(
        "M18.4 10.6C16.55 8.99 14.15 8 11.5 8c-4.65 0-8.58 3.03-9.96 7.22L3.9 16c1.05-3.19 " +
            "4.05-5.5 7.6-5.5 1.95 0 3.73.72 5.12 1.88L13 16h9V7l-3.6 3.6z",
    ),
    fill = SolidColor(Color.Black),
).build()

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

/** `lucide-arrow-right`, the "move to folder" action. */
private val MoveIcon: ImageVector = ImageVector.Builder(
    name = "move-right",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(4f, 12f)
        lineTo(20f, 12f)
        moveTo(14f, 6f)
        lineTo(20f, 12f)
        lineTo(14f, 18f)
    }
}.build()

/** `lucide-copy`, used for "make a copy". */
private val CopyIcon: ImageVector = ImageVector.Builder(
    name = "copy",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(9f, 9f)
        lineTo(20f, 9f)
        lineTo(20f, 20f)
        lineTo(9f, 20f)
        close()
        moveTo(15f, 5f)
        lineTo(15f, 4f)
        lineTo(4f, 4f)
        lineTo(4f, 15f)
        lineTo(5f, 15f)
    }
}.build()

/** `lucide-clipboard`, used for "copy path". */
private val ClipboardIcon: ImageVector = ImageVector.Builder(
    name = "clipboard",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(9f, 4f)
        lineTo(15f, 4f)
        moveTo(9f, 2f)
        lineTo(15f, 2f)
        lineTo(15f, 6f)
        lineTo(9f, 6f)
        close()
        moveTo(7f, 4f)
        lineTo(5f, 4f)
        lineTo(5f, 21f)
        lineTo(19f, 21f)
        lineTo(19f, 4f)
        lineTo(17f, 4f)
    }
}.build()

/** `lucide-trash-2`, the destructive action. */
private val TrashIcon: ImageVector = ImageVector.Builder(
    name = "trash",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(4f, 7f)
        lineTo(20f, 7f)
        moveTo(10f, 7f)
        lineTo(10f, 4f)
        lineTo(14f, 4f)
        lineTo(14f, 7f)
        moveTo(6f, 7f)
        lineTo(7f, 20f)
        lineTo(17f, 20f)
        lineTo(18f, 7f)
        moveTo(10f, 11f)
        lineTo(10f, 16f)
        moveTo(14f, 11f)
        lineTo(14f, 16f)
    }
}.build()

/** A tab with a cross: "close all tabs". */
private val CloseTabsIcon: ImageVector = ImageVector.Builder(
    name = "close-tabs",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(5f, 4f)
        lineTo(19f, 4f)
        quadTo(21f, 4f, 21f, 6f)
        lineTo(21f, 18f)
        quadTo(21f, 20f, 19f, 20f)
        lineTo(5f, 20f)
        quadTo(3f, 20f, 3f, 18f)
        lineTo(3f, 6f)
        quadTo(3f, 4f, 5f, 4f)
        close()
        moveTo(9f, 9f)
        lineTo(15f, 15f)
        moveTo(15f, 9f)
        lineTo(9f, 15f)
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

/** Material `remove`: the bar of `add` without the upright stroke. */
private val MinusIcon: ImageVector = ImageVector.Builder(
    name = "remove",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = SolidColor(Color.Black)) {
        moveTo(5f, 11f)
        lineTo(19f, 11f)
        lineTo(19f, 13f)
        lineTo(5f, 13f)
        close()
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

/** `lucide-sun`, shown in dark mode to switch to light. */
private val SunIcon: ImageVector = ImageVector.Builder(
    name = "sun",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.75f) {
        moveTo(16f, 12f)
        arcTo(4f, 4f, 0f, true, true, 8f, 12f)
        arcTo(4f, 4f, 0f, true, true, 16f, 12f)
        moveTo(12f, 2f)
        lineTo(12f, 4f)
        moveTo(12f, 20f)
        lineTo(12f, 22f)
        moveTo(2f, 12f)
        lineTo(4f, 12f)
        moveTo(20f, 12f)
        lineTo(22f, 12f)
        moveTo(4.93f, 4.93f)
        lineTo(6.34f, 6.34f)
        moveTo(17.66f, 17.66f)
        lineTo(19.07f, 19.07f)
        moveTo(4.93f, 19.07f)
        lineTo(6.34f, 17.66f)
        moveTo(17.66f, 6.34f)
        lineTo(19.07f, 4.93f)
    }
}.build()

/** `lucide-moon`, shown in light mode to switch to dark. */
private val MoonIcon: ImageVector = ImageVector.Builder(
    name = "moon",
    defaultWidth = 24.dp,
    defaultHeight = 24.dp,
    viewportWidth = 24f,
    viewportHeight = 24f,
).apply {
    path(fill = SolidColor(Color.Black)) {
        moveTo(21f, 12.79f)
        arcTo(9f, 9f, 0f, true, true, 11.21f, 3f)
        arcTo(7f, 7f, 0f, false, false, 21f, 12.79f)
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
    val dark = when (state.settings.themeMode) {
        ThemeMode.Dark -> true
        ThemeMode.Light -> false
        ThemeMode.System -> isSystemInDarkTheme()
    }
    val palette = if (dark) state.themeDarkPalette else state.themeLightPalette
    LaunchedEffect(dark, palette) {
        controller.applyNativePalette(dark)
    }
    VibecodiumTheme(palette) {
        CompositionLocalProvider(
            LocalDensity provides Density(
                density.density,
                density.fontScale * state.settings.textScale,
            ),
        ) {
            BackHandler(
                enabled = state.vaultChooserOpen || state.settingsOpen || state.drawer != null ||
                    state.tabSwitcher || state.viewerPath != null || state.historyIndex > 0 ||
                    state.explorerMenuPath != null || state.movePath != null ||
                    state.explorerRenamePath != null,
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
                        // The system navigation bar is deliberately independent
                        // of the Obsidian theme.
                        .background(Color.Black),
                )
                state.message?.let { message ->
                    MessageBar(message) { controller.dismissMessage() }
                }
                state.deletePath?.let { path ->
                    DeleteDialog(path, state.folders.contains(path), controller)
                }
                state.renamePath?.let {
                    RenameDialog(state, controller)
                }
                if (state.movePath != null) {
                    MoveDialog(state, controller)
                }
                if (state.vaultChooserOpen) {
                    VaultChooserDialog(state, controller)
                }
                if (state.explorerMenuPath != null) {
                    ExplorerContextMenu(state, controller)
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
    val headings: HeadingsSettings,
    val find: FindUiState = FindUiState(),
    val settingsOpen: Boolean = false,
    val overlayOpen: Boolean = false,
    val focusRequestId: Long = 0L,
)

/**
 * New tabs are appended to [tabs], so the reversed indices show the most
 * recently opened or created note at the top. Switching between existing tabs
 * does not change their position.
 */
internal fun newestFirstTabOrder(tabs: List<Tab>): List<Int> = tabs.indices.reversed().toList()

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
                headings = current.settings.headings,
                find = current.find,
                settingsOpen = current.settingsOpen,
                overlayOpen = current.settingsOpen || current.renamePath != null ||
                    current.deletePath != null || current.vaultChooserOpen ||
                    current.tabSwitcher,
                focusRequestId = current.focusRequestId,
            )
        }
    }
    // MIUI briefly reports no navigation bar while the screen wakes up. If the
    // layout follows that transient, both native surfaces resize twice (the
    // visible "squeeze"). Own the bottom inset here, with the last real height,
    // instead of letting systemBarsPadding apply an animated value.
    val density = LocalDensity.current
    val palette = LocalVibecodiumPalette.current
    val orientation = LocalConfiguration.current.orientation
    val navBarBottomPx = WindowInsets.navigationBars.getBottom(density)
    var stableNavBarBottomPx by rememberSaveable(orientation) { mutableStateOf(navBarBottomPx) }
    LaunchedEffect(navBarBottomPx, orientation) {
        if (navBarBottomPx > 0) stableNavBarBottomPx = navBarBottomPx
    }
    val stableNavBottom = with(density) { stableNavBarBottomPx.toDp() }
    val imeVisible = WindowInsets.isImeVisible
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            // safeDrawing also covers display cutouts and caption bars that
            // tablets report instead of a plain status-bar inset.
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top))
            .windowInsetsPadding(
                WindowInsets.navigationBars.only(WindowInsetsSides.Horizontal),
            )
            .padding(bottom = stableNavBottom),
    ) {
        val drawerWidth = phoneDrawerWidth(maxWidth)
        val leftDrawerWidth = leftDrawerWidth(maxWidth)
        val closeButtonX = leftDrawerWidth.coerceAtMost((maxWidth - CloseButtonSlot).coerceAtLeast(0.dp))
        val navbarEnabled = state.settings.navbarEnabled
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
        val onEditorScroll: (Float, Float, Boolean) -> Unit = remember {
            { scroll, maxY, isUserGesture ->
                if (!controller.state.settings.autoHideChrome) {
                    if (chromeHidden) chromeHidden = false
                    lastAnchor[0] = 0f
                    lastScrollVal[0] = scroll
                } else if (!isUserGesture) {
                    if (maxY <= 0.5f || !chromeHidden) {
                        chromeHidden = false
                    }
                    lastAnchor[0] = scroll
                    lastScrollVal[0] = scroll
                } else {
                    val anchor = lastAnchor[0]
                    val prev = lastScrollVal[0]
                    lastScrollVal[0] = scroll
                    if (maxY <= 0.5f || scroll <= 2f) {
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
                state.settings.editorFontDp,
                state.settings.codeFontDp,
                state.settings.textFontFace,
                state.settings.codeFontFace,
                state.settings.headings,
                imeVisible,
                navbarEnabled,
                chromeProgress,
                // Drawers and the tab switcher are full-screen browsing
                // surfaces; the keyboard must not cover them.
                state.drawer != null || state.tabSwitcher,
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
                // Opaque: the editor behind the explorer must not show through.
                DrawerBackdrop(alpha = 1f) {
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
                renameActive = state.explorerRenamePath != null,
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .width(leftDrawerWidth)
                    .fillMaxHeight()
                    .offset(x = if (state.drawer == Drawer.Left) 0.dp else -leftDrawerWidth)
                    .then(
                        if (state.drawer == Drawer.Left) {
                            Modifier
                        } else {
                            Modifier.semantics { invisibleToUser() }
                        },
                    ),
            )
            // The close button sits outside the drawer's right edge, in the
            // strip the drawer width reserves for it. Its top edge lines up
            // with the top of the drawer and of the note header, so it reads
            // as part of the same toolbar row.
            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .offset(x = closeButtonX)
                    .size(CloseButtonSlot, CloseButtonSize)
                    .then(
                        if (state.drawer == Drawer.Left) {
                            Modifier
                        } else {
                            Modifier.semantics { invisibleToUser() }
                        },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                if (state.drawer == Drawer.Left) {
                    FloatingIconButton(
                        icon = Icons.Default.Close,
                        description = "Close explorer",
                        modifier = Modifier.size(CloseButtonSize),
                    ) {
                        focusManager.clearFocus()
                        controller.closeOverlays()
                    }
                }
            }

            if (state.drawer == Drawer.Left) {
                // Theme and settings live in the same right-side strip as the
                // close button, but at the bottom: theme above settings.
                Column(
                    Modifier
                        .align(Alignment.CenterStart)
                        .offset(x = closeButtonX)
                        .fillMaxHeight()
                        .width(CloseButtonSlot),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.Bottom),
                ) {
                    FloatingIconButton(
                        icon = if (palette.dark) SunIcon else MoonIcon,
                        description = if (palette.dark) {
                            "Switch to light theme"
                        } else {
                            "Switch to dark theme"
                        },
                        modifier = Modifier.size(CloseButtonSize),
                    ) { controller.toggleThemeMode() }
                    FloatingIconButton(
                        icon = Icons.Default.Settings,
                        description = "Settings",
                        modifier = Modifier.size(CloseButtonSize),
                    ) {
                        focusManager.clearFocus()
                        controller.openSettings()
                    }
                }
            }

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
    editorFontDp: Int,
    codeFontDp: Int,
    textFontFace: EditorFontFace,
    codeFontFace: EditorFontFace,
    headings: HeadingsSettings,
    imeVisible: Boolean,
    navbarEnabled: Boolean,
    chromeProgress: State<Float>,
    keyboardHidden: Boolean,
    onEditorScroll: (Float, Float, Boolean) -> Unit,
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
    } else if (!tab.isLoaded) {
        Box(
            modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background),
            contentAlignment = Alignment.Center,
        ) {
            CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
        }
    } else {
        EditorScreen(
            editor,
            tab,
            controller,
            editorFontDp,
            codeFontDp,
            textFontFace,
            codeFontFace,
            headings,
            imeVisible,
            navbarEnabled,
            bottomInsetPx,
            chromeProgress,
            keyboardHidden,
            onEditorScroll,
        )
    }
}

/** `--mobile-sidebar-width: 84vw`, clamped to 300–450 px in the phone sheet. */
private fun phoneDrawerWidth(maxWidth: Dp): Dp {
    if (maxWidth <= 300.dp) return maxWidth
    return (maxWidth * 0.84f).coerceIn(300.dp, minOf(420.dp, maxWidth))
}

/** Width of the strip right of the left drawer that holds its close button. */
private val CloseButtonSlot = 56.dp

/** Height of the drawer's top section bar, matching the note header actions. */
private val DrawerTopBarHeight = 44.dp

/** Same 44 dp circle as the open note's header actions. */
private val CloseButtonSize = DrawerTopBarHeight

/** Panel corners, curved like the note header pills: half the 44 dp header. */
private val PanelCornerRadius = ViewHeaderHeight / 2

/** Height of the dissolve scrim at the top and bottom of the file tree. */
private val TreeEdgeFade = 20.dp

/** Left drawer keeps a gutter on the right so the close button never covers it. */
private fun leftDrawerWidth(maxWidth: Dp): Dp {
    val gutter = CloseButtonSlot
    if (maxWidth <= gutter + 200.dp) return maxWidth
    return phoneDrawerWidth(maxWidth).coerceAtMost(maxWidth - gutter)
}

@Composable
private fun EditorScreen(
    state: EditorSlice,
    tab: Tab,
    controller: VibecodiumController,
    editorFontDp: Int,
    codeFontDp: Int,
    textFontFace: EditorFontFace,
    codeFontFace: EditorFontFace,
    headings: HeadingsSettings,
    imeVisible: Boolean,
    navbarEnabled: Boolean,
    bottomInsetPx: Float,
    chromeProgress: State<Float>,
    keyboardHidden: Boolean,
    onEditorScroll: (Float, Float, Boolean) -> Unit,
) {
    val bottomInset = if (imeVisible || !navbarEnabled) 0.dp else NavbarHeight + 12.dp
    val preview = state.editorMode == EditorMode.Preview
    val topInsetPx = with(LocalDensity.current) { (ViewHeaderTopPadding + ViewHeaderHeight + 10.dp).toPx() }
    val findBarSpace = if (state.find.open) {
        (if (state.find.replaceOpen) 92.dp else 54.dp)
    } else {
        0.dp
    }
    val editorTopInsetPx = topInsetPx +
        with(LocalDensity.current) { findBarSpace.toPx() }
    var editorView by remember { mutableStateOf<NativeEditorView?>(null) }
    var keyboardPrompt by remember { mutableStateOf(!preview) }
    val onViewReady: (NativeEditorView) -> Unit = remember { { editorView = it } }
    val onCaretPlaced: () -> Unit = remember { { keyboardPrompt = true } }
    LaunchedEffect(editorView, tab.path, state.activeTab, preview, state.focusRequestId) {
        val view = editorView ?: return@LaunchedEffect
        view.recalculateScrollMetrics()
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
    LaunchedEffect(keyboardHidden) {
        if (keyboardHidden) {
            editorView?.hideKeyboard()
        } else if (!preview) {
            editorView?.focusEditor()
        }
    }
    var prevImeVisible by remember { mutableStateOf(false) }
    LaunchedEffect(imeVisible) {
        if (imeVisible && !prevImeVisible && !preview) {
            // A floating keyboard does not change the window insets, so the
            // native inset handler cannot reveal the caret for it.
            editorView?.onKeyboardShown()
        }
        if (prevImeVisible && !imeVisible && !preview) {
            keyboardPrompt = true
            editorView?.onKeyboardDismissed()
        }
        prevImeVisible = imeVisible
        editorView?.recalculateScrollMetrics()
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
            editorFontDp,
            codeFontDp,
            textFontFace,
            codeFontFace,
            headings,
            editorTopInsetPx,
            bottomInsetPx,
            preview,
            onEditorScroll,
            onViewReady,
            onCaretPlaced,
        )
        if (state.find.open) {
            Box(
                Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth()
                    .graphicsLayer {
                        // Follow the floating header: when scrolling down hides
                        // the chrome, the find panel must not stay pinned below
                        // the empty header slot.
                        translationY = -(ViewHeaderHeight + ViewHeaderTopPadding)
                            .toPx() * (1f - chromeProgress.value)
                    }
                    .padding(
                        start = 8.dp,
                        end = 8.dp,
                        top = ViewHeaderTopPadding + ViewHeaderHeight + 4.dp,
                    ),
            ) {
                FindBar(state.find, sourceMode = !preview, controller)
            }
        }
        // Drawn before the header so the popup page cover, which the header
        // paints, also hides this button while a menu is open.
        if (!preview && keyboardPrompt && !imeVisible) {
            // The navbar slides down with the chrome; the keyboard button
            // follows the same progress instead of leaving an empty gap.
            val bottomPadding = if (navbarEnabled) {
                16.dp + NavbarHeight * chromeProgress.value
            } else {
                16.dp
            }
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
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }
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
            ViewHeader(state, tab, controller, onHideKeyboard = { editorView?.hideKeyboard() })
        }
    }
}

/** Places the full-width tab menu right below its anchor, flipping when needed. */
private class TabMenuPositionProvider(
    private val gap: Int,
    private val left: Int,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val below = windowSize.height - (anchorBounds.bottom + gap)
        val above = anchorBounds.top - gap
        return if (popupContentSize.height <= below || below >= above) {
            IntOffset(left, anchorBounds.bottom + gap)
        } else {
            IntOffset(
                left,
                (anchorBounds.top - gap - popupContentSize.height).coerceAtLeast(0),
            )
        }
    }
}

/** Positions the full-screen dim layer used by the tab menu. */
private object FullScreenPositionProvider : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset = IntOffset.Zero
}

/**
 * Obsidian-style tab dropdown: a boundeded full-width LazyColumn under the
 * title pill with its own scrollbar (the stock DropdownMenu cannot host one).
 */
@Composable
private fun TabsDropdown(
    state: EditorSlice,
    controller: VibecodiumController,
    anchorBottomPx: Int,
    onDismiss: () -> Unit,
) {
    val density = LocalDensity.current
    val screenHeight = LocalConfiguration.current.screenHeightDp.dp
    val screenWidth = LocalConfiguration.current.screenWidthDp.dp
    // The list starts where the file-name pill starts; the dimmed note stays
    // visible in the empty gutter on the left.
    val leftInset = 60.dp
    val rightInset = 12.dp
    val menuWidth = (screenWidth - leftInset - rightInset).coerceAtLeast(200.dp)
    // The Popup window starts below the status bar, while the anchor is in
    // window coordinates, so subtract the status-bar inset for the offset.
    val statusBarTopPx = WindowInsets.statusBars.getTop(density)
    val navBarBottomPx = WindowInsets.navigationBars.getBottom(density)
    val gapPx = with(density) { 4.dp.roundToPx() }
    val menuTopScreenPx = anchorBottomPx + gapPx
    val menuOffsetY = with(density) {
        (menuTopScreenPx - statusBarTopPx).coerceAtLeast(0).toDp()
    }
    // The keyboard is hidden before the menu opens; keep room for the system
    // navigation bar so the last row is never covered.
    val maxMenuHeight = with(density) {
        (screenHeight.toPx() - menuTopScreenPx - navBarBottomPx - 8.dp.toPx())
            .coerceAtLeast(140.dp.toPx())
            .toDp()
    }
    val footerHeight = 44.dp
    val footerGap = 6.dp
    val listMaxHeight = (maxMenuHeight - footerHeight - footerGap).coerceAtLeast(72.dp)
    val tabOrder = remember(state.tabs) { newestFirstTabOrder(state.tabs) }
    val tabEntries = remember(state.tabs, state.activeTab, state.bookmarks, tabOrder) {
        tabOrder.map { index ->
            val tab = state.tabs[index]
            TabListEntry(
                index = index,
                tab = tab,
                active = index == state.activeTab,
                bookmarked = tab.path in state.bookmarks,
            )
        }
    }
    val tabColors = TabListColors(
        activeBackground = MaterialTheme.colorScheme.primary.copy(alpha = 0.15f).toArgb(),
        text = MaterialTheme.colorScheme.onSurface.toArgb(),
        muted = MaterialTheme.colorScheme.onSurfaceVariant.toArgb(),
        accent = MaterialTheme.colorScheme.primary.toArgb(),
    )
    Popup(
        popupPositionProvider = FullScreenPositionProvider,
        onDismissRequest = onDismiss,
        properties = PopupProperties(
            focusable = true,
            dismissOnBackPress = true,
            dismissOnClickOutside = true,
        ),
    ) {
        Box(Modifier.fillMaxSize().clickable(onClick = onDismiss)) {
            // The header paints the page cover in the main window, in the same
            // frame the toolbar strip fills, so this window only carries the
            // list and stays transparent everywhere else.
            // New note sits in the header gutter, left of the menu and under
            // the explorer toggle, so it only shows while this menu is open.
            FloatingIconButton(
                icon = NewNoteIcon,
                description = "New note",
                tint = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(start = 12.dp, top = NoteActionTop)
                    .size(44.dp),
            ) {
                onDismiss()
                controller.newNote()
            }
            if (controller.state.settings.dailyNotes.enabled) {
                FloatingIconButton(
                    icon = CalendarIcon,
                    description = "Daily note",
                    tint = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(start = 12.dp, top = NoteActionTop + NoteActionStep)
                        .size(44.dp),
                ) {
                    onDismiss()
                    controller.openDailyNote()
                }
            }
            Column(
                Modifier
                    .offset(x = leftInset, y = menuOffsetY)
                    .width(menuWidth)
                    .heightIn(max = maxMenuHeight),
            ) {
                Surface(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = listMaxHeight),
                    shape = RoundedCornerShape(PanelCornerRadius),
                    color = MaterialTheme.colorScheme.surfaceContainerHigh,
                    tonalElevation = 3.dp,
                    shadowElevation = 6.dp,
                ) {
                    AndroidView(
                        factory = { context -> TabListView(context) },
                        modifier = Modifier.fillMaxWidth().heightIn(max = listMaxHeight),
                        update = { view ->
                            view.setData(
                                entries = tabEntries,
                                colors = tabColors,
                                onSelect = { index ->
                                    controller.selectTab(index, onReady = onDismiss)
                                },
                                onClose = { index -> controller.closeTab(index) },
                            )
                        },
                    )
                }
                // Under the whole dropdown, on the page cover, so the action
                // reads as separate from the list of tabs it closes.
                Spacer(Modifier.height(footerGap))
                Row(
                    Modifier
                        .fillMaxWidth()
                        .height(footerHeight),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.End,
                ) {
                    Text(
                        "Close all (${state.tabs.size})",
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable(
                                enabled = state.tabs.isNotEmpty(),
                                onClick = {
                                    onDismiss()
                                    controller.closeAllTabs()
                                },
                            )
                            .padding(horizontal = 8.dp, vertical = 8.dp),
                        fontSize = 12.sp,
                        color = if (state.tabs.isNotEmpty()) {
                            MaterialTheme.colorScheme.onSurface
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f)
                        },
                    )
                }
            }
    }
}
}

@Composable
private fun ViewHeader(
    state: EditorSlice,
    tab: Tab,
    controller: VibecodiumController,
    onHideKeyboard: () -> Unit,
) {
    val bookmarked = tab.path in state.bookmarks
    val preview = state.editorMode == EditorMode.Preview
    var menuOpen by remember { mutableStateOf(false) }
    var tabsMenuOpen by remember { mutableStateOf(false) }
    var titleBottomPx by remember { mutableStateOf(0) }
    val palette = LocalVibecodiumPalette.current
    val density = LocalDensity.current
    // The popups are separate windows, so the page is covered here in the main
    // window: the toolbar strip and the rest of the page fill in the same
    // frame, and only the menu card itself arrives on the next one.
    val coverHeightPx = with(density) {
        LocalConfiguration.current.screenHeightDp.dp.toPx()
    }
    val popupBackdrop = if (menuOpen || tabsMenuOpen) {
        val cover = palette.overlay.compositeOver(palette.background)
        Modifier.drawBehind {
            // Taller than the window; the excess is clipped.
            drawRect(color = cover, size = Size(size.width, coverHeightPx))
        }
    } else {
        Modifier
    }
    Row(
        Modifier
            .fillMaxWidth()
            .height(ViewHeaderHeight)
            .then(popupBackdrop)
            .padding(start = 60.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The floating explorer button is rendered by PhoneShell at the
        // same top-left position for both empty and open states.
        Box(
            Modifier
                .weight(1f)
                .fillMaxHeight()
                .onGloballyPositioned { titleBottomPx = it.boundsInWindow().bottom.roundToInt() },
        ) {
            Surface(
                onClick = {
                    onHideKeyboard()
                    tabsMenuOpen = true
                },
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
            if (tabsMenuOpen) {
                TabsDropdown(
                    state = state,
                    controller = controller,
                    anchorBottomPx = titleBottomPx,
                    onDismiss = { tabsMenuOpen = false },
                )
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
                    ) {
                        onHideKeyboard()
                        menuOpen = true
                    }
                    HeaderNoteMenu(
                        expanded = menuOpen,
                        onDismiss = { menuOpen = false },
                        tab = tab,
                        editorMode = state.editorMode,
                        bookmarked = bookmarked,
                        activeTab = state.activeTab,
                        controller = controller,
                        anchorBottomPx = titleBottomPx,
                    )                }
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

/**
 * Toolbar "three dots" menu: dims the note, stays under the toolbar, and is
 * only as wide as its longest row.
 */
@Composable
private fun HeaderNoteMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    tab: Tab,
    editorMode: EditorMode,
    bookmarked: Boolean,
    activeTab: Int,
    controller: VibecodiumController,
    anchorBottomPx: Int,
) {
    if (!expanded) return
    val density = LocalDensity.current
    val screenHeight = LocalConfiguration.current.screenHeightDp.dp
    val statusBarTopPx = WindowInsets.statusBars.getTop(density)
    val navBarBottomPx = WindowInsets.navigationBars.getBottom(density)
    val gapPx = with(density) { 4.dp.roundToPx() }
    val menuTopScreenPx = anchorBottomPx + gapPx
    val menuOffsetY = with(density) {
        (menuTopScreenPx - statusBarTopPx).coerceAtLeast(0).toDp()
    }
    val maxMenuHeight = with(density) {
        (screenHeight.toPx() - menuTopScreenPx - navBarBottomPx - 8.dp.toPx())
            .coerceAtLeast(160.dp.toPx())
            .toDp()
    }
    Popup(
        popupPositionProvider = FullScreenPositionProvider,
        onDismissRequest = onDismiss,
        properties = PopupProperties(
            focusable = true,
            dismissOnBackPress = true,
            dismissOnClickOutside = true,
        ),
    ) {
        Box(Modifier.fillMaxSize().clickable(onClick = onDismiss)) {
            // The page cover is painted by the header in the main window, so
            // this window holds only the menu card and is transparent around it.
            Surface(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(end = 12.dp, top = menuOffsetY)
                    .width(IntrinsicSize.Max)
                    .heightIn(max = maxMenuHeight),
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 6.dp,
            ) {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    NoteActionItems(
                        tab = tab,
                        editorMode = editorMode,
                        bookmarked = bookmarked,
                        activeTab = activeTab,
                        controller = controller,
                        onDismiss = onDismiss,
                    )
                }
            }
        }
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
    anchorBottomPx: Int,
) {
    if (!expanded) return
    val density = LocalDensity.current
    val screenHeight = LocalConfiguration.current.screenHeightDp.dp
    val screenWidth = LocalConfiguration.current.screenWidthDp.dp
    // Match the tab menu: same gutters, rounding and position below the bar.
    val sideInset = 12.dp
    val menuWidth = (screenWidth - sideInset * 2).coerceAtLeast(200.dp)
    val leftInsetPx = with(density) { sideInset.roundToPx() }
    val anchorBottom = with(density) { anchorBottomPx.toDp() }
    val maxMenuHeight = (screenHeight - anchorBottom - 48.dp - 8.dp).coerceAtLeast(160.dp)
    Popup(
        popupPositionProvider = remember(density, leftInsetPx) {
            TabMenuPositionProvider(
                gap = with(density) { 4.dp.roundToPx() },
                left = leftInsetPx,
            )
        },
        onDismissRequest = onDismiss,
        properties = PopupProperties(
            focusable = true,
            dismissOnBackPress = true,
            dismissOnClickOutside = true,
        ),
    ) {
        Surface(
            modifier = Modifier.width(menuWidth).heightIn(max = maxMenuHeight),
            shape = RoundedCornerShape(16.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 3.dp,
            shadowElevation = 6.dp,
        ) {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                NoteActionItems(
                    tab = tab,
                    editorMode = editorMode,
                    bookmarked = bookmarked,
                    activeTab = activeTab,
                    controller = controller,
                    onDismiss = onDismiss,
                )
            }
        }
    }
}

/** Menu rows shared by the toolbar menu and the bottom-nav menu. */
@Composable
private fun NoteActionItems(
    tab: Tab,
    editorMode: EditorMode,
    bookmarked: Boolean,
    activeTab: Int,
    controller: VibecodiumController,
    onDismiss: () -> Unit,
) {
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
        text = { Text("Find in note") },
        onClick = { onDismiss(); controller.openFindBar() },
    )
    if (editorMode == EditorMode.Source) {
        DropdownMenuItem(
            text = { Text("Find and replace") },
            onClick = { onDismiss(); controller.openFindBar(replace = true) },
        )
    }
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
        DropdownMenuItem(
            text = { Text("Table source / grid") },
            onClick = { onDismiss(); controller.toggleActiveTableSource() },
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

// --------------------------------------------------------------------- source

@Composable
private fun SourceEditor(
    state: EditorSlice,
    controller: VibecodiumController,
    tab: Tab,
    editorFontDp: Int,
    codeFontDp: Int,
    textFontFace: EditorFontFace,
    codeFontFace: EditorFontFace,
    headings: HeadingsSettings,
    topInsetPx: Float,
    bottomInsetPx: Float,
    preview: Boolean,
    onScroll: (Float, Float, Boolean) -> Unit,
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
            ).also { view ->
                controller.attachEditorView(view)
                view.setOnTabFrameShown { shownPath -> controller.onTabFrameShown(shownPath) }
                view.setOnHistoryChanged { canUndo, canRedo ->
                    controller.setEditorHistoryState(canUndo, canRedo)
                }
                onViewReady(view)
            }
        },
        modifier = Modifier.fillMaxSize(),
        onRelease = { controller.attachEditorView(null) },
        update = { view ->
            view.setTopInset(topInsetPx)
            view.setBottomInset(bottomInsetPx)
            view.setOnScrollChanged(onScroll)
            view.setOnCaretPlaced(onCaretPlaced)
            view.setEditorPreview(preview)
            view.setEditorFont(editorFontDp, AppSettingsJson.editorLineDp(editorFontDp))
            view.setEditorCodeFont(codeFontDp)
            view.setEditorFonts(textFontFace, codeFontFace)
            view.setEditorHeadings(headings)
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
                "highlight" -> SpanStyle(background = accent.copy(alpha = 0.20f))
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
                icon = UndoIcon,
                label = "Undo",
                enabled = state.editorCanUndo,
            ) { controller.undoEditor() }
            NavButton(
                icon = RedoIcon,
                label = "Redo",
                enabled = state.editorCanRedo,
            ) { controller.redoEditor() }
            NavButton(
                icon = Icons.Default.Search,
                label = "Find in note",
            ) { controller.openFindBar() }
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
                    anchorBottomPx = 0,
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
private fun BoxScope.DrawerBackdrop(alpha: Float = 0.5f, onDismiss: () -> Unit) {
    val palette = LocalVibecodiumPalette.current
    // Keep the theme's cover colour but flatten its own alpha over the theme
    // background. The left drawer then paints an opaque, theme-tinted sheet
    // that hides text/buttons behind it instead of becoming pure black.
    val cover = palette.overlay.compositeOver(palette.background)
    Box(
        Modifier
            .fillMaxSize()
            .background(cover.copy(alpha = alpha.coerceIn(0f, 1f)))
            .clickable(onClick = onDismiss),
    )
}

private fun Color.compositeOver(background: Color): Color {
    val a = alpha
    if (a >= 0.999f) return this
    return Color(
        red = red * a + background.red * (1f - a),
        green = green * a + background.green * (1f - a),
        blue = blue * a + background.blue * (1f - a),
        alpha = 1f,
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

/**
 * Keyboard height the drawer reserves while the inline rename field is open,
 * measured above the navigation bar the shell already keeps clear.
 *
 * The IME inset is animated over many frames. Padding the drawer with the
 * animated value resized (and re-rendered) the native tree surface on every
 * one of them, which read as the list squeezing and stretching while the
 * keyboard slid in or out. Only the settled height is published, once the
 * show animation stops moving; it is dropped as soon as the keyboard starts
 * to hide. Both changes then land underneath the keyboard, where they cannot
 * be seen, and the tree resizes once instead of once per animation frame.
 */
@Composable
private fun renameImeBottomPx(active: Boolean): Int {
    val density = LocalDensity.current
    val navigationBottomPx = WindowInsets.navigationBars.getBottom(density)
    // The IME insets object lives in the composition; sampling its animated
    // value from the loop below keeps the drawer from recomposing per frame.
    val ime = WindowInsets.ime
    var settled by remember { mutableStateOf(0) }
    LaunchedEffect(active, navigationBottomPx) {
        if (!active) {
            settled = 0
            return@LaunchedEffect
        }
        var previous = 0
        var stableFrames = 0
        while (true) {
            val bottom = ime.getBottom(density)
            stableFrames = if (bottom == previous) stableFrames + 1 else 0
            when {
                // Gone or already sliding away: release the space now, while
                // the keyboard still covers the bottom of the tree.
                bottom == 0 || bottom < previous -> if (settled != 0) settled = 0
                // Shrink only after the show animation settles.
                stableFrames >= 2 -> if (settled != bottom) settled = bottom
            }
            previous = bottom
            withFrameNanos { }
        }
    }
    return (settled - navigationBottomPx).coerceAtLeast(0)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LeftDrawer(
    state: UiState,
    controller: VibecodiumController,
    renameActive: Boolean,
    modifier: Modifier,
) {
    val focusManager = LocalFocusManager.current
    val density = LocalDensity.current
    val palette = LocalVibecodiumPalette.current
    // While the inline rename field is open its row is lifted above the
    // keyboard by shrinking the tree area. The shell already keeps the drawer
    // above the navigation bar, so only the space above it is reserved.
    val imeBottomPx = renameImeBottomPx(renameActive)
    val contentBottom = with(density) { imeBottomPx.toDp() }
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(topEnd = PanelCornerRadius, bottomEnd = PanelCornerRadius),
        color = palette.surfaceAlt,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.fillMaxSize().padding(bottom = contentBottom)) {
            // The bar is toolbar height, but the tiles stay inset from its
            // edges so their rounded highlight never touches the dividers.
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(DrawerTopBarHeight)
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
                        modifier = Modifier.fillMaxSize(),
                        color = palette.surfaceAlt,
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
                DrawerSyncButton(state, controller)
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
    val listState = rememberLazyListState()
    val tabOrder = remember(state.tabs) { newestFirstTabOrder(state.tabs) }
    Surface(
        modifier = modifier.imePadding(),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
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
                    Icon(NewNoteIcon, contentDescription = "New note", modifier = Modifier.size(20.dp))
                }
                IconButton(
                    onClick = { controller.closeAllTabs() },
                    enabled = state.tabs.isNotEmpty(),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            CloseTabsIcon,
                            contentDescription = "Close all tabs",
                            modifier = Modifier.size(20.dp),
                            tint = if (state.tabs.isNotEmpty()) {
                                MaterialTheme.colorScheme.onSurface
                            } else {
                                MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f)
                            },
                        )
                        if (state.tabs.isNotEmpty()) {
                            Spacer(Modifier.width(3.dp))
                            Text(
                                state.tabs.size.toString(),
                                fontSize = 11.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
                IconButton(onClick = { controller.toggleTabSwitcher() }) {
                    Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Box(Modifier.weight(1f).fillMaxWidth()) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(start = 8.dp, end = 16.dp, top = 4.dp, bottom = 4.dp),
                ) {
                    itemsIndexed(tabOrder, key = { _, index -> state.tabs[index].path }) { _, index ->
                        val tab = state.tabs[index]
                        TabListRow(
                            tab = tab,
                            active = index == state.activeTab,
                            bookmarked = tab.path in state.bookmarks,
                            shape = RoundedCornerShape(8.dp),
                            onSelect = { controller.selectTab(index) },
                            onClose = { controller.closeTab(index) },
                        )
                    }
                }
                TabScrollBar(
                    state = listState,
                    modifier = Modifier
                        .align(Alignment.CenterEnd)
                        .fillMaxHeight()
                        .padding(end = 3.dp, top = 6.dp, bottom = 6.dp),
                )
            }
        }
    }
}

@Composable
private fun TabListRow(
    tab: Tab,
    active: Boolean,
    bookmarked: Boolean,
    shape: RoundedCornerShape,
    onSelect: () -> Unit,
    onClose: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .background(
                if (active) MaterialTheme.colorScheme.primary.copy(alpha = 0.15f)
                else Color.Transparent,
                shape,
            )
            .clip(shape)
            .clickable(onClick = onSelect)
            .padding(start = 8.dp, top = 4.dp, end = 4.dp, bottom = 4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) {
                if (active) {
                    Icon(
                        Icons.Default.Check,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
            }
            Spacer(Modifier.width(8.dp))
            Text(
                tab.title + if (tab.dirty) " •" else "",
                modifier = Modifier.weight(1f),
                fontSize = 14.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (bookmarked) {
                Icon(
                    Icons.Default.Star,
                    contentDescription = "Bookmarked",
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
            IconButton(onClick = onClose, modifier = Modifier.size(40.dp)) {
                Icon(
                    Icons.Default.Close,
                    contentDescription = "Close tab",
                    modifier = Modifier.size(16.dp),
                )
            }
        }
        Text(
            tab.path,
            modifier = Modifier.padding(start = 24.dp),
            fontSize = 11.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Minimal vertical scrollbar for a Compose list; drawn only when it overflows. */
@Composable
private fun TabScrollBar(state: LazyListState, modifier: Modifier = Modifier) {
    val layout = state.layoutInfo
    val visible = layout.visibleItemsInfo
    val total = layout.totalItemsCount
    if (visible.isEmpty() || total <= visible.size) return
    BoxWithConstraints(modifier) {
        val track = maxHeight
        val fraction = visible.size.toFloat() / total
        val thumb = (track * fraction).coerceAtLeast(24.dp).coerceAtMost(track)
        val maxOffset = (track - thumb).coerceAtLeast(0.dp)
        val scrollable = (total - visible.size).coerceAtLeast(1)
        val offset = maxOffset * (visible.first().index.toFloat() / scrollable)
        Box(
            Modifier
                .offset(y = offset)
                .width(3.dp)
                .height(thumb)
                .clip(RoundedCornerShape(2.dp))
                .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.55f)),
        )
    }
}

@Composable
private fun TreeList(state: UiState, controller: VibecodiumController) {
    var filterFocused by remember { mutableStateOf(false) }
    val focusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    val palette = LocalVibecodiumPalette.current
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
                        if (filterFocused) palette.backgroundAlt else palette.surface,
                    )
                    .border(
                        width = if (filterFocused) 1.5.dp else 1.dp,
                        color = if (filterFocused) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            palette.outline
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
                                    color = palette.textMuted,
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
        val filterActive = state.fileQuery.isNotBlank()
        // Folders on the way to a matching file open automatically; the user
        // can still collapse them for this filter pass.
        val matchAncestors = remember(state.explorerTree, state.fileQuery) {
            filteredAncestorsForQuery(state.explorerTree, state.fileQuery)
        }
        var filterOverrides by remember(state.fileQuery) {
            mutableStateOf<Map<String, Boolean>>(emptyMap())
        }
        val collapsed = remember(state.collapsedFolders, matchAncestors, filterOverrides, filterActive) {
            if (!filterActive) {
                state.collapsedFolders
            } else {
                val result = (state.collapsedFolders - matchAncestors).toMutableSet()
                for ((path, expanded) in filterOverrides) {
                    if (expanded) result.remove(path) else result.add(path)
                }
                result
            }
        }
        val rows = remember(filtered, collapsed) {
            flattenExplorerTree(filtered, collapsed)
        }
        val toggle: (String) -> Unit = { path ->
            if (!filterActive) {
                controller.toggleFolder(path)
            } else {
                // false = collapsed, true = expanded for this filter pass.
                val currentlyCollapsed = path in collapsed
                filterOverrides = filterOverrides + (path to currentlyCollapsed)
            }
        }
        val toggleState = rememberUpdatedState(toggle)
        if (rows.isEmpty()) {
            Placeholder("No matching files.")
            return@Column
        }
        val activePath = state.tabs.getOrNull(state.activeTab)?.path
        val treeJson = remember(rows, activePath, collapsed, state.fileQuery) {
            val query = state.fileQuery.trim()
            val array = JSONArray()
            for (row in rows) {
                val displayName = explorerDisplayName(row.node)
                val matchIndex = if (query.isEmpty()) {
                    -1
                } else {
                    displayName.indexOf(query, ignoreCase = true)
                }
                array.put(
                    JSONObject()
                        .put("name", displayName)
                        .put("path", row.node.path)
                        .put("folder", row.node.isFolder)
                        .put("depth", row.depth)
                        .put("expanded", row.node.path !in collapsed)
                        .put("has_children", row.node.children.isNotEmpty())
                        .put("note", row.node.entry?.isNote == true)
                        .put("extension", row.node.entry?.extension.orEmpty())
                        .put("match_start", if (matchIndex >= 0) displayName.codePointCount(0, matchIndex) else 0)
                        .put("match_len", if (matchIndex >= 0) query.codePointCount(0, query.length) else 0),
                )
            }
            JSONObject()
                .put("rows", array)
                .put("active", activePath ?: JSONObject.NULL)
                .toString()
        }
        var treeView by remember { mutableStateOf<NativeTreeView?>(null) }
        var treeHeight by remember { mutableStateOf(0) }
        // Long-press highlights the row immediately; the menu opens on release.
        val pressedPath = remember { mutableStateOf<String?>(null) }
        val renamePath = state.explorerRenamePath
        // A path can disappear while its editor is open (external delete,
        // refresh). Never freeze the tree for an editor that cannot render.
        val renameRow = renamePath?.takeIf { path -> rows.any { it.node.path == path } }
        // Touching the tree while renaming scrolls it; clearing focus here
        // would dismiss the keyboard and break that flow.
        val renamingNow by rememberUpdatedState(renameRow != null)
        LaunchedEffect(renamePath, rows) {
            if (renamePath != null && renameRow == null) controller.dismissExplorerRename()
        }
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clipToBounds()
                .onSizeChanged { treeHeight = it.height },
        ) {
            AndroidView(
                factory = { context ->
                    NativeTreeView(
                        context,
                        onAction = { action ->
                            handleTreeAction(action, controller) { path -> toggleState.value(path) }
                        },
                        onTouchStart = { if (!renamingNow) focusManager.clearFocus() },
                        onPress = { path -> pressedPath.value = path },
                    ).also { treeView = it }
                },
                modifier = Modifier.fillMaxSize(),
                update = {
                    it.setRows(treeJson)
                    it.setDrawerVisible(state.drawer == Drawer.Left)
                    it.setInteractive(state.drawerMode == DrawerMode.Files)
                    it.setRenaming(renameRow != null)
                },
            )
            // Rows dissolve into the drawer surface at the bottom edge, where
            // the tree meets the vault footer without a divider.
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .height(TreeEdgeFade)
                    .background(
                        Brush.verticalGradient(
                            0f to palette.surfaceAlt.copy(alpha = 0f),
                            1f to palette.surfaceAlt,
                        ),
                    ),
            )
            val pressed = pressedPath.value
            val pressedIndex = pressed?.let { path -> rows.indexOfFirst { it.node.path == path } } ?: -1
            if (pressedIndex >= 0) {
                val density = LocalDensity.current
                val rowHeightPx = with(density) { 36.dp.roundToPx() }.toFloat()
                val scroll = treeView?.let { CoreBridge.nativeTreeScroll(it.surfaceId) } ?: 0f
                Box(
                    Modifier
                        .offset(y = with(density) { (pressedIndex * rowHeightPx - scroll).toDp() })
                        .fillMaxWidth()
                        .height(36.dp)
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.24f)),
                )
            }
            if (renameRow != null) {
                TreeInlineRename(
                    rows = rows,
                    path = renameRow,
                    initialValue = state.explorerRenameValue,
                    isFolder = state.explorerRenameFolder,
                    view = treeView,
                    viewportHeight = treeHeight,
                    onValueChange = controller::updateExplorerRenameValue,
                    onCommit = { controller.confirmExplorerRename(it) },
                )
            }
        }
    }
}

/**
 * Inline name editor for one explorer row. Kotlin knows the row order, the
 * native tree owns the scroll, so the field is positioned from both. The tree
 * keeps scrolling while the keyboard is open; a short tap commits the name.
 */
@Composable
private fun BoxScope.TreeInlineRename(
    rows: List<ExplorerRow>,
    path: String,
    initialValue: String,
    isFolder: Boolean,
    view: NativeTreeView?,
    viewportHeight: Int,
    onValueChange: (String) -> Unit,
    onCommit: (String) -> Unit,
) {
    val rowIndex = rows.indexOfFirst { it.node.path == path }
    if (rowIndex < 0) return
    val depth = rows[rowIndex].depth
    val density = LocalDensity.current
    val palette = LocalVibecodiumPalette.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focusRequester = remember { FocusRequester() }
    val initialSelection = remember(path) {
        val end = if (isFolder) {
            initialValue.length
        } else {
            initialValue.lastIndexOf('.').takeIf { it > 0 } ?: initialValue.length
        }
        TextRange(0, end)
    }
    var scroll by remember(path) { mutableStateOf(0f) }
    var value by remember(path) {
        mutableStateOf(TextFieldValue(initialValue, initialSelection))
    }
    LaunchedEffect(path, view, viewportHeight) {
        val target = view ?: return@LaunchedEffect
        // Runs again when the keyboard resizes the tree, so the row never
        // ends up hidden behind the IME.
        target.revealRow(rowIndex)
        scroll = CoreBridge.nativeTreeScroll(target.surfaceId)
    }
    // The tree scrolls natively, so follow its scroll while the editor sits
    // on a row: finger drags with the keyboard open keep the field aligned.
    LaunchedEffect(path, view) {
        val target = view ?: return@LaunchedEffect
        var last = Float.NaN
        while (true) {
            val current = CoreBridge.nativeTreeScroll(target.surfaceId)
            if (current != last) {
                last = current
                scroll = current
            }
            withFrameNanos { }
        }
    }
    LaunchedEffect(path) {
        focusRequester.requestFocus()
        // Some IMEs collapse the preselection when the input connection is
        // created, so apply it again once focus has settled.
        withFrameNanos { }
        value = value.copy(selection = initialSelection)
        keyboard?.show()
    }
    DisposableEffect(Unit) {
        onDispose { keyboard?.hide() }
    }
    fun commit() {
        keyboard?.hide()
        onCommit(value.text)
    }
    val shape = RoundedCornerShape(4.dp)
    val rowHeight = 36.dp
    // Match the native row height exactly: Rust rounds 36dp to whole pixels.
    val rowHeightPx = with(density) { rowHeight.roundToPx() }.toFloat()
    val top = with(density) { (rowIndex * rowHeightPx - scroll).toDp() }
    Row(
        Modifier
            .offset(y = top)
            .padding(start = 10.dp + 16.dp * depth + 35.dp, end = 8.dp)
            .fillMaxWidth()
            .height(rowHeight)
            .clip(shape)
            .background(palette.backgroundAlt)
            .border(1.5.dp, MaterialTheme.colorScheme.primary, shape)
            .padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicTextField(
            value = value,
            onValueChange = {
                value = it
                onValueChange(it.text)
            },
            modifier = Modifier
                .weight(1f)
                .focusRequester(focusRequester),
            singleLine = true,
            textStyle = TextStyle(
                fontSize = with(density) { 17.dp.toSp() },
                color = palette.text,
            ),
            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { commit() }),
        )
    }
}

private fun handleTreeAction(
    action: String,
    controller: VibecodiumController,
    onToggle: (String) -> Unit = { controller.toggleFolder(it) },
) {
    val value = runCatching { JSONObject(action) }.getOrNull() ?: return
    val kind = value.optString("kind")
    if (kind == "commit_rename") {
        controller.confirmExplorerRename()
        return
    }
    val path = value.optString("path")
    if (path.isEmpty()) return
    when (kind) {
        "toggle" -> onToggle(path)
        "open_note" -> {
            // The drawer closes from openNote() once the note is loaded.
            controller.openNote(path)
        }
        "open_file" -> controller.openAttachment(path)
        "rename" -> controller.beginExplorerRename(path, controller.state.folders.contains(path))
        "menu" -> controller.openExplorerMenu(path)
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
        if (state.settings.dailyNotes.enabled) {
            CompactToolbarButton(
                CalendarIcon,
                "Daily note",
                modifier = Modifier.weight(1f).fillMaxHeight(),
            ) { controller.openDailyNote() }
        }
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
    }
}

/**
 * Drawer footer sync action: syncs while connected, spins while running and
 * falls back to a plain refresh. Sized like the settings button beside it.
 */
@Composable
private fun DrawerSyncButton(state: UiState, controller: VibecodiumController) {
    val label = when {
        state.sync.busy -> "Stop sync"
        state.sync.connected && !state.vaultSync.paused -> "Sync"
        else -> "Refresh files"
    }
    val angle = if (state.sync.busy) {
        val transition = rememberInfiniteTransition(label = "sync-rotation")
        val spinning by transition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(900, easing = LinearEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "sync-angle",
        )
        spinning
    } else {
        0f
    }
    IconButton(
        onClick = { controller.refreshOrSync() },
        modifier = Modifier.size(44.dp),
    ) {
        Icon(
            SyncArrowsIcon,
            contentDescription = label,
            modifier = Modifier
                .size(24.dp)
                .graphicsLayer { rotationZ = angle },
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
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
    val activePath = state.tabs.getOrNull(state.activeTab)?.path
    val entries = remember(notes, activePath) {
        notes.map { note ->
            NoteListEntry(
                path = note.path,
                title = note.basename,
                detail = note.path,
                active = note.path == activePath,
                bookmarked = true,
            )
        }
    }
    val colors = NoteListColors(
        activeBackground = MaterialTheme.colorScheme.primary.copy(alpha = 0.15f).toArgb(),
        text = MaterialTheme.colorScheme.onSurface.toArgb(),
        muted = MaterialTheme.colorScheme.onSurfaceVariant.toArgb(),
        accent = MaterialTheme.colorScheme.primary.toArgb(),
        divider = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f).toArgb(),
    )
    AndroidView(
        factory = { context -> NoteListView(context) },
        modifier = Modifier.fillMaxSize(),
        update = { view ->
            view.setData(entries, colors, onOpen = { path -> controller.openNote(path) })
        },
    )
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
    val entries = remember(recent, state.bookmarks) {
        recent.map { (note, openedAt) ->
            NoteListEntry(
                path = note.path,
                title = note.basename,
                detail = if (openedAt > 0) dateTimeLabel(openedAt) else "—",
                active = false,
                bookmarked = note.path in state.bookmarks,
                closable = true,
            )
        }
    }
    val colors = NoteListColors(
        activeBackground = MaterialTheme.colorScheme.primary.copy(alpha = 0.15f).toArgb(),
        text = MaterialTheme.colorScheme.onSurface.toArgb(),
        muted = MaterialTheme.colorScheme.onSurfaceVariant.toArgb(),
        accent = MaterialTheme.colorScheme.primary.toArgb(),
        divider = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f).toArgb(),
    )
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
        AndroidView(
            factory = { context -> NoteListView(context) },
            modifier = Modifier.fillMaxWidth().weight(1f),
            update = { view ->
                view.setData(
                    entries,
                    colors,
                    onOpen = { path -> controller.openNote(path) },
                    onClose = { path -> controller.removeRecent(path) },
                )
            },
        )
    }
}

@Composable
private fun ThinScrollbar(state: LazyListState, modifier: Modifier = Modifier) {
    val layout = state.layoutInfo
    val total = layout.totalItemsCount
    val visible = layout.visibleItemsInfo
    if (total <= 0 || visible.isEmpty() || total <= visible.size) return
    // Rows are not uniform (file headers are taller than match lines), so a
    // median height makes the thumb jump whenever a header enters or leaves.
    // Average over the visible rows changes gradually; animating the result
    // keeps the track from stepping while the list is scrolling.
    val averageSize = visible.map { it.size }.average().toFloat().coerceAtLeast(1f)
    val firstSize = visible.first().size.coerceAtLeast(1).toFloat()
    val viewport = (layout.viewportEndOffset - layout.viewportStartOffset).toFloat()
    val targetProgress = (
        (state.firstVisibleItemIndex + (state.firstVisibleItemScrollOffset / firstSize).coerceIn(0f, 1f)) /
            (total - 1).coerceAtLeast(1).toFloat()
        ).coerceIn(0f, 1f)
    val targetFraction = (viewport / (averageSize * total)).coerceIn(0.08f, 1f)
    val progress by animateFloatAsState(
        targetValue = targetProgress,
        animationSpec = tween(90),
        label = "scrollbar-progress",
    )
    val fraction by animateFloatAsState(
        targetValue = targetFraction,
        animationSpec = tween(120),
        label = "scrollbar-fraction",
    )
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
                    RoundedCornerShape(1.5.dp),
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
    val palette = LocalVibecodiumPalette.current
    val results = state.searchResults
    val collapsed = state.searchCollapsedPaths
    val collapsible = results.filter { it.matches.isNotEmpty() }
    val allCollapsed = collapsible.isNotEmpty() && collapsible.all { it.path in collapsed }
    val totalMatches = results.sumOf { it.matchCount }
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
                        if (queryFocused) palette.backgroundAlt else palette.surface,
                    )
                    .border(
                        width = if (queryFocused) 1.5.dp else 1.dp,
                        color = if (queryFocused) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            palette.outline
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
                                    color = palette.textMuted,
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
        if (results.isNotEmpty()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(start = 12.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "${results.size} files · $totalMatches matches",
                    modifier = Modifier.weight(1f),
                    fontSize = 11.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                CompactToolbarButton(
                    CollapseAllIcon,
                    if (allCollapsed) "Expand all" else "Collapse all",
                    enabled = collapsible.isNotEmpty(),
                    modifier = Modifier.size(32.dp),
                ) { controller.setAllSearchCollapsed(!allCollapsed) }
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
        val resultColors = SearchResultColors(
            headerBackground = palette.surfaceAlt.copy(alpha = 0.55f).toArgb(),
            text = MaterialTheme.colorScheme.onSurface.toArgb(),
            muted = MaterialTheme.colorScheme.onSurfaceVariant.toArgb(),
            error = MaterialTheme.colorScheme.error.toArgb(),
            accent = MaterialTheme.colorScheme.primary.toArgb(),
            outline = MaterialTheme.colorScheme.outline.toArgb(),
            surface = palette.surface.toArgb(),
            divider = MaterialTheme.colorScheme.outline.copy(alpha = 0.8f).toArgb(),
            highlight = MaterialTheme.colorScheme.primary.copy(alpha = 0.20f).toArgb(),
            matchedText = MaterialTheme.colorScheme.onSurface.toArgb(),
        )
        when {
            state.searchQuery.isBlank() -> Text(
                "Type to search file names and note content.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            )

            results.isEmpty() -> Text(
                "No matches.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            )

            else -> AndroidView(
                factory = { context -> SearchResultsView(context) },
                modifier = Modifier.fillMaxWidth().weight(1f),
                update = { view ->
                    view.setData(
                        results = results,
                        collapsed = collapsed,
                        colors = resultColors,
                        onToggle = controller::toggleSearchCollapsed,
                        onOpenFile = { file ->
                            controller.openSearchResult(file, file.matches.firstOrNull())
                        },
                        onOpenMatch = { file, match ->
                            controller.openSearchResult(file, match)
                        },
                    )
                },
            )
        }
    }
}

@Composable
private fun FindBar(
    find: FindUiState,
    sourceMode: Boolean,
    controller: VibecodiumController,
) {
    val focusRequester = remember { FocusRequester() }
    val palette = LocalVibecodiumPalette.current
    val textColor = MaterialTheme.colorScheme.onSurface
    val muted = palette.textMuted
    val count = when {
        find.query.isEmpty() -> ""
        find.matches.isEmpty() -> "0/0"
        else -> {
            val current = (find.active.coerceAtLeast(0) + 1).coerceAtMost(find.matches.size)
            "${current}/${find.matches.size}${if (find.truncated) "+" else ""}"
        }
    }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(24.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        tonalElevation = 3.dp,
        shadowElevation = 5.dp,
    ) {
        Column(Modifier.padding(horizontal = 8.dp, vertical = 5.dp)) {
            Row(
                Modifier.fillMaxWidth().height(36.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(
                    Modifier
                        .weight(1f)
                        .height(34.dp)
                        .clip(RoundedCornerShape(percent = 50))
                        .background(palette.surface)
                        .border(
                            1.dp,
                            palette.outline,
                            RoundedCornerShape(percent = 50),
                        )
                        .padding(horizontal = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        Icons.Default.Search,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = muted,
                    )
                    BasicTextField(
                        value = find.query,
                        onValueChange = controller::updateFindQuery,
                        modifier = Modifier
                            .weight(1f)
                            .padding(horizontal = 6.dp)
                            .focusRequester(focusRequester),
                        singleLine = true,
                        textStyle = TextStyle(fontSize = 14.sp, color = textColor),
                        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                        decorationBox = { inner ->
                            Box {
                                if (find.query.isEmpty()) {
                                    Text("Find in note...", fontSize = 14.sp, color = muted)
                                }
                                inner()
                            }
                        },
                    )
                    if (count.isNotEmpty()) {
                        Text(
                            count,
                            fontSize = 10.sp,
                            color = muted,
                            modifier = Modifier.padding(start = 4.dp),
                        )
                    }
                }
                FindToggle("Aa", find.caseSensitive) { controller.toggleFindCase() }
                FindToggle("ab", find.wholeWord, underline = true) {
                    controller.toggleFindWholeWord()
                }
                IconButton(
                    onClick = controller::findPrevious,
                    modifier = Modifier.size(30.dp),
                ) {
                    Icon(
                        Icons.Default.KeyboardArrowUp,
                        contentDescription = "Previous match",
                        modifier = Modifier.size(18.dp),
                        tint = muted,
                    )
                }
                IconButton(
                    onClick = controller::findNext,
                    modifier = Modifier.size(30.dp),
                ) {
                    Icon(
                        Icons.Default.KeyboardArrowDown,
                        contentDescription = "Next match",
                        modifier = Modifier.size(18.dp),
                        tint = muted,
                    )
                }
                if (sourceMode) {
                    IconButton(
                        onClick = controller::toggleFindReplace,
                        modifier = Modifier.size(30.dp),
                    ) {
                        Icon(
                            Icons.Default.Edit,
                            contentDescription = "Replace",
                            modifier = Modifier.size(16.dp),
                            tint = if (find.replaceOpen) MaterialTheme.colorScheme.primary else muted,
                        )
                    }
                }
                IconButton(
                    onClick = controller::closeFindBar,
                    modifier = Modifier.size(30.dp),
                ) {
                    Icon(
                        Icons.Default.Close,
                        contentDescription = "Close find",
                        modifier = Modifier.size(18.dp),
                        tint = muted,
                    )
                }
            }
            if (sourceMode && find.replaceOpen) {
                Row(
                    Modifier.fillMaxWidth().height(36.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Row(
                        Modifier
                            .weight(1f)
                            .height(34.dp)
                            .clip(RoundedCornerShape(percent = 50))
                            .background(palette.surface)
                            .border(
                                1.dp,
                                palette.outline,
                                RoundedCornerShape(percent = 50),
                            )
                            .padding(horizontal = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            Icons.Default.Edit,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = muted,
                        )
                        BasicTextField(
                            value = find.replace,
                            onValueChange = controller::updateFindReplace,
                            modifier = Modifier
                                .weight(1f)
                                .padding(horizontal = 6.dp),
                            singleLine = true,
                            textStyle = TextStyle(fontSize = 13.sp, color = textColor),
                            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                            decorationBox = { inner ->
                                Box {
                                    if (find.replace.isEmpty()) {
                                        Text("Replace with...", fontSize = 13.sp, color = muted)
                                    }
                                    inner()
                                }
                            },
                        )
                    }
                    TextButton(onClick = controller::replaceCurrentFind) {
                        Text("Replace", fontSize = 11.sp)
                    }
                    TextButton(onClick = controller::replaceAllFind) {
                        Text("All", fontSize = 11.sp)
                    }
                }
            }
        }
    }
}

@Composable
private fun FindToggle(
    label: String,
    active: Boolean,
    underline: Boolean = false,
    onClick: () -> Unit,
) {
    val color = if (active) MaterialTheme.colorScheme.primary else LocalVibecodiumPalette.current.textMuted
    Box(
        Modifier
            .size(26.dp)
            .clip(RoundedCornerShape(5.dp))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = 11.sp,
            color = color,
            textDecoration = if (underline) TextDecoration.Underline else null,
        )
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
            .align(Alignment.TopCenter)
            .statusBarsPadding()
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

/**
 * Obsidian's file explorer action menu: folder rows get "New note" and
 * "New folder", every row gets rename/move/copy/delete actions.
 */
@Composable
private fun ExplorerContextMenu(state: UiState, controller: VibecodiumController) {
    val path = state.explorerMenuPath ?: return
    val isFolder = state.folders.contains(path)
    val name = path.substringAfterLast('/')
    val files = if (isFolder) state.entries.count { it.path.startsWith("$path/") } else 0
    val folders = if (isFolder) state.folders.count { it.startsWith("$path/") } else 0
    val subtitle = if (isFolder) {
        "$files ${if (files == 1) "file" else "files"} · " +
            "$folders ${if (folders == 1) "folder" else "folders"}"
    } else {
        val extension = state.entries.firstOrNull { it.path == path }?.extension.orEmpty()
        when {
            extension.isBlank() -> "File"
            extension.equals("md", ignoreCase = true) -> "Note"
            else -> extension.uppercase(Locale.US) + " file"
        }
    }
    BackHandler { controller.dismissExplorerMenu() }
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.45f))
            .pointerInput(path) {
                detectTapGestures(onTap = { controller.dismissExplorerMenu() })
            },
    ) {
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .navigationBarsPadding()
                .clip(RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp))
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) {}
                .padding(vertical = 8.dp),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 18.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    if (isFolder) NewFolderIcon else Icons.Default.List,
                    contentDescription = null,
                    modifier = Modifier.size(22.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        name,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        subtitle,
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            if (isFolder) {
                ActionMenuRow(NewNoteIcon, "New note") { controller.menuNewNote(path) }
                ActionMenuRow(NewFolderIcon, "New folder") { controller.menuNewFolder(path) }
                HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.35f))
            }
            ActionMenuRow(Icons.Default.Edit, "Rename") { controller.menuRename(path) }
            ActionMenuRow(MoveIcon, "Move to…") { controller.menuMove(path) }
            ActionMenuRow(CopyIcon, "Make a copy") { controller.menuDuplicate(path) }
            ActionMenuRow(ClipboardIcon, "Copy path") { controller.menuCopyPath(path) }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.35f))
            ActionMenuRow(TrashIcon, "Delete", danger = true) { controller.menuDelete(path) }
        }
    }
}

@Composable
private fun ActionMenuRow(
    icon: ImageVector,
    label: String,
    danger: Boolean = false,
    onClick: () -> Unit,
) {
    val color = if (danger) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .heightIn(min = 52.dp)
            .padding(horizontal = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = color,
        )
        Spacer(Modifier.width(14.dp))
        Text(label, fontSize = 14.sp, color = color)
    }
}

@Composable
private fun MoveDialog(state: UiState, controller: VibecodiumController) {
    val path = state.movePath ?: return
    val isFolder = state.folders.contains(path)
    val current = path.substringBeforeLast('/', "")
    val destinations = buildList {
        add("" to "Vault root")
        for (folder in state.folders) {
            if (isFolder && (folder == path || folder.startsWith("$path/"))) continue
            add(folder to folder)
        }
    }
    AlertDialog(
        onDismissRequest = { controller.dismissMove() },
        title = { Text("Move “${path.substringAfterLast('/')}”") },
        text = {
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp)) {
                items(destinations, key = { it.first }) { (folder, label) ->
                    val here = folder == current
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable(enabled = !here) { controller.moveTo(folder) }
                            .padding(horizontal = 8.dp, vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            label,
                            modifier = Modifier.weight(1f),
                            fontSize = 14.sp,
                            color = if (here) {
                                MaterialTheme.colorScheme.onSurfaceVariant
                            } else {
                                MaterialTheme.colorScheme.onSurface
                            },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (here) {
                            Text(
                                "Current",
                                fontSize = 11.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { controller.dismissMove() }) {
                Text("Cancel")
            }
        },
    )
}

@Composable
private fun DeleteDialog(path: String, isFolder: Boolean, controller: VibecodiumController) {
    AlertDialog(
        onDismissRequest = { controller.dismissDelete() },
        title = { Text(if (isFolder) "Delete folder?" else "Delete file?") },
        text = {
            Text(if (isFolder) "$path\n\nAll files inside it will be deleted." else path)
        },
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

private enum class SettingsSection(val title: String, val icon: ImageVector) {
    Appearance("Appearance", Icons.Default.Settings),
    Editor("Editor", Icons.Default.Edit),
    Files("Files & links", Icons.Default.List),
    DailyNotes("Daily notes", Icons.Default.DateRange),
    Sync("Sync", Icons.Default.Refresh),
    Advanced("Advanced", Icons.Default.Build),
    About("About", Icons.Default.Info),
}

/** Size range per heading level, matching the desktop settings. */
private val HEADING_LEVELS = listOf(
    1 to (14..40),
    2 to (13..36),
    3 to (12..32),
    4 to (11..28),
    5 to (10..26),
    6 to (9..24),
)

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
    Surface(modifier = modifier, color = MaterialTheme.colorScheme.background) {
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
private fun ThemeAppearanceGroup(state: UiState, controller: VibecodiumController) {
    val settings = state.settings
    var themeDialogSlot by remember { mutableStateOf<ThemeSlot?>(null) }
    val installedNames = state.installedThemes.map { it.name }
    fun themeOptions(current: String): List<Pair<String, String>> = buildList {
        add("Default (built-in)" to "")
        for (name in installedNames) add(name to name)
        if (current.isNotEmpty() && current !in installedNames) add("$current (missing)" to current)
    }
    val currentSlot = when (settings.themeMode) {
        ThemeMode.Dark -> ThemeSlot.Dark
        ThemeMode.Light -> ThemeSlot.Light
        ThemeMode.System -> if (isSystemInDarkTheme()) ThemeSlot.Dark else ThemeSlot.Light
    }
    val updateCount = state.themeUpdates.size
    SettingsGroup("Theme") {
        SettingsChoiceRow(
            title = "Base color scheme",
            description = "Obsidian's palette for the built-in theme and community themes",
            value = settings.themeMode,
            options = listOf(
                "Dark" to ThemeMode.Dark,
                "Light" to ThemeMode.Light,
                "Adapt to system" to ThemeMode.System,
            ),
            onSelect = controller::setThemeMode,
        )
        SettingsChoiceRow(
            title = "Light theme",
            description = "Independent theme used when the light scheme is active",
            value = settings.lightThemeName,
            options = themeOptions(settings.lightThemeName),
            onSelect = { controller.setThemeName(ThemeSlot.Light, it) },
        )
        SettingsChoiceRow(
            title = "Dark theme",
            description = "Independent theme used when the dark scheme is active",
            value = settings.darkThemeName,
            options = themeOptions(settings.darkThemeName),
            onSelect = { controller.setThemeName(ThemeSlot.Dark, it) },
        )
        SettingsActionRow(
            title = "Manage themes",
            description = if (state.themeCatalogLoading) {
                "Loading the catalog…"
            } else {
                "Install, update and remove themes; installs into the current scheme slot"
            },
            onClick = {
                controller.loadThemeCatalog()
                themeDialogSlot = currentSlot
            },
            showDivider = true,
            enabled = !state.themeCatalogLoading,
        )
        SettingsActionRow(
            title = "Check for updates",
            description = when {
                state.themeCheckingUpdates -> "Checking installed themes…"
                updateCount > 0 -> "$updateCount update${if (updateCount == 1) "" else "s"} available"
                else -> "Compare installed themes with GitHub"
            },
            onClick = controller::checkThemeUpdates,
            showDivider = false,
            enabled = state.installedThemes.isNotEmpty() && !state.themeCheckingUpdates,
        )
    }
    SettingsGroup("Accent") {
        SettingsAccentRow(
            selected = settings.accentColor,
            onSelect = controller::setAccentColor,
        )
    }
    if (state.themeWarnings.isNotEmpty()) {
        SettingsGroup("Compatibility") {
            SettingsInfoRow(
                title = "Theme notes",
                value = state.themeWarnings.joinToString("\n"),
                multiline = true,
            )
        }
    }
    themeDialogSlot?.let { slot ->
        ThemeManagerDialog(
            state = state,
            controller = controller,
            slot = slot,
            onClose = { themeDialogSlot = null },
        )
    }
}

@Composable
private fun SettingsAccentRow(selected: Int?, onSelect: (Int?) -> Unit) {
    val palette = LocalVibecodiumPalette.current
    val presets = listOf(
        "Theme" to null,
        "Violet" to 0xFF8A5CF5.toInt(),
        "Red" to 0xFFE93147.toInt(),
        "Green" to 0xFF08B94E.toInt(),
        "Blue" to 0xFF086DDD.toInt(),
        "Orange" to 0xFFEC7500.toInt(),
        "Pink" to 0xFFD53984.toInt(),
    )
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text("Accent color", fontSize = 14.sp, fontWeight = FontWeight.Medium)
        Text(
            "Used by buttons, links and selection; “Theme” keeps the theme's own accent",
            modifier = Modifier.padding(top = 2.dp),
            fontSize = 11.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(
            Modifier
                .fillMaxWidth()
                .padding(top = 10.dp)
                .horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            presets.forEach { (label, argb) ->
                val color = if (argb == null) palette.accent else Color(argb)
                val active = selected == argb
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Box(
                        Modifier
                            .size(34.dp)
                            .clip(CircleShape)
                            .background(color)
                            .border(
                                width = if (active) 3.dp else 1.dp,
                                color = if (active) MaterialTheme.colorScheme.onSurface else palette.outline,
                                shape = CircleShape,
                            )
                            .clickable { onSelect(argb) },
                    )
                    Text(
                        label,
                        modifier = Modifier.padding(top = 4.dp),
                        fontSize = 10.sp,
                        color = if (active) {
                            MaterialTheme.colorScheme.onSurface
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun ThemeManagerDialog(
    state: UiState,
    controller: VibecodiumController,
    slot: ThemeSlot,
    onClose: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    var installedOnly by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { controller.loadThemeCatalog() }

    val installed = state.installedThemes
    val installedByName = installed.associateBy { it.name }
    val merged = LinkedHashMap<String, ThemeEntry>()
    for (theme in installed) {
        merged[theme.name] = theme.entry?.copy(
            installedVersion = theme.version,
            updateVersion = state.themeUpdates[theme.name],
        ) ?: ThemeEntry(
            name = theme.name,
            author = theme.author,
            installedVersion = theme.version,
            updateVersion = state.themeUpdates[theme.name],
        )
    }
    for (entry in state.themeCatalog) {
        val current = merged[entry.name]
        merged[entry.name] = if (current == null) {
            entry.copy(
                installedVersion = installedByName[entry.name]?.version,
                updateVersion = state.themeUpdates[entry.name],
            )
        } else {
            entry.copy(
                installedVersion = current.installedVersion,
                updateVersion = state.themeUpdates[entry.name],
            )
        }
    }
    val needle = query.trim().lowercase()
    val visible = merged.values
        .filter { entry ->
            (needle.isEmpty() || entry.name.lowercase().contains(needle) ||
                entry.author.lowercase().contains(needle)) &&
                (!installedOnly || entry.installed)
        }
        .sortedWith(
            compareByDescending<ThemeEntry> { it.installed }
                .thenByDescending { it.downloads }
                .thenBy { it.name.lowercase() },
        )

    Dialog(onDismissRequest = onClose, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            shape = RoundedCornerShape(0.dp),
            color = MaterialTheme.colorScheme.surface,
        ) {
            Column(
                Modifier
                    .fillMaxSize()
                    .windowInsetsPadding(WindowInsets.safeDrawing),
            ) {
                Row(
                    Modifier.fillMaxWidth().height(52.dp).padding(start = 12.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        if (slot == ThemeSlot.Light) "Light theme" else "Dark theme",
                        modifier = Modifier.weight(1f),
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    TextButton(onClick = { installedOnly = !installedOnly }) {
                        Text(if (installedOnly) "Show all" else "Installed only")
                    }
                    IconButton(onClick = onClose, modifier = Modifier.size(44.dp)) {
                        Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                    }
                }
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                    singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(fontSize = 14.sp),
                    placeholder = { Text("Search themes", fontSize = 14.sp) },
                )
                if (state.themeCatalogLoading || state.themeCheckingUpdates) {
                    LinearProgressIndicator(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                }
                state.themeError?.let { error ->
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            error,
                            modifier = Modifier.weight(1f),
                            fontSize = 12.sp,
                            color = MaterialTheme.colorScheme.error,
                        )
                        TextButton(onClick = { controller.clearThemeError() }) { Text("Dismiss") }
                    }
                }
                if (visible.isEmpty() && !state.themeCatalogLoading) {
                    Text(
                        if (installedOnly) "No installed themes yet." else "No themes match this search.",
                        modifier = Modifier.padding(16.dp),
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                LazyColumn(Modifier.fillMaxWidth().weight(1f)) {
                    items(visible, key = { it.name }) { entry ->
                        ThemeCatalogRow(entry, slot, state, controller)
                        HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.4f))
                    }
                }
            }
        }
    }
}

@Composable
private fun ThemeCatalogRow(
    entry: ThemeEntry,
    slot: ThemeSlot,
    state: UiState,
    controller: VibecodiumController,
) {
    val lightActive = state.settings.lightThemeName == entry.name
    val darkActive = state.settings.darkThemeName == entry.name
    val active = if (slot == ThemeSlot.Light) lightActive else darkActive
    val busy = state.themeBusy == entry.name
    val badge = when {
        active -> "Active" to true
        lightActive && darkActive -> "Light + Dark" to false
        lightActive -> "Light" to false
        darkActive -> "Dark" to false
        entry.installed -> "Installed" to false
        else -> null
    }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(
                modifier = Modifier.padding(end = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                ThemeThumbnail(
                    entry = entry,
                    modifier = Modifier
                        .width(76.dp)
                        .height(57.dp),
                )
                badge?.let { (label, isActive) ->
                    ThemeBadge(label = label, active = isActive)
                }
            }
            Column(Modifier.weight(1f)) {
                Text(entry.name, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                val meta = buildString {
                    if (entry.author.isNotBlank()) append(entry.author)
                    if (entry.modesLabel.isNotBlank()) {
                        if (isNotEmpty()) append(" · ")
                        append(entry.modesLabel)
                    }
                    if (entry.downloads > 0) {
                        if (isNotEmpty()) append(" · ")
                        append(formatDownloads(entry.downloads))
                    }
                }
                if (meta.isNotBlank()) {
                    Text(
                        meta,
                        modifier = Modifier.padding(top = 2.dp),
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Column(
                modifier = Modifier.padding(start = 8.dp),
                horizontalAlignment = Alignment.End,
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (busy) {
                    CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                } else if (!entry.installed) {
                    ThemeActionButton("Install", primary = true) {
                        controller.installTheme(entry.name, slot)
                    }
                } else {
                    if (!active) {
                        ThemeActionButton("Use", primary = true) {
                            controller.setThemeName(slot, entry.name)
                        }
                    }
                    if (entry.updateVersion != null) {
                        ThemeActionButton("Update") { controller.updateTheme(entry.name) }
                    }
                    ThemeActionButton("Remove") { controller.uninstallTheme(entry.name) }
                }
            }
        }
    }
}

@Composable
private fun ThemeBadge(label: String, active: Boolean) {
    val primary = MaterialTheme.colorScheme.primary
    val background = if (active) {
        primary.copy(alpha = 0.14f)
    } else {
        MaterialTheme.colorScheme.surfaceContainerHigh
    }
    val border = if (active) primary.copy(alpha = 0.55f) else MaterialTheme.colorScheme.outline
    Box(
        Modifier
            .padding(top = 4.dp)
            .clip(CircleShape)
            .background(background)
            .border(1.dp, border, CircleShape)
            .padding(horizontal = 9.dp, vertical = 2.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = 9.sp,
            lineHeight = 11.sp,
            fontWeight = if (active) FontWeight.SemiBold else FontWeight.Medium,
            color = if (active) primary else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun ThemeActionButton(
    text: String,
    primary: Boolean = false,
    onClick: () -> Unit,
) {
    if (primary) {
        Button(
            onClick = onClick,
            modifier = Modifier.height(34.dp),
            contentPadding = PaddingValues(horizontal = 14.dp, vertical = 0.dp),
        ) {
            Text(text, fontSize = 12.sp, maxLines = 1)
        }
    } else {
        OutlinedButton(
            onClick = onClick,
            modifier = Modifier.height(34.dp),
            contentPadding = PaddingValues(horizontal = 14.dp, vertical = 0.dp),
        ) {
            Text(text, fontSize = 12.sp, maxLines = 1)
        }
    }
}

private fun formatDownloads(count: Int): String = when {
    count >= 1_000_000 -> "${count / 1_000_000}.${(count % 1_000_000) / 100_000}M"
    count >= 1_000 -> "${count / 1_000}.${(count % 1_000) / 100}K"
    else -> count.toString()
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
                ThemeAppearanceGroup(state, controller)
                SettingsGroup("Text") {
                    SettingsSliderRow(
                        title = "Text scale",
                        description = "Scales interface text.",
                        value = settings.textScale,
                        range = AppSettingsJson.MIN_TEXT_SCALE..AppSettingsJson.MAX_TEXT_SCALE,
                        format = { "${(it * 100).roundToInt()}%" },
                        onCommit = controller::setTextScale,
                        showDivider = true,
                    )
                    SettingsStepperRow(
                        title = "Editor font size",
                        description = "Body text in the source editor. Line spacing follows it.",
                        value = settings.editorFontDp,
                        range = AppSettingsJson.MIN_EDITOR_FONT_DP..AppSettingsJson.MAX_EDITOR_FONT_DP,
                        format = { "$it dp" },
                        onChange = controller::setEditorFontSize,
                        showDivider = true,
                    )
                    SettingsStepperRow(
                        title = "Code block font size",
                        description = "Fenced code blocks. Can be smaller than body text.",
                        value = settings.codeFontDp,
                        range = AppSettingsJson.MIN_CODE_FONT_DP..AppSettingsJson.MAX_CODE_FONT_DP,
                        format = { "$it dp" },
                        onChange = controller::setCodeFontSize,
                        showDivider = true,
                    )
                    SettingsChoiceRow(
                        title = "Text font",
                        description = "Markdown prose, lists, quotes and headings",
                        value = settings.textFontFace,
                        options = EDITOR_FONT_FACE_OPTIONS,
                        onSelect = controller::setTextFontFace,
                    )
                    EditorFontSample(settings.textFontFace, showDivider = true)
                    SettingsChoiceRow(
                        title = "Code font",
                        description = "Fenced and inline code, and code files",
                        value = settings.codeFontFace,
                        options = EDITOR_FONT_FACE_OPTIONS,
                        onSelect = controller::setCodeFontFace,
                    )
                    EditorFontSample(settings.codeFontFace)
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
                var colorHeading by remember { mutableStateOf<Int?>(null) }
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
                SettingsGroup("Headings") {
                    HEADING_LEVELS.forEachIndexed { index, (level, range) ->
                        HeadingStyleRow(
                            level = level,
                            style = settings.headings.style(level),
                            range = range,
                            onSizeChange = { controller.setHeadingFontSize(level, it) },
                            onColorClick = { colorHeading = level },
                            onToggleBold = { controller.toggleHeadingBold(level) },
                            onToggleItalic = { controller.toggleHeadingItalic(level) },
                            showDivider = index != HEADING_LEVELS.lastIndex,
                        )
                    }
                }
                colorHeading?.let { level ->
                    HeadingColorDialog(
                        title = "H$level",
                        current = settings.headings.style(level).colorName,
                        onDismiss = { colorHeading = null },
                        onSelect = { name ->
                            controller.setHeadingColor(level, name)
                            colorHeading = null
                        },
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

            SettingsSection.DailyNotes -> {
                val daily = settings.dailyNotes
                SettingsGroup("Daily notes") {
                    SettingsSwitchRow(
                        title = "Enable daily notes",
                        description = "Show the calendar button in the sidebar",
                        checked = daily.enabled,
                        onChange = controller::setDailyNotesEnabled,
                        showDivider = false,
                    )
                }
                SettingsGroup("Note") {
                    SettingsChoiceRow(
                        title = "Date format",
                        description = "Preset examples; the custom format below stays editable",
                        value = daily.format,
                        options = dailyFormatOptions(daily.format),
                        onSelect = controller::setDailyNoteFormat,
                        showDivider = true,
                    )
                    SettingsTextFieldRow(
                        title = "File name format",
                        description = "Placeholders: {{YYYY}} year, {{YY}} short year, " +
                            "{{MM}}/{{M}} month, {{DD}}/{{D}} day, {{HH}}/{{H}} hour, " +
                            "{{mm}}/{{m}} minute, {{ss}}/{{s}} second. Other text is used as is.",
                        value = daily.format,
                        placeholder = "{{YYYY}}-{{MM}}-{{DD}}",
                        onValueChange = controller::setDailyNoteFormat,
                        showDivider = true,
                    )
                    SettingsChoiceRow(
                        title = "Folder",
                        description = "Relative to the vault root; the note folder is created when needed",
                        value = daily.folder,
                        options = dailyFolderOptions(state, daily.folder),
                        onSelect = controller::setDailyNoteFolder,
                        showDivider = true,
                    )
                    SettingsChoiceRow(
                        title = "Template",
                        description = "Markdown note copied into a new daily note",
                        value = daily.template,
                        options = dailyTemplateOptions(state, daily.template),
                        onSelect = controller::setDailyNoteTemplate,
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
                .background(MaterialTheme.colorScheme.surface),
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
    showDivider: Boolean = false,
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
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

@Composable
private fun SettingsStepperRow(
    title: String,
    description: String,
    value: Int,
    range: IntRange,
    format: (Int) -> String,
    onChange: (Int) -> Unit,
    showDivider: Boolean = false,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 60.dp)
            .padding(start = 16.dp, end = 6.dp, top = 10.dp, bottom = 10.dp),
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
        Spacer(Modifier.width(8.dp))
        IconButton(
            onClick = { onChange(value - 1) },
            enabled = value > range.first,
            modifier = Modifier.size(40.dp),
        ) {
            Icon(
                MinusIcon,
                contentDescription = "Decrease $title",
                modifier = Modifier.size(18.dp),
            )
        }
        Text(
            format(value),
            modifier = Modifier.widthIn(min = 44.dp),
            textAlign = TextAlign.Center,
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        IconButton(
            onClick = { onChange(value + 1) },
            enabled = value < range.last,
            modifier = Modifier.size(40.dp),
        ) {
            Icon(
                Icons.Default.Add,
                contentDescription = "Increase $title",
                modifier = Modifier.size(18.dp),
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
private fun HeadingStyleRow(
    level: Int,
    style: HeadingStyle,
    range: IntRange,
    onSizeChange: (Int) -> Unit,
    onColorClick: () -> Unit,
    onToggleBold: () -> Unit,
    onToggleItalic: () -> Unit,
    showDivider: Boolean,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .padding(start = 16.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text("H$level", fontSize = 14.sp, fontWeight = FontWeight.Medium)
            Text(
                HeadingColors.hex(style.colorName),
                modifier = Modifier.padding(top = 2.dp),
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(
            onClick = { onSizeChange(style.fontSize - 1) },
            enabled = style.fontSize > range.first,
            modifier = Modifier.size(32.dp),
        ) {
            Icon(MinusIcon, contentDescription = "Decrease H$level size", modifier = Modifier.size(16.dp))
        }
        Text(
            "${style.fontSize}",
            modifier = Modifier.widthIn(min = 28.dp),
            textAlign = TextAlign.Center,
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        IconButton(
            onClick = { onSizeChange(style.fontSize + 1) },
            enabled = style.fontSize < range.last,
            modifier = Modifier.size(32.dp),
        ) {
            Icon(Icons.Default.Add, contentDescription = "Increase H$level size", modifier = Modifier.size(16.dp))
        }
        Box(
            Modifier
                .padding(start = 4.dp)
                .size(26.dp)
                .clip(CircleShape)
                .background(headingColor(HeadingColors.rgb(style.colorName)))
                .border(1.dp, MaterialTheme.colorScheme.outline, CircleShape)
                .clickable(onClick = onColorClick),
        )
        HeadingFormatToggle(
            label = "B",
            active = style.bold,
            italic = false,
            onClick = onToggleBold,
        )
        HeadingFormatToggle(
            label = "I",
            active = style.italic,
            italic = true,
            onClick = onToggleItalic,
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
private fun HeadingFormatToggle(
    label: String,
    active: Boolean,
    italic: Boolean,
    onClick: () -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary
    val shape = RoundedCornerShape(6.dp)
    Box(
        Modifier
            .padding(start = 4.dp)
            .size(30.dp)
            .clip(shape)
            .background(if (active) accent.copy(alpha = 0.18f) else Color.Transparent)
            .border(1.dp, if (active) accent else MaterialTheme.colorScheme.outline, shape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = 12.sp,
            fontWeight = FontWeight.Bold,
            fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal,
            color = if (active) accent else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** Palette and custom hex picker for one heading level, following the desktop popup. */
@Composable
private fun HeadingColorDialog(
    title: String,
    current: String,
    onDismiss: () -> Unit,
    onSelect: (String) -> Unit,
) {
    var hex by remember(current) { mutableStateOf(HeadingColors.hex(current)) }
    val parsed = HeadingColors.parseHex(hex)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("$title colour") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                HeadingColors.presets.chunked(4).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        row.forEach { preset ->
                            val selected = preset.rgb == HeadingColors.rgb(current)
                            Box(
                                Modifier
                                    .size(30.dp)
                                    .clip(CircleShape)
                                    .background(headingColor(preset.rgb))
                                    .border(
                                        width = if (selected) 2.dp else 1.dp,
                                        color = if (selected) {
                                            MaterialTheme.colorScheme.primary
                                        } else {
                                            MaterialTheme.colorScheme.outline
                                        },
                                        shape = CircleShape,
                                    )
                                    .clickable { onSelect(preset.id) },
                            )
                        }
                    }
                }
                OutlinedTextField(
                    value = hex,
                    onValueChange = { hex = it },
                    label = { Text("Hex") },
                    singleLine = true,
                    isError = parsed == null,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { parsed?.let { onSelect("#%06X".format(it)) } },
                enabled = parsed != null,
            ) {
                Text("Apply")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

private fun headingColor(rgb: Int): Color = Color(0xFF000000.toInt() or rgb)

private val EDITOR_FONT_FACE_OPTIONS = listOf(
    "System" to EditorFontFace.System,
    "Monospace" to EditorFontFace.Mono,
    "Serif" to EditorFontFace.Serif,
)

private fun EditorFontFace.previewFamily(): FontFamily = when (this) {
    EditorFontFace.System -> FontFamily.Default
    EditorFontFace.Mono -> FontFamily.Monospace
    EditorFontFace.Serif -> FontFamily.Serif
}

/** Sample text under a font choice so the difference is visible before switching. */
@Composable
private fun EditorFontSample(face: EditorFontFace, showDivider: Boolean = false) {
    val family = face.previewFamily()
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = 2.dp, bottom = 10.dp),
    ) {
        Text(
            "Aa Бб 123 — съешь ещё этих мягких французских булок",
            fontSize = 14.sp,
            fontFamily = family,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            "if (x == 42) { return \"OK\"; }",
            modifier = Modifier.padding(top = 2.dp),
            fontSize = 13.sp,
            fontFamily = family,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
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
private fun <T> SettingsChoiceRow(
    title: String,
    description: String,
    value: T,
    options: List<Pair<String, T>>,
    onSelect: (T) -> Unit,
    showDivider: Boolean = false,
) {
    var open by remember { mutableStateOf(false) }
    val label = options.firstOrNull { it.second == value }?.first ?: ""
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
        // The value button sits under the description and spans the whole row:
        // a grey field reads as an input, unlike a value tucked to the right.
        val buttonShape = RoundedCornerShape(14.dp)
        Row(
            Modifier
                .fillMaxWidth()
                .padding(top = 8.dp)
                .clip(buttonShape)
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .border(1.dp, MaterialTheme.colorScheme.outline, buttonShape)
                .clickable { open = true }
                .padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                label,
                modifier = Modifier.weight(1f),
                fontSize = 13.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.width(8.dp))
            Icon(
                Icons.Default.KeyboardArrowDown,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    if (open) {
        SettingsChoiceDialog(
            title = title,
            value = value,
            options = options,
            onSelect = { optionValue ->
                open = false
                onSelect(optionValue)
            },
            onDismiss = { open = false },
        )
    }
    if (showDivider) {
        HorizontalDivider(
            modifier = Modifier.padding(start = 16.dp),
            color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
        )
    }
}

/**
 * Modal option picker. A long choice list (daily-note folders, templates)
 * scrolls inside the dialog with the same thin scrollbar as the tab menu.
 */
@Composable
private fun <T> SettingsChoiceDialog(
    title: String,
    value: T,
    options: List<Pair<String, T>>,
    onSelect: (T) -> Unit,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.55f))
                .pointerInput(Unit) { detectTapGestures(onTap = { onDismiss() }) },
            contentAlignment = Alignment.Center,
        ) {
            // Cap the list against the dialog window itself: a LazyColumn here
            // would stretch the card to the window height.
            val listMaxHeight = (maxHeight * 0.6f).coerceAtLeast(200.dp)
            Surface(
                modifier = Modifier
                    .widthIn(max = 560.dp)
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp)
                    // Swallow taps inside the card so they do not reach the
                    // dismiss-on-tap scrim behind it.
                    .pointerInput(Unit) { detectTapGestures { } },
                shape = RoundedCornerShape(20.dp),
                color = MaterialTheme.colorScheme.surface,
                tonalElevation = 6.dp,
                shadowElevation = 10.dp,
            ) {
                Column {
                    Text(
                        title,
                        modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 12.dp),
                        fontSize = 15.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
                    Box(Modifier.fillMaxWidth()) {
                        val scrollState = rememberScrollState()
                        var viewportPx by remember { mutableStateOf(0) }
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(max = listMaxHeight)
                                // Measure the clipped viewport, not the content:
                                // the scroll node must stay outside the observer.
                                .onSizeChanged { viewportPx = it.height }
                                .verticalScroll(scrollState)
                                .padding(vertical = 6.dp),
                        ) {
                            options.forEach { option ->
                                val (optionLabel, optionValue) = option
                                val selected = optionValue == value
                                Row(
                                    Modifier
                                        .fillMaxWidth()
                                        .clickable { onSelect(optionValue) }
                                        .padding(horizontal = 20.dp, vertical = 9.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(
                                        optionLabel,
                                        modifier = Modifier.weight(1f),
                                        fontSize = 14.sp,
                                        color = if (selected) {
                                            MaterialTheme.colorScheme.primary
                                        } else {
                                            MaterialTheme.colorScheme.onSurface
                                        },
                                    )
                                    if (selected) {
                                        Spacer(Modifier.width(10.dp))
                                        Icon(
                                            Icons.Default.Check,
                                            contentDescription = null,
                                            modifier = Modifier.size(18.dp),
                                            tint = MaterialTheme.colorScheme.primary,
                                        )
                                    }
                                }
                            }
                        }
                        val density = LocalDensity.current
                        val contentPx = viewportPx + scrollState.maxValue
                        if (scrollState.maxValue > 0 && viewportPx > 0 && contentPx > 0) {
                            val track = with(density) { viewportPx.toDp() }
                            val thumb = (track * (viewportPx.toFloat() / contentPx))
                                .coerceAtLeast(24.dp)
                                .coerceAtMost(track)
                            val maxOffset = (track - thumb).coerceAtLeast(0.dp)
                            val offset = maxOffset * (scrollState.value.toFloat() / scrollState.maxValue)
                            Box(
                                Modifier
                                    .align(Alignment.TopEnd)
                                    .padding(end = 3.dp)
                                    .offset(y = offset)
                                    .width(6.dp)
                                    .height(thumb)
                                    .clip(CircleShape)
                                    .background(Color(0xFF9E9E9E))
                                    .border(1.dp, Color(0xFF4A4A4A), CircleShape),
                            )
                        }
                    }
                }
            }
        }
    }
}

private data class DailyFormatPreset(val title: String?, val format: String)

private val DAILY_FORMAT_PRESETS = listOf(
    DailyFormatPreset("Y-M-D", "{{YYYY}}-{{MM}}-{{DD}}"),
    DailyFormatPreset("D-M-Y", "{{DD}}-{{MM}}-{{YYYY}}"),
    DailyFormatPreset("D.M.Y", "{{DD}}.{{MM}}.{{YYYY}}"),
    DailyFormatPreset("D/M/Y", "{{DD}}/{{MM}}/{{YYYY}}"),
    DailyFormatPreset("Y.M.D", "{{YYYY}}.{{MM}}.{{DD}}"),
    DailyFormatPreset("Y M D", "{{YYYY}} {{MM}} {{DD}}"),
    DailyFormatPreset(null, "{{D}} {{MMMM}} {{YYYY}}"),
    DailyFormatPreset(null, "{{D}} {{MMM}} {{YYYY}}"),
    DailyFormatPreset(null, "{{MMMM}} {{D}}, {{YYYY}}"),
    DailyFormatPreset(null, "{{MMM}} {{D}}, {{YYYY}}"),
)

private fun dailyFormatOptions(current: String): List<Pair<String, String>> {
    val sample = LocalDateTime.of(2026, 11, 12, 0, 0)
    val options = DAILY_FORMAT_PRESETS.map { preset ->
        val example = expandDailyNoteFormat(preset.format, sample)
        val label = if (preset.title == null) example else "$example · ${preset.title}"
        label to preset.format
    }
    if (options.none { it.second == current }) {
        val label = if (current.isEmpty()) {
            "ISO date fallback · custom"
        } else {
            "${expandDailyNoteFormat(current, sample)} · custom"
        }
        return listOf(label to current) + options
    }
    return options
}

private fun dailyFolderOptions(state: UiState, current: String): List<Pair<String, String>> =
    buildList {
        add("Vault root" to "")
        for (folder in state.folders) {
            add(folder to folder)
        }
        if (current.isNotEmpty() && current !in state.folders) {
            add(current to current)
        }
    }

private fun dailyTemplateOptions(state: UiState, current: String): List<Pair<String, String>> {
    val notes = state.notes
        .map { it.path }
        .filter { it.endsWith(".md", ignoreCase = true) }
        .sortedBy { it.lowercase(Locale.ROOT) }
    return buildList {
        add("No template" to "")
        for (path in notes) {
            add(path to path)
        }
        if (current.isNotEmpty() && notes.none { it == current }) {
            add(current to current)
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
    /** Long values (error messages) need the full row and wrap instead of squeezing the title. */
    multiline: Boolean = false,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        if (multiline) {
            Text(
                title,
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
            )
            Text(
                value,
                modifier = Modifier.padding(top = 4.dp),
                fontSize = 13.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
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
            SettingsInfoRow("Last result", sync.message, showDivider = false, multiline = true)
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
        if (folders.isNotEmpty()) {
            HorizontalDivider(
                modifier = Modifier.padding(start = 16.dp),
                color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
            )
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
        sync.busy -> when {
            sync.progressKind == SyncProgressKind.ReadingLocal && sync.progressTotal > 0 ->
                "Reading local files ${sync.progressDone}/${sync.progressTotal}"
            sync.progressKind == SyncProgressKind.Transferring && sync.progressTotal > 0 ->
                "Syncing cloud files ${sync.progressDone}/${sync.progressTotal}"
            else -> "Syncing…"
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
