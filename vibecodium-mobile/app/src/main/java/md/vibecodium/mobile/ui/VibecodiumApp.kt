package md.vibecodium.mobile.ui

import android.graphics.BitmapFactory
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import md.vibecodium.mobile.Drawer
import md.vibecodium.mobile.DrawerMode
import md.vibecodium.mobile.DocStats
import md.vibecodium.mobile.EditorMode
import md.vibecodium.mobile.CodeColors
import md.vibecodium.mobile.ExplorerRow
import md.vibecodium.mobile.LocalCodeColors
import md.vibecodium.mobile.LIVE_MODEL_CHAR_LIMIT
import md.vibecodium.mobile.MarkdownLine
import md.vibecodium.mobile.MarkdownSpan
import md.vibecodium.mobile.NoteItem
import md.vibecodium.mobile.R
import md.vibecodium.mobile.Tab
import md.vibecodium.mobile.UiState
import md.vibecodium.mobile.VibecodiumController
import md.vibecodium.mobile.filterExplorerTree
import md.vibecodium.mobile.flattenExplorerTree
import md.vibecodium.mobile.indentAtCursor
import md.vibecodium.mobile.outdentAtCursor
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** `--navbar-height: var(--touch-size-l)` in the Obsidian phone stylesheet. */
private val NavbarHeight = 52.dp

/** `--view-header-height: var(--touch-size-m)` on a phone. */
private val ViewHeaderHeight = 44.dp

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
            if (state.vaultName == null) {
                StartScreen(state, controller)
            } else {
                PhoneShell(state, controller)
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
        }
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
        Column(Modifier.fillMaxSize()) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                if (editor.value.tabs.isEmpty()) {
                    EmptyEditor(controller)
                } else {
                    EditorScreen(editor.value, controller)
                }

                when (state.drawer) {
                    Drawer.Left -> {
                        DrawerBackdrop { controller.closeOverlays() }
                        LeftDrawer(
                            state,
                            controller,
                            Modifier
                                .align(Alignment.CenterStart)
                                .width(drawerWidth)
                                .fillMaxHeight(),
                        )
                    }

                    Drawer.Right -> {
                        DrawerBackdrop { controller.closeOverlays() }
                        RightDrawer(
                            state,
                            controller,
                            Modifier
                                .align(Alignment.CenterEnd)
                                .width(drawerWidth)
                                .fillMaxHeight(),
                        )
                    }

                    null -> Unit
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
            MobileNavbar(state, controller)
        }
    }
}

/** `--mobile-sidebar-width: 84vw`, clamped to 300–450 px in the phone sheet. */
private fun phoneDrawerWidth(maxWidth: Dp): Dp {
    if (maxWidth <= 300.dp) return maxWidth
    return (maxWidth * 0.84f).coerceIn(300.dp, minOf(420.dp, maxWidth))
}

@Composable
private fun EditorScreen(state: EditorSlice, controller: VibecodiumController) {
    val tab = state.tabs.getOrNull(state.activeTab) ?: return
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        ViewHeader(state, tab, controller)
        when (state.editorMode) {
            EditorMode.Source -> SourceEditor(state, controller, tab)
            EditorMode.Preview -> if (tab.language != null) {
                CodeViewer(tab, Modifier.fillMaxWidth().weight(1f))
            } else {
                MarkdownPreview(state, controller, tab)
            }
        }
    }
}

