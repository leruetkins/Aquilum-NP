package md.vibecodium.mobile.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import md.vibecodium.mobile.FormattingAction

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
)

private val FORMATTING_ENTRIES = listOf(
    FormattingEntry("B", "Bold", FormattingAction.Bold),
    FormattingEntry("I", "Italic", FormattingAction.Italic),
    FormattingEntry("S", "Strikethrough", FormattingAction.Strikethrough),
    FormattingEntry("H", "Highlight", FormattingAction.Highlight),
    FormattingEntry("</>", "Inline code", FormattingAction.InlineCode),
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
    FormattingEntry("[[ ]]", "Internal link", FormattingAction.Link),
    FormattingEntry("[ ]", "External link", FormattingAction.ExternalLink),
    FormattingEntry(">", "Callout", FormattingAction.Callout),
    FormattingEntry("```", "Code block", FormattingAction.CodeBlock),
    FormattingEntry("▦", "Table", FormattingAction.Table),
    FormattingEntry("∑", "Math block", FormattingAction.Math),
    FormattingEntry("[^1]", "Footnote", FormattingAction.Footnote),
)

private fun entriesFor(section: FormattingSection): List<FormattingEntry> = when (section) {
    FormattingSection.Formatting -> FORMATTING_ENTRIES
    FormattingSection.Paragraph -> PARAGRAPH_ENTRIES
    FormattingSection.Insert -> INSERT_ENTRIES
}

/**
 * Obsidian-style editor context menu: a rounded bottom sheet that holds the
 * main categories. Choosing one slides the same sheet's content sideways into
 * that category, with a "back" row at the top; Back returns to the categories
 * and then closes the sheet, like the desktop right-click menu's submenus.
 */
@Composable
internal fun FormattingMenuSheet(
    onDismiss: () -> Unit,
    onAction: (FormattingAction) -> Unit,
) {
    var section by remember { mutableStateOf<FormattingSection?>(null) }

    BackHandler(enabled = section != null) { section = null }
    BackHandler(enabled = section == null, onBack = onDismiss)

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.45f))
            .pointerInput(Unit) {
                detectTapGestures(onTap = { onDismiss() })
            },
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Surface(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .heightIn(max = maxHeight * 0.85f)
                    .pointerInput(Unit) {
                        // Taps on the sheet's padding must not fall through to
                        // the dismissing backdrop.
                        detectTapGestures(onTap = {})
                    },
                shape = RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp),
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 3.dp,
                shadowElevation = 12.dp,
            ) {
                Column(Modifier.fillMaxWidth()) {
                    Box(
                        Modifier
                            .align(Alignment.CenterHorizontally)
                            .padding(top = 8.dp, bottom = 6.dp)
                            .fillMaxWidth(0.15f)
                            .height(4.dp)
                            .clip(CircleShape)
                            .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f)),
                    )
                    AnimatedContent(
                        targetState = section,
                        transitionSpec = {
                            val enteringSubmenu = targetState != null
                            if (enteringSubmenu) {
                                slideInHorizontally(tween(220)) { it } togetherWith
                                    slideOutHorizontally(tween(220)) { -it }
                            } else {
                                slideInHorizontally(tween(220)) { -it } togetherWith
                                    slideOutHorizontally(tween(220)) { it }
                            }
                        },
                        label = "formatting-page",
                    ) { page ->
                        if (page == null) {
                            MainMenuPage(onSelect = { section = it })
                        } else {
                            SubmenuPage(
                                section = page,
                                onBack = { section = null },
                                onAction = onAction,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MainMenuPage(onSelect: (FormattingSection) -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(bottom = 6.dp),
    ) {
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
            .padding(bottom = 6.dp),
    ) {
        BackRow(title = section.title, onBack = onBack)
        MenuDivider()
        entriesFor(section).forEachIndexed { index, entry ->
            if (index > 0) {
                MenuDivider()
            }
            MenuRow(
                glyph = entry.glyph,
                title = entry.label,
                hasSubmenu = false,
                onClick = { onAction(entry.action) },
            )
        }
    }
}

@Composable
private fun MenuRow(
    glyph: String,
    title: String,
    hasSubmenu: Boolean,
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
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            glyph,
            modifier = Modifier.width(30.dp),
            fontSize = 13.sp,
            fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            title,
            modifier = Modifier.weight(1f),
            fontSize = 16.sp,
            color = MaterialTheme.colorScheme.onSurface,
        )
        if (hasSubmenu) {
            Icon(
                Icons.Default.KeyboardArrowRight,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
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
            .heightIn(min = 48.dp)
            .padding(start = 12.dp, end = 16.dp),
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
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

@Composable
private fun MenuDivider() {
    HorizontalDivider(
        modifier = Modifier.padding(start = 16.dp),
        color = MaterialTheme.colorScheme.outline.copy(alpha = 0.35f),
    )
}
