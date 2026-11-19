package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Test

class MarkdownFormattingTest {
    @Test
    fun boldWrapsSelectionAndKeepsInnerTextSelected() {
        val edit = MarkdownFormatting.apply("hello world", 6, 11, FormattingAction.Bold)

        assertEquals(6, edit.start)
        assertEquals(11, edit.end)
        assertEquals("**world**", edit.replacement)
        assertEquals(2, edit.selectStart)
        assertEquals(7, edit.selectEnd)
    }

    @Test
    fun boldAtCaretInsertsMarkersWithCaretBetween() {
        val edit = MarkdownFormatting.apply("abc", 1, 1, FormattingAction.Bold)

        assertEquals(1, edit.start)
        assertEquals(1, edit.end)
        assertEquals("****", edit.replacement)
        assertEquals(2, edit.selectStart)
        assertEquals(2, edit.selectEnd)
    }

    @Test
    fun headingPrefixesEverySelectedLine() {
        val edit = MarkdownFormatting.apply("one\ntwo", 0, 7, FormattingAction.Heading2)

        assertEquals(0, edit.start)
        assertEquals(7, edit.end)
        assertEquals("## one\n## two", edit.replacement)
        assertEquals(13, edit.selectStart)
        assertEquals(13, edit.selectEnd)
    }

    @Test
    fun sameHeadingLevelTogglesItOff() {
        val edit = MarkdownFormatting.apply("# Title", 0, 7, FormattingAction.Heading1)

        assertEquals("Title", edit.replacement)
    }

    @Test
    fun anotherHeadingLevelReplacesTheOldMarker() {
        val edit = MarkdownFormatting.apply("## Title", 0, 8, FormattingAction.Heading3)

        assertEquals("### Title", edit.replacement)
    }

    @Test
    fun headingToggleKeepsIndentation() {
        val off = MarkdownFormatting.apply("  ## Title", 0, 10, FormattingAction.Heading2)
        assertEquals("  Title", off.replacement)

        val replaced = MarkdownFormatting.apply("  ## Title", 0, 10, FormattingAction.Heading1)
        assertEquals("  # Title", replaced.replacement)
    }

    @Test
    fun paragraphPrefixUsesOnlyEnclosingLines() {
        val text = "before\nitem\nafter"
        val edit = MarkdownFormatting.apply(text, 9, 9, FormattingAction.BulletList)

        assertEquals(7, edit.start)
        assertEquals(11, edit.end)
        assertEquals("- item", edit.replacement)
    }

    @Test
    fun offsetsCountCodePointsNotSurrogatePairs() {
        // The emoji is one Rust char / one code point but two UTF-16 units.
        val edit = MarkdownFormatting.apply("😀abc", 1, 4, FormattingAction.Italic)

        assertEquals(1, edit.start)
        assertEquals(4, edit.end)
        assertEquals("*abc*", edit.replacement)
        assertEquals(1, edit.selectStart)
        assertEquals(4, edit.selectEnd)
    }

    @Test
    fun linksWrapSelectionAndEmptyLinkPlacesCaretInside() {
        val wrapped = MarkdownFormatting.apply("note", 0, 4, FormattingAction.Link)
        assertEquals("[[note]]", wrapped.replacement)
        assertEquals(2, wrapped.selectStart)
        assertEquals(6, wrapped.selectEnd)

        val empty = MarkdownFormatting.apply("", 0, 0, FormattingAction.Link)
        assertEquals("[[]]", empty.replacement)
        assertEquals(2, empty.selectStart)
        assertEquals(2, empty.selectEnd)
    }

    @Test
    fun calloutQuotesSelectedLines() {
        val edit = MarkdownFormatting.apply("a\nb", 0, 3, FormattingAction.Callout)

        assertEquals("> [!NOTE]\n> a\n> b\n", edit.replacement)
    }

    @Test
    fun codeBlockWrapsSelectedTextAndSelectsIt() {
        val edit = MarkdownFormatting.apply("let x = 1;", 0, 10, FormattingAction.CodeBlock)

        assertEquals("```\nlet x = 1;\n```\n", edit.replacement)
        assertEquals(4, edit.selectStart)
        assertEquals(14, edit.selectEnd)
    }

    @Test
    fun tableInsertsTemplateAndLeavesCaretAtEnd() {
        val edit = MarkdownFormatting.apply("", 0, 0, FormattingAction.Table)

        assertEquals(0, edit.start)
        assertEquals(0, edit.end)
        assertEquals(true, edit.replacement.contains("| Column 1 | Column 2 | Column 3 |"))
        assertEquals(edit.replacement.codePointCount(0, edit.replacement.length), edit.selectEnd)
    }

    @Test
    fun mathWrapsSelectionWithoutLosingIt() {
        val edit = MarkdownFormatting.apply("x^2", 0, 3, FormattingAction.Math)

        assertEquals("\$\$x^2\$\$", edit.replacement)
        assertEquals(2, edit.selectStart)
        assertEquals(5, edit.selectEnd)
    }

    @Test
    fun reversedSelectionIsNormalized() {
        val edit = MarkdownFormatting.apply("hello", 4, 1, FormattingAction.Strikethrough)

        assertEquals(1, edit.start)
        assertEquals(4, edit.end)
        assertEquals("~~ell~~", edit.replacement)
    }
}