@Composable
private fun ViewHeader(state: EditorSlice, tab: Tab, controller: VibecodiumController) {
    val bookmarked = tab.path in state.bookmarks
    val preview = state.editorMode == EditorMode.Preview
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(ViewHeaderHeight)
                .padding(start = 14.dp, end = 2.dp),
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
            Text(
                "${state.stats.words} w",
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            IconButton(onClick = { controller.toggleBookmarkActive() }) {
                Icon(
                    Icons.Default.Star,
                    contentDescription = if (bookmarked) "Remove bookmark" else "Bookmark",
                    modifier = Modifier.size(20.dp),
                    tint = if (bookmarked) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                )
            }
            IconButton(onClick = {
                controller.setEditorMode(if (preview) EditorMode.Source else EditorMode.Preview)
            }) {
                Icon(
                    if (preview) Icons.Default.Edit else Icons.Default.List,
                    contentDescription = if (preview) "Edit" else "Preview",
                    modifier = Modifier.size(20.dp),
                )
            }
            if (tab.language == null) {
                IconButton(onClick = { controller.toggleDrawer(Drawer.Right) }) {
                    Icon(
                        Icons.Default.List,
                        contentDescription = "Outline",
                        modifier = Modifier.size(20.dp),
                    )
                }
            }
            IconButton(onClick = { controller.saveActiveNow() }) {
                Icon(
                    Icons.Default.Check,
                    contentDescription = "Save",
                    modifier = Modifier.size(20.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
    }
}

// --------------------------------------------------------------------- source

private data class SourceColors(
    val accent: Color,
    val muted: Color,
    val code: CodeColors,
)

@Composable
private fun SourceEditor(state: EditorSlice, controller: VibecodiumController, tab: Tab) {
    val scroll = rememberScrollState()
    val value = TextFieldValue(
        text = tab.text,
        selection = TextRange(tab.selection.coerceIn(0, tab.text.length)),
    )
    val codeColors = LocalCodeColors.current
    val colors = SourceColors(
        accent = MaterialTheme.colorScheme.primary,
        muted = MaterialTheme.colorScheme.onSurfaceVariant,
        code = codeColors,
    )
    val isCode = tab.language != null
    val cursorLine = remember(tab.selection, tab.text) {
        tab.text.take(tab.selection.coerceIn(0, tab.text.length)).count { it == '\n' }
    }
    val transformation: VisualTransformation = when {
        tab.language != null -> CodeSourceHighlight(tab.language, codeColors)
        state.lines.isNotEmpty() && tab.text.length <= LIVE_MODEL_CHAR_LIMIT ->
            MarkdownModelHighlight(
                modelText = state.markdownText,
                lines = state.lines,
                cursorLine = cursorLine,
                colors = colors,
                codeColors = codeColors,
            )
        else -> VisualTransformation.None
    }
    val lineCount = remember(tab.text) { tab.text.count { it == '\n' } + 1 }
    // One text block instead of one composable per line: a 10k-line note used
    // to create 10k measure/layout nodes before the first frame.
    val lineNumbers = remember(lineCount) {
        buildString {
            for (line in 1..lineCount) {
                if (line > 1) append('\n')
                append(line)
            }
        }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(if (isCode) codeColors.background else Color.Transparent),
    ) {
        Row(Modifier.fillMaxWidth().verticalScroll(scroll)) {
            Text(
                text = lineNumbers,
                modifier = Modifier
                    .padding(start = 8.dp, top = 10.dp, bottom = 10.dp)
                    .widthIn(min = 30.dp),
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
                lineHeight = EditorLineHeight,
                color = if (isCode) {
                    codeColors.comment
                } else {
                    MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)
                },
                textAlign = TextAlign.End,
            )
            BasicTextField(
                value = value,
                onValueChange = controller::updateEditor,
                modifier = Modifier
                    .weight(1f)
                    .padding(start = 10.dp, end = 12.dp, top = 10.dp, bottom = 10.dp)
                    .onPreviewKeyEvent { event ->
                        if (event.type == KeyEventType.KeyDown && event.key == Key.Tab) {
                            controller.updateEditor(
                                if (event.isShiftPressed) {
                                    outdentAtCursor(value)
                                } else {
                                    indentAtCursor(value)
                                },
                            )
                            true
                        } else {
                            false
                        }
                    },
                textStyle = TextStyle(
                    color = if (isCode) codeColors.normal else MaterialTheme.colorScheme.onBackground,
                    fontSize = EditorFontSize,
                    lineHeight = EditorLineHeight,
                    fontFamily = FontFamily.Monospace,
                ),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                visualTransformation = transformation,
            )
        }
    }
}

/// Applies the Rust markdown model to the source editor. If the model is
/// momentarily behind the text (it is computed after a short debounce), the raw
/// text is shown unstyled until the next model arrives.
private class MarkdownModelHighlight(
    private val modelText: String,
    private val lines: List<MarkdownLine>,
    private val cursorLine: Int,
    private val colors: SourceColors,
    private val codeColors: CodeColors,
) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        if (text.text != modelText || lines.isEmpty()) {
            return TransformedText(text, OffsetMapping.Identity)
        }
        return TransformedText(
            buildModelAnnotated(text.text, lines, cursorLine, colors, codeColors),
            OffsetMapping.Identity,
        )
    }
}

