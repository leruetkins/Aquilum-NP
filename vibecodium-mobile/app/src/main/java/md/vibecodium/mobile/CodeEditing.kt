package md.vibecodium.mobile

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue

private const val INDENT = "    "

/** Tab: insert four spaces, or indent every selected line. */
fun indentAtCursor(value: TextFieldValue): TextFieldValue =
    indentSelection(value, outdent = false)

/** Shift+Tab: remove up to four leading spaces, or outdent selected lines. */
fun outdentAtCursor(value: TextFieldValue): TextFieldValue =
    indentSelection(value, outdent = true)

private fun indentSelection(value: TextFieldValue, outdent: Boolean): TextFieldValue {
    val text = value.text
    val start = minOf(value.selection.start, value.selection.end)
    val end = maxOf(value.selection.start, value.selection.end)

    if (start == end) {
        if (!outdent) {
            return TextFieldValue(
                text = text.substring(0, start) + INDENT + text.substring(start),
                selection = TextRange(start + INDENT.length),
            )
        }
        val lineStart = text.lastIndexOf('\n', start - 1).let { if (it < 0) 0 else it + 1 }
        val lineEnd = text.indexOf('\n', start).let { if (it < 0) text.length else it }
        val leading = text.substring(lineStart, lineEnd).takeWhile { it == ' ' || it == '\t' }
        val remove = if (leading.startsWith("\t")) 1 else minOf(4, leading.length)
        if (remove == 0) return value
        val newText = text.removeRange(lineStart, lineStart + remove)
        return TextFieldValue(
            text = newText,
            selection = TextRange((start - remove).coerceAtLeast(lineStart)),
        )
    }

    val firstLineStart = text.lastIndexOf('\n', start - 1).let { if (it < 0) 0 else it + 1 }
    val lastLineEnd = text.indexOf('\n', end).let { if (it < 0) text.length else it }
    val lines = text.substring(firstLineStart, lastLineEnd).split('\n')
    var firstShift = 0
    var totalShift = 0

    val newLines = lines.mapIndexed { index, line ->
        if (!outdent) {
            if (index == 0) firstShift = INDENT.length
            totalShift += INDENT.length
            INDENT + line
        } else {
            val trimmed = line.trimStart(' ', '\t')
            val remove = when {
                line.startsWith("\t") -> 1
                else -> minOf(4, line.length - trimmed.length)
            }
            if (index == 0) firstShift = -remove
            totalShift -= remove
            line.substring(remove)
        }
    }

    val newText = text.substring(0, firstLineStart) + newLines.joinToString("\n") + text.substring(lastLineEnd)
    return TextFieldValue(
        text = newText,
        selection = TextRange(
            start = (start + firstShift).coerceIn(0, newText.length),
            end = (end + totalShift).coerceIn(0, newText.length),
        ),
    )
}

/**
 * Continues list markers and indentation after Enter. It only reacts to a
 * single inserted newline, so ordinary edits and pastes are untouched.
 */
fun autoIndent(previousText: String, value: TextFieldValue): TextFieldValue {
    val text = value.text
    if (text.length != previousText.length + 1) return value
    val cursor = value.selection.start
    if (!value.selection.collapsed || cursor <= 0 || cursor > text.length) return value
    if (text[cursor - 1] != '\n') return value

    val previousLineEnd = cursor - 1
    val previousLineStart = text.lastIndexOf('\n', previousLineEnd - 1).let { if (it < 0) 0 else it + 1 }
    val previousLine = text.substring(previousLineStart, previousLineEnd)
    val indent = previousLine.takeWhile { it == ' ' || it == '\t' }
    val trimmed = previousLine.trimStart()
    val marker = when {
        trimmed.startsWith("- [ ] ") || trimmed.startsWith("- [x] ") ||
            trimmed.startsWith("- [X] ") -> "- [ ] "
        trimmed.startsWith("- ") -> "- "
        trimmed.startsWith("* ") -> "* "
        trimmed.startsWith("+ ") -> "+ "
        else -> null
    }

    val insertion = indent + (marker ?: "")
    if (insertion.isEmpty()) return value
    return TextFieldValue(
        text = text.substring(0, cursor) + insertion + text.substring(cursor),
        selection = TextRange(cursor + insertion.length),
    )
}
