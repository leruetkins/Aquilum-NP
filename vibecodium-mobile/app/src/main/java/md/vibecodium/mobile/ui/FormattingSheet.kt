package md.vibecodium.mobile.ui

import android.graphics.RectF
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.round
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import md.vibecodium.mobile.FormattingAction
import md.vibecodium.mobile.SelectionToolbarState
import kotlin.math.roundToInt

/** The three groups of the desktop editor's right-click menu. */
internal enum class FormattingSection(val title: String) {
    Formatting("Formatting"),
    Paragraph("Paragraph"),
    Insert("Insert"),
}

private data class FormattingEntry(
    val glyph: String,
    val label: String,
    val action: FormattingAction,
    val icon: ImageVector? = null,
)

private fun menuIcon(name: String, path: String): ImageVector =
    ImageVector.Builder(
        name = name,
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).addPath(
        pathData = addPathNodes(path),
        fill = SolidColor(Color.Black),
    ).build()

private val LinkIcon = menuIcon(
    "link",
    "M3.9,12c0,-1.71 1.39,-3.1 3.1,-3.1h4L11,7L7,7c-2.76,0 -5,2.24 -5,5s2.24,5 5,5h4v-1.9L7,15.1c-1.71,0 -3.1,-1.39 -3.1,-3.1zM8,13h8v-2L8,11v2zM17,7h-4v1.9h4c1.71,0 3.1,1.39 3.1,3.1s-1.39,3.1 -3.1,3.1h-4L13,17h4c2.76,0 5,-2.24 5,-5s-2.24,-5 -5,-5z",
)
private val ExternalLinkIcon = menuIcon(
    "external-link",
    "M19,19L5,19L5,5h7L12,3L5,3c-1.11,0 -2,0.9 -2,2v14c0,1.1 0.89,2 2,2h14c1.1,0 2,-0.9 2,-2v-7h-2v7zM14,3v2h3.59l-9.83,9.83 1.41,1.41L19,6.41L19,10h2L21,3h-7z",
)
private val CodeBlockIcon = menuIcon(
    "code-block",
    "M9.4,16.6L4.8,12l4.6,-4.6L8,6l-6,6 6,6 1.4,-1.4zM14.6,16.6l4.6,-4.6 -4.6,-4.6L16,6l6,6 -6,6 -1.4,-1.4z",
)
private val FootnoteIcon = menuIcon(
    "footnote",
    "M19,3L5,3c-1.1,0 -2,0.9 -2,2v14c0,1.1 0.9,2 2,2h14c1.1,0 2,-0.9 2,-2L21,5c0,-1.1 -0.9,-2 -2,-2zM8,17L6,17v-2h2v2zM8,13L6,13v-2h2v2zM8,9L6,9L6,7h2v2zM18,17L10,17v-2h8v2zM18,13L10,13v-2h8v2zM18,9L10,9L10,7h8v2z",
)

private val FORMATTING_ENTRIES = listOf(
    FormattingEntry("B", "Bold", FormattingAction.Bold),
    FormattingEntry("I", "Italic", FormattingAction.Italic),
    FormattingEntry("S", "Strikethrough", FormattingAction.Strikethrough),
    FormattingEntry("H", "Highlight", FormattingAction.Highlight),
    FormattingEntry("", "Inline code", FormattingAction.InlineCode, CodeBlockIcon),
)

private val PARAGRAPH_ENTRIES = listOf(
    FormattingEntry("H1", "Heading 1", FormattingAction.Heading1),
    FormattingEntry("H2", "Heading 2", FormattingAction.Heading2),
    FormattingEntry("H3", "Heading 3", FormattingAction.Heading3),
    FormattingEntry("H4", "Heading 4", FormattingAction.Heading4),
    FormattingEntry("H5", "Heading 5", FormattingAction.Heading5),
    FormattingEntry("H6", "Heading 6", FormattingAction.Heading6),
    FormattingEntry("•", "Bullet list", FormattingAction.BulletList),
    FormattingEntry("1.", "Numbered list", FormattingAction.NumberedList),
    FormattingEntry("☐", "Task list", FormattingAction.TaskList),
    FormattingEntry("❯", "Quote", FormattingAction.Quote),
)

private val INSERT_ENTRIES = listOf(
    FormattingEntry("", "Internal link", FormattingAction.Link, LinkIcon),
    FormattingEntry("", "External link", FormattingAction.ExternalLink, ExternalLinkIcon),
    FormattingEntry(">", "Callout", FormattingAction.Callout),
    FormattingEntry("", "Code block", FormattingAction.CodeBlock, CodeBlockIcon),
    FormattingEntry("▦", "Table", FormattingAction.Table),
    FormattingEntry("∑", "Math block", FormattingAction.Math),
    FormattingEntry("", "Footnote", FormattingAction.Footnote, FootnoteIcon),
)