private fun buildModelAnnotated(
    text: String,
    lines: List<MarkdownLine>,
    cursorLine: Int,
    colors: SourceColors,
    codeColors: CodeColors,
): AnnotatedString {
    val sourceLines = text.split('\n').let { if (text.endsWith('\n')) it.dropLast(1) else it }
    if (sourceLines.size != lines.size) return AnnotatedString(text)

    // Markers keep their char cells (they only turn into spaces or a rendered
    // glyph), so the caret offsets stay 1:1 with the original text. The line
    // with the caret keeps its raw markers, which is the Obsidian live-preview
    // behaviour.
    val chars = text.toCharArray()
    var lineStart = 0
    for (index in lines.indices) {
        val raw = sourceLines[index]
        if (index != cursorLine) {
            hideIdleMarkers(chars, lineStart, raw, lines[index])
        }
        lineStart += raw.length + 1
    }

    return buildAnnotatedString {
        append(String(chars))
        var offset = 0
        for (index in lines.indices) {
            val line = lines[index]
            val raw = sourceLines[index]
            val start = offset
            val active = index == cursorLine
            val markerChars = line.prefixChars.coerceIn(0, raw.length)
            if (markerChars > 0 && active) {
                addStyle(
                    SpanStyle(color = colors.accent, fontWeight = FontWeight.Medium),
                    start,
                    start + markerChars,
                )
            }
            if (!active && markerChars > 0 && (line.kind == "bullet" || line.kind == "task")) {
                val glyph = if (line.kind == "bullet") {
                    markerChars - 2
                } else {
                    (0 until markerChars).firstOrNull {
                        raw.getOrNull(it) != ' ' && raw.getOrNull(it) != '\t'
                    } ?: 0
                }
                addStyle(
                    SpanStyle(color = colors.accent),
                    start + glyph,
                    start + glyph + 1,
                )
            }
            when (line.kind) {
                "heading" -> addStyle(
                    SpanStyle(color = colors.accent, fontWeight = FontWeight.Bold),
                    start,
                    start + raw.length,
                )

                "quote" -> addStyle(SpanStyle(color = colors.muted), start, start + raw.length)

                "code" -> {
                    addStyle(codeStyle(colors), start, start + raw.length)
                    addCodeLineStyles(raw, line.language.orEmpty(), start, codeColors)
                }

                "code_fence" -> addStyle(codeStyle(colors), start, start + raw.length)

                "rule" -> addStyle(SpanStyle(color = colors.muted), start, start + raw.length)

                "table", "table_row", "table_separator", "image" -> Unit

                else -> {
                    val contentStart = start + line.prefixChars
                    for (span in line.spans) {
                        if (span.kind == "marker" && !active) continue
                        val spanStart = contentStart + span.start
                        val spanEnd = contentStart + span.end
                        spanStyle(span.kind, active, colors, codeColors)?.let { style ->
                            addStyle(style, spanStart, spanEnd)
                        }
                    }
                }
            }
            offset = start + raw.length + 1
        }
    }
}

