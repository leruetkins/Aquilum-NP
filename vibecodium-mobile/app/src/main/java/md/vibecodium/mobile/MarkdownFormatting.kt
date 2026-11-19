package md.vibecodium.mobile

/**
 * Formatting commands offered by the selection toolbar menu.
 *
 * These mirror the desktop editor's right-click menu, which wraps the selection
 * in Markdown markers, prefixes the selected lines, or replaces the selection
 * with a template. Offsets follow the Rust editor's convention: one unit is one
 * Unicode code point, so emoji and other surrogate pairs count as one.
 */
enum class FormattingAction {
    Bold,
    Italic,
    Strikethrough,
    Highlight,
    InlineCode,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    BulletList,
    NumberedList,
    TaskList,
    Quote,
    Link,
    ExternalLink,
    Callout,
    CodeBlock,
    Table,
    Math,
    Footnote,
}

/**
 * A document replacement produced by [MarkdownFormatting].
 *
 * [start] and [end] are code-point offsets in the old text. The replacement is
 * written at that range and [selectStart]/[selectEnd] are code-point offsets
 * inside [replacement] for the caret or selection that should follow.
 */
data class FormatEdit(
    val start: Int,
    val end: Int,
    val replacement: String,
    val selectStart: Int,
    val selectEnd: Int,
)

object MarkdownFormatting {
    private const val TABLE_TEMPLATE =
        "\n| Column 1 | Column 2 | Column 3 |\n" +
            "| -------- | -------- | -------- |\n" +
            "| Item 1   | Item 2   | Item 3   |\n" +
            "| Item 4   | Item 5   | Item 6   |\n"

    fun apply(text: String, anchor: Int, selection: Int, action: FormattingAction): FormatEdit {
        val codePoints = toCodePoints(text)
        val length = codePoints.size
        val start = minOf(anchor, selection).coerceIn(0, length)
        val end = maxOf(anchor, selection).coerceIn(0, length)

        return when (action) {
            FormattingAction.Bold -> wrap(codePoints, start, end, "**", "**")
            FormattingAction.Italic -> wrap(codePoints, start, end, "*", "*")
            FormattingAction.Strikethrough -> wrap(codePoints, start, end, "~~", "~~")
            FormattingAction.Highlight -> wrap(codePoints, start, end, "==", "==")
            FormattingAction.InlineCode -> wrap(codePoints, start, end, "`", "`")
            FormattingAction.Heading1 -> setHeading(codePoints, start, end, 1)
            FormattingAction.Heading2 -> setHeading(codePoints, start, end, 2)
            FormattingAction.Heading3 -> setHeading(codePoints, start, end, 3)
            FormattingAction.Heading4 -> setHeading(codePoints, start, end, 4)
            FormattingAction.Heading5 -> setHeading(codePoints, start, end, 5)
            FormattingAction.Heading6 -> setHeading(codePoints, start, end, 6)
            FormattingAction.BulletList -> prefixLines(codePoints, start, end, "- ")
            FormattingAction.NumberedList -> prefixLines(codePoints, start, end, "1. ")
            FormattingAction.TaskList -> prefixLines(codePoints, start, end, "- [ ] ")
            FormattingAction.Quote -> prefixLines(codePoints, start, end, "> ")
            FormattingAction.Link -> wrapLink(codePoints, start, end)
            FormattingAction.ExternalLink -> externalLink(codePoints, start, end)
            FormattingAction.Callout -> callout(codePoints, start, end)
            FormattingAction.CodeBlock -> codeBlock(codePoints, start, end)
            FormattingAction.Table -> replaceWith(codePoints, start, end, TABLE_TEMPLATE)
            FormattingAction.Math -> wrap(codePoints, start, end, "$$", "$$")
            FormattingAction.Footnote -> replaceWith(codePoints, start, end, "[^1]")
        }
    }

    private fun wrap(codePoints: IntArray, start: Int, end: Int, open: String, close: String): FormatEdit {
        val inner = slice(codePoints, start, end)
        val replacement = open + inner + close
        val innerStart = codePointCount(open)
        return FormatEdit(
            start = start,
            end = end,
            replacement = replacement,
            selectStart = innerStart,
            selectEnd = innerStart + codePointCount(inner),
        )
    }

    private fun wrapLink(codePoints: IntArray, start: Int, end: Int): FormatEdit {
        val inner = slice(codePoints, start, end)
        val replacement = if (inner.isEmpty()) "[[]]" else "[[${inner}]]"
        return FormatEdit(
            start = start,
            end = end,
            replacement = replacement,
            selectStart = 2,
            selectEnd = 2 + codePointCount(inner),
        )
    }

    private fun externalLink(codePoints: IntArray, start: Int, end: Int): FormatEdit {
        val inner = slice(codePoints, start, end)
        val replacement = if (inner.isEmpty()) "[](url)" else "[$inner](url)"
        return FormatEdit(
            start = start,
            end = end,
            replacement = replacement,
            // Empty selection puts the caret between the brackets; a selection
            // keeps the link text selected so it can be edited.
            selectStart = 1,
            selectEnd = 1 + codePointCount(inner),
        )
    }