private fun entriesFor(section: FormattingSection): List<FormattingEntry> = when (section) {
    FormattingSection.Formatting -> FORMATTING_ENTRIES
    FormattingSection.Paragraph -> PARAGRAPH_ENTRIES
    FormattingSection.Insert -> INSERT_ENTRIES
}

private val MenuWidth = 344.dp
private val ToolbarHeight = 52.dp
private val MenuRowHeight = 50.dp
private val BackRowHeight = 48.dp

/**
 * One Compose surface for both the selection toolbar and the caret context
 * menu. Collapsed it is the `Cut / Copy / Paste / All / ⋮` bar above the
 * selection; tapping ⋮ morphs the same surface into the context menu under
 * the caret, so navigation feels like one continuous menu instead of two
 * system windows.
 */
@Composable
internal fun EditorContextOverlay(
    state: MutableState<SelectionToolbarState?>,
    topBoundaryPx: Float,
    onDismiss: () -> Unit,
    onAction: (FormattingAction) -> Unit,
    onCut: () -> Unit,
    onCopy: () -> Unit,
    onPaste: () -> Unit,
    onSelectAll: () -> Unit,
) {
    // No persistent popup: a zero-size window left a phantom surface on some
    // devices, so the window is destroyed with the selection.
    val current = state.value ?: return

    // Local to the menu: expanding must not recompose the editor screen.
    var expanded by remember { mutableStateOf(false) }
    var section by remember { mutableStateOf<FormattingSection?>(null) }
    LaunchedEffect(expanded) {
        if (!expanded) section = null
    }

    BackHandler(enabled = true) {
        when {
            expanded && section != null -> section = null
            expanded -> expanded = false
            else -> onDismiss()
        }
    }

    // Measure the editor container's window origin: topBoundaryPx is relative
    // to it, while Popup positions are in window coordinates.
    var originY by remember { mutableStateOf<Int?>(null) }
    Box(
        Modifier
            .size(0.dp)
            .onGloballyPositioned { originY = it.positionInWindow().round().y },
    )
    val origin = originY ?: return

    val density = LocalDensity.current
    val gapPx = with(density) { 6.dp.roundToPx() }
    val edgePx = with(density) { 8.dp.roundToPx() }
    val minTop = maxOf(edgePx, origin + topBoundaryPx.roundToInt())
    val maxHeight = (LocalConfiguration.current.screenHeightDp * 0.7f).dp
    val anchor = current.anchor
    val provider = remember(anchor, gapPx, edgePx, minTop) {
        ContextMenuPositionProvider(anchor, gapPx, edgePx, minTop)
    }

    // Separate window: touching/animating the menu does not force the main
    // window (and the native editor's TextureView) to redraw, which is what
    // made each panel update cost ~100 ms.
    Popup(
        popupPositionProvider = provider,
        onDismissRequest = onDismiss,
        properties = PopupProperties(
            focusable = true,
            dismissOnClickOutside = true,
            dismissOnBackPress = false,
            clippingEnabled = false,
        ),
    ) {
        Surface(
            modifier = Modifier
                .width(MenuWidth)
                .heightIn(max = maxHeight)
                .pointerInput(Unit) { detectTapGestures(onTap = { onDismiss() }) },
            shape = RoundedCornerShape(if (expanded) 16.dp else 22.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 3.dp,
            border = BorderStroke(
                1.dp,
                MaterialTheme.colorScheme.outline.copy(alpha = 0.3f),
            ),
        ) {
            if (expanded) {
                val page = section
                if (page == null) {
                    MenuRootPage(
                        onBack = { expanded = false },
                        onSelect = { section = it },
                    )
                } else {
                    SubmenuPage(
                        section = page,
                        onBack = { section = null },
                        onAction = onAction,
                    )
                }
            } else {
                ToolbarPage(
                    state = current,
                    onCut = onCut,
                    onCopy = {
                        onCopy()
                        onDismiss()
                    },
                    onPaste = onPaste,
                    onSelectAll = {
                        onSelectAll()
                        onDismiss()
                    },
                    onMore = { expanded = true },
                )
            }
        }
    }
}

/** Places the popup above or below the caret depending on the room available. */
private class ContextMenuPositionProvider(
    private val anchor: RectF,
    private val gapPx: Int,
    private val edgePx: Int,
    private val minTop: Int,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val width = popupContentSize.width
        val height = popupContentSize.height
        val rightEdge = windowSize.width - edgePx
        val bottomEdge = windowSize.height - edgePx
        val spaceAbove = (anchor.top - gapPx - minTop).coerceAtLeast(0f)
        val spaceBelow = (bottomEdge - anchor.bottom - gapPx).coerceAtLeast(0f)
        val placeAbove = when {
            height <= spaceAbove -> true
            height <= spaceBelow -> false
            else -> spaceAbove > spaceBelow
        }
        var x = (anchor.left + anchor.right) / 2f - width / 2f
        var y = if (placeAbove) anchor.top - gapPx - height else anchor.bottom + gapPx
        x = x.coerceIn(edgePx.toFloat(), (rightEdge - width).coerceAtLeast(edgePx).toFloat())
        y = y.coerceIn(minTop.toFloat(), (bottomEdge - height).coerceAtLeast(minTop).toFloat())
        return IntOffset(x.roundToInt(), y.roundToInt())
    }
}