private fun hideIdleMarkers(
    chars: CharArray,
    lineStart: Int,
    raw: String,
    line: MarkdownLine,
) {
    val markerChars = line.prefixChars.coerceIn(0, raw.length)
    when (line.kind) {
        "heading" -> for (index in 0 until markerChars) {
            chars[lineStart + index] = ' '
        }

        "quote" -> for (index in 0 until markerChars) {
            chars[lineStart + index] = if (chars[lineStart + index] == '>') '│' else ' '
        }

        "bullet" -> {
            for (index in 0 until markerChars) chars[lineStart + index] = ' '
            if (markerChars >= 2) {
                chars[lineStart + markerChars - 2] = '•'
                chars[lineStart + markerChars - 1] = ' '
            }
        }

        "task" -> {
            val box = if (line.checked == true) '☑' else '☐'
            val first = (0 until markerChars).firstOrNull {
                raw.getOrNull(it) != ' ' && raw.getOrNull(it) != '\t'
            } ?: 0
            for (index in 0 until markerChars) chars[lineStart + index] = ' '
            if (markerChars > 0) chars[lineStart + first] = box
        }

        "code_fence" -> for (index in 0 until raw.length) {
            chars[lineStart + index] = ' '
        }

        "numbered", "rule", "image", "table", "table_row", "table_separator" -> Unit

        else -> Unit
    }

    if (line.kind !in CODE_OR_BLOCK_KINDS) {
        val contentStart = lineStart + line.prefixChars
        for (span in line.spans) {
            if (span.kind != "marker") continue
            val start = contentStart + span.start
            val end = minOf(contentStart + span.end, chars.size)
            for (index in start until end) {
                if (index in chars.indices) chars[index] = ' '
            }
        }
    }
}

private val CODE_OR_BLOCK_KINDS = setOf(
    "code", "code_fence", "rule", "image", "table", "table_row", "table_separator",
)

private fun spanStyle(
    kind: String,
    active: Boolean,
    colors: SourceColors,
    codeColors: CodeColors,
): SpanStyle? = when (kind) {
    "bold" -> SpanStyle(fontWeight = FontWeight.Bold)
    "italic" -> SpanStyle(fontStyle = FontStyle.Italic)
    "bold_italic" -> SpanStyle(fontWeight = FontWeight.Bold, fontStyle = FontStyle.Italic)
    "inline_code" -> SpanStyle(
        color = codeColors.normal,
        background = codeColors.background,
        fontFamily = FontFamily.Monospace,
    )
    "strikethrough" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
    "link" -> SpanStyle(color = colors.accent, textDecoration = TextDecoration.Underline)
    "underline" -> SpanStyle(textDecoration = TextDecoration.Underline)
    "highlight" -> SpanStyle(background = Color(0x33A882FF))
    "tag" -> SpanStyle(color = colors.accent)
    "marker" -> SpanStyle(color = if (active) colors.accent else colors.muted)
    "grammar" -> SpanStyle(fontStyle = FontStyle.Italic)
    else -> null
}