    private fun setHeading(codePoints: IntArray, start: Int, end: Int, level: Int): FormatEdit {
        var lineStart = start
        while (lineStart > 0 && codePoints[lineStart - 1] != '\n'.code) {
            lineStart -= 1
        }
        var lineEnd = end
        while (lineEnd < codePoints.size && codePoints[lineEnd] != '\n'.code) {
            lineEnd += 1
        }

        val targetPrefix = headingPrefix(level)
        val replacement = splitLines(codePoints, lineStart, lineEnd).joinToString("\n") { line ->
            val trimmed = line.trimStart()
            val indent = line.substring(0, line.length - trimmed.length)
            val current = detectHeadingLevel(trimmed)
            when {
                // Same level toggles the heading off.
                current == level -> indent + trimmed.substring(targetPrefix.length)
                // Another level replaces the old marker instead of stacking.
                current != null -> indent + targetPrefix + trimmed.substring(headingPrefix(current).length)
                else -> indent + targetPrefix + trimmed
            }
        }
        val cursor = codePointCount(replacement)
        return FormatEdit(
            start = lineStart,
            end = lineEnd,
            replacement = replacement,
            selectStart = cursor,
            selectEnd = cursor,
        )
    }

    private fun detectHeadingLevel(line: String): Int? {
        for (level in 6 downTo 1) {
            if (line.startsWith(headingPrefix(level))) return level
        }
        return null
    }

    private fun headingPrefix(level: Int): String = "#".repeat(level) + " "

    private fun prefixLines(codePoints: IntArray, start: Int, end: Int, prefix: String): FormatEdit {
        var lineStart = start
        while (lineStart > 0 && codePoints[lineStart - 1] != '\n'.code) {
            lineStart -= 1
        }
        var lineEnd = end
        while (lineEnd < codePoints.size && codePoints[lineEnd] != '\n'.code) {
            lineEnd += 1
        }

        val replacement = splitLines(codePoints, lineStart, lineEnd)
            .joinToString("\n") { prefix + it }
        val cursor = codePointCount(replacement)
        return FormatEdit(
            start = lineStart,
            end = lineEnd,
            replacement = replacement,
            selectStart = cursor,
            selectEnd = cursor,
        )
    }

    private fun callout(codePoints: IntArray, start: Int, end: Int): FormatEdit {
        val inner = slice(codePoints, start, end)
        if (inner.isEmpty()) {
            val prefix = "> [!NOTE]\n> "
            return FormatEdit(
                start = start,
                end = end,
                replacement = prefix + "\n",
                selectStart = codePointCount(prefix),
                selectEnd = codePointCount(prefix),
            )
        }
        val content = inner.split('\n').joinToString("\n") { "> $it" }
        val replacement = "> [!NOTE]\n$content\n"
        val cursor = codePointCount(replacement)
        return FormatEdit(
            start = start,
            end = end,
            replacement = replacement,
            selectStart = cursor,
            selectEnd = cursor,
        )
    }

    private fun codeBlock(codePoints: IntArray, start: Int, end: Int): FormatEdit {
        val inner = slice(codePoints, start, end)
        val atLineStart = start == 0 || codePoints[start - 1] == '\n'.code
        val atLineEnd = end == codePoints.size || codePoints[end] == '\n'.code
        // Mid-line selections get their own blank line around the fence, the
        // same spacing the desktop editor produces.
        val prefix = if (atLineStart) "" else "\n\n"
        val suffix = if (atLineEnd) "\n" else "\n\n"
        val replacement = prefix + "```\n" + inner + "\n```" + suffix
        val innerStart = codePointCount(prefix) + 4
        return FormatEdit(
            start = start,
            end = end,
            replacement = replacement,
            selectStart = innerStart,
            selectEnd = innerStart + codePointCount(inner),
        )
    }

    private fun replaceWith(codePoints: IntArray, start: Int, end: Int, template: String): FormatEdit {
        val cursor = codePointCount(template)
        return FormatEdit(
            start = start,
            end = end,
            replacement = template,
            selectStart = cursor,
            selectEnd = cursor,
        )
    }

    private fun splitLines(codePoints: IntArray, from: Int, to: Int): List<String> {
        val lines = ArrayList<String>()
        var lineStart = from
        var index = from
        while (index < to) {
            if (codePoints[index] == '\n'.code) {
                lines.add(slice(codePoints, lineStart, index))
                lineStart = index + 1
            }
            index += 1
        }
        lines.add(slice(codePoints, lineStart, to))
        return lines
    }

    private fun toCodePoints(text: String): IntArray {
        val result = ArrayList<Int>(text.length)
        var index = 0
        while (index < text.length) {
            val codePoint = text.codePointAt(index)
            result.add(codePoint)
            index += Character.charCount(codePoint)
        }
        return result.toIntArray()
    }

    private fun slice(codePoints: IntArray, from: Int, to: Int): String {
        val builder = StringBuilder(to - from)
        var index = from
        while (index < to) {
            builder.appendCodePoint(codePoints[index])
            index += 1
        }
        return builder.toString()
    }

    private fun codePointCount(text: String): Int = text.codePointCount(0, text.length)
}