@Composable
private fun ToolbarPage(
    state: SelectionToolbarState,
    onCut: () -> Unit,
    onCopy: () -> Unit,
    onPaste: () -> Unit,
    onSelectAll: () -> Unit,
    onMore: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().height(ToolbarHeight)) {
        ToolbarButton("Cut", 1f, state.canCut, onCut)
        ToolbarButton("Copy", 1f, state.canCopy, onCopy)
        ToolbarButton("Paste", 1f, state.canPaste, onPaste)
        ToolbarButton("Select all", 1.7f, true, onSelectAll)
        ToolbarButton("⋮", 0.7f, true, onMore, icon = true)
    }
}

@Composable
private fun RowScope.ToolbarButton(
    label: String,
    weight: Float,
    enabled: Boolean,
    onClick: () -> Unit,
    icon: Boolean = false,
) {
    val color = if (enabled) {
        MaterialTheme.colorScheme.onSurface
    } else {
        MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
    }
    Box(
        Modifier
            .weight(weight)
            .fillMaxHeight()
            .clickable(
                enabled = enabled,
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
            )
            .semantics { contentDescription = if (icon) "More" else label },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = if (icon) 18.sp else 13.sp,
            fontFamily = if (icon) FontFamily.Monospace else null,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            color = color,
        )
    }
}

@Composable
private fun MenuRootPage(
    onBack: () -> Unit,
    onSelect: (FormattingSection) -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(vertical = 4.dp),
    ) {
        BackRow(title = "Back", onBack = onBack)
        MenuDivider()
        MenuRow(
            glyph = "B",
            title = FormattingSection.Formatting.title,
            hasSubmenu = true,
        ) { onSelect(FormattingSection.Formatting) }
        MenuDivider()
        MenuRow(
            glyph = "¶",
            title = FormattingSection.Paragraph.title,
            hasSubmenu = true,
        ) { onSelect(FormattingSection.Paragraph) }
        MenuDivider()
        MenuRow(
            glyph = "+",
            title = FormattingSection.Insert.title,
            hasSubmenu = true,
        ) { onSelect(FormattingSection.Insert) }
    }
}

@Composable
private fun SubmenuPage(
    section: FormattingSection,
    onBack: () -> Unit,
    onAction: (FormattingAction) -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(vertical = 4.dp),
    ) {
        BackRow(title = section.title, onBack = onBack)
        entriesFor(section).forEach { entry ->
            MenuDivider()
            MenuRow(
                glyph = entry.glyph,
                icon = entry.icon,
                title = entry.label,
                onClick = { onAction(entry.action) },
            )
        }
    }
}

@Composable
private fun MenuRow(
    glyph: String,
    title: String,
    hasSubmenu: Boolean = false,
    icon: ImageVector? = null,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick,
            )
            .heightIn(min = MenuRowHeight)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) {
            if (icon != null) {
                Icon(
                    icon,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                )
            } else {
                Text(
                    glyph,
                    fontSize = 13.sp,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    softWrap = false,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
                )
            }
        }
        Text(
            title,
            modifier = Modifier.weight(1f),
            fontSize = 15.sp,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            color = MaterialTheme.colorScheme.onSurface,
        )
        if (hasSubmenu) {
            Icon(
                Icons.Default.KeyboardArrowRight,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.72f),
            )
        }
    }
}

@Composable
private fun BackRow(title: String, onBack: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onBack,
            )
            .heightIn(min = BackRowHeight)
            .padding(start = 10.dp, end = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Default.KeyboardArrowLeft,
            contentDescription = "Back",
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(4.dp))
        Text(
            title,
            fontSize = 15.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun MenuDivider() {
    HorizontalDivider(
        modifier = Modifier.padding(horizontal = 10.dp),
        color = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f),
    )
}