private fun codeStyle(colors: SourceColors) = SpanStyle(
    color = colors.code.normal,
    background = colors.code.background,
    fontFamily = FontFamily.Monospace,
)

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
private fun MarkdownPreview(state: EditorSlice, controller: VibecodiumController, tab: Tab) {
    val blocks = remember(state.lines) { buildPreviewBlocks(state.lines) }
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    LazyColumn(
        Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 10.dp),
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
private fun CodeViewer(tab: Tab, modifier: Modifier) {
    val language = tab.language ?: return
    val colors = LocalCodeColors.current
    val clipboard = LocalClipboardManager.current
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()
    val lineCount = tab.text.count { it == '\n' } + 1
    Column(modifier.background(colors.background)) {
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
                Modifier.padding(start = 8.dp, end = 10.dp, bottom = 12.dp),
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
private fun MobileNavbar(state: UiState, controller: VibecodiumController) {
    var menuOpen by remember { mutableStateOf(false) }
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(NavbarHeight)
                .padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NavButton(
                icon = Icons.Default.Menu,
                label = "Drawer",
                active = state.drawer == Drawer.Left,
            ) { controller.toggleDrawer(Drawer.Left) }
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
                    Icons.Default.MoreVert,
                    contentDescription = "More",
                    modifier = Modifier.size(22.dp),
                    tint = MaterialTheme.colorScheme.onBackground,
                )
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    DropdownMenuItem(
                        text = { Text("Save note") },
                        onClick = { menuOpen = false; controller.saveActiveNow() },
                    )
                    DropdownMenuItem(
                        text = { Text("Rename note") },
                        onClick = { menuOpen = false; controller.beginRename() },
                    )
                    DropdownMenuItem(
                        text = { Text("Toggle bookmark") },
                        onClick = { menuOpen = false; controller.toggleBookmarkActive() },
                    )
                    DropdownMenuItem(
                        text = {
                            Text(
                                if (state.editorMode == EditorMode.Source) "Preview" else "Edit",
                            )
                        },
                        onClick = {
                            menuOpen = false
                            controller.setEditorMode(
                                if (state.editorMode == EditorMode.Source) EditorMode.Preview else EditorMode.Source,
                            )
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("Outline") },
                        onClick = { menuOpen = false; controller.toggleDrawer(Drawer.Right) },
                    )
                    DropdownMenuItem(
                        text = { Text("Sync") },
                        onClick = { menuOpen = false; controller.openSync() },
                    )
                    DropdownMenuItem(
                        text = { Text("Close tab") },
                        onClick = { menuOpen = false; controller.closeTab(state.activeTab) },
                    )
                    DropdownMenuItem(
                        text = { Text("Delete note") },
                        onClick = { menuOpen = false; controller.confirmDeleteActive() },
                    )
                    HorizontalDivider()
                    DropdownMenuItem(
                        text = { Text("Open another vault") },
                        onClick = { menuOpen = false; controller.closeVault() },
                    )
                    DropdownMenuItem(
                        text = { Text("About") },
                        onClick = {
                            menuOpen = false
                            controller.showMessage("vibecodium · Rust core 0.1.0")
                        },
                    )
                }
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
    Surface(modifier = modifier, color = MaterialTheme.colorScheme.surface, tonalElevation = 8.dp) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(52.dp)
                    .padding(start = 14.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(
                        state.vaultName ?: "Vault",
                        fontSize = 15.sp,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val fileCount = if (state.entries.isNotEmpty()) state.entries.size else state.notes.size
                    Text(
                        "$fileCount files · ${syncSubtitle(state)}",
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = { controller.refreshNotes() }) {
                    Icon(
                        Icons.Default.Refresh,
                        contentDescription = "Refresh files",
                        modifier = Modifier.size(20.dp),
                    )
                }
                IconButton(onClick = { controller.toggleDrawer(Drawer.Right) }) {
                    Icon(
                        Icons.Default.KeyboardArrowRight,
                        contentDescription = "Right panel",
                        modifier = Modifier.size(20.dp),
                    )
                }
                IconButton(onClick = { controller.closeOverlays() }) {
                    Icon(Icons.Default.Close, contentDescription = "Close", modifier = Modifier.size(20.dp))
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(44.dp)
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ModeTab("Files", Icons.Default.Home, state.drawerMode == DrawerMode.Files) {
                    controller.setDrawerMode(DrawerMode.Files)
                }
                ModeTab("Search", Icons.Default.Search, state.drawerMode == DrawerMode.Search) {
                    controller.setDrawerMode(DrawerMode.Search)
                }
                ModeTab("Stars", Icons.Default.Star, state.drawerMode == DrawerMode.Bookmarks) {
                    controller.setDrawerMode(DrawerMode.Bookmarks)
                }
                ModeTab("Recent", Icons.Default.Refresh, state.drawerMode == DrawerMode.Recent) {
                    controller.setDrawerMode(DrawerMode.Recent)
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            Box(Modifier.weight(1f).fillMaxWidth()) {
                when (state.drawerMode) {
                    DrawerMode.Files -> TreeList(state, controller)
                    DrawerMode.Search -> SearchPanel(state, controller)
                    DrawerMode.Bookmarks -> BookmarkedList(state, controller)
                    DrawerMode.Recent -> RecentList(state, controller)
                }
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f))
            TextButton(
                onClick = { controller.closeVault() },
                modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            ) {
                Text("Open another vault")
            }
        }
    }
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
    Column(Modifier.fillMaxSize()) {
        ExplorerToolbar(state, controller)
        OutlinedTextField(
            value = state.fileQuery,
            onValueChange = controller::setFileQuery,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
            placeholder = { Text("Filter files", fontSize = 13.sp) },
            leadingIcon = {
                Icon(Icons.Default.Search, contentDescription = null, modifier = Modifier.size(18.dp))
            },
            trailingIcon = if (state.fileQuery.isEmpty()) {
                null
            } else {
                {
                    IconButton(onClick = { controller.setFileQuery("") }) {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = "Clear filter",
                            modifier = Modifier.size(16.dp),
                        )
                    }
                }
            },
            singleLine = true,
            textStyle = androidx.compose.ui.text.TextStyle(fontSize = 13.sp),
        )
        Spacer(Modifier.height(4.dp))
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
        LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
            items(rows, key = { it.node.path }) { row ->
                ExplorerTreeRow(row, activePath, state, controller)
                HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
            }
        }
    }
}

/** Toolbar above the instant file filter. */
@Composable
private fun ExplorerToolbar(state: UiState, controller: VibecodiumController) {
    val allCollapsed = state.folders.isNotEmpty() &&
        state.folders.all { it in state.collapsedFolders }
    Row(
        Modifier
            .fillMaxWidth()
            .height(40.dp)
            .padding(horizontal = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextButton(
            onClick = { controller.toggleAllFolders() },
            enabled = state.folders.isNotEmpty(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 8.dp),
        ) {
            Icon(
                if (allCollapsed) Icons.Default.KeyboardArrowDown else Icons.Default.KeyboardArrowUp,
                contentDescription = if (allCollapsed) "Expand all folders" else "Collapse all folders",
                modifier = Modifier.size(18.dp),
            )
            Spacer(Modifier.width(4.dp))
            Text(if (allCollapsed) "Expand all" else "Collapse all", fontSize = 11.sp)
        }
        Spacer(Modifier.weight(1f))
        Text(
            "${state.entries.size} files",
            fontSize = 11.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(end = 8.dp),
        )
    }
}

@Composable
private fun ExplorerTreeRow(
    row: ExplorerRow,
    activePath: String?,
    state: UiState,
    controller: VibecodiumController,
) {
    val node = row.node
    val isFolder = node.isFolder
    val collapsed = node.path in state.collapsedFolders
    val hasChildren = node.children.isNotEmpty()
    Row(
        Modifier
            .fillMaxWidth()
            .background(
                if (!isFolder && node.path == activePath) {
                    MaterialTheme.colorScheme.primary.copy(alpha = 0.15f)
                } else {
                    Color.Transparent
                },
            )
            .clickable {
                if (isFolder) {
                    controller.toggleFolder(node.path)
                } else {
                    node.entry?.let { entry ->
                        if (entry.isNote) {
                            controller.openNote(entry.path)
                            controller.closeOverlays()
                        } else {
                            controller.openAttachment(entry.path)
                        }
                    }
                }
            }
            .padding(
                start = (8 + row.depth * 14).dp,
                end = 14.dp,
                top = 8.dp,
                bottom = 8.dp,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (isFolder) {
            if (hasChildren) {
                Icon(
                    if (collapsed) Icons.Default.KeyboardArrowRight else Icons.Default.KeyboardArrowDown,
                    contentDescription = if (collapsed) "Expand" else "Collapse",
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Spacer(Modifier.width(16.dp))
            }
            Icon(
                painter = painterResource(R.drawable.ic_folder),
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        } else {
            Icon(
                Icons.Default.List,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(
                    alpha = if (node.entry?.isNote == true || node.entry?.isCode == true) 1f else 0.55f,
                ),
            )
        }
        Spacer(Modifier.width(8.dp))
        Text(
            node.name,
            modifier = Modifier.weight(1f),
            fontSize = 14.sp,
            fontWeight = if (isFolder) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (!isFolder) {
            val entry = node.entry
            if (entry != null && !entry.isNote) {
                Text(
                    entry.extension.uppercase(),
                    fontSize = 9.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (entry != null && entry.isNote && entry.path in state.bookmarks) {
                Icon(
                    Icons.Default.Star,
                    contentDescription = "Bookmarked",
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
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
