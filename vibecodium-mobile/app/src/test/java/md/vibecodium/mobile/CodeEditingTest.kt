package md.vibecodium.mobile

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import org.junit.Assert.assertEquals
import org.junit.Test

class CodeEditingTest {
    @Test
    fun tokenizerFindsRustRoles() {
        val code = "fn main() {\n    let x = 42; // answer\n    println!(\"{}\", x);\n}"
        val tokens = tokenizeCode(code, "rust")
        val byKind = tokens.groupBy { it.kind }

        assertEquals("fn", code.substring(byKind[CodeTokenKind.KEYWORD]!![0].start, byKind[CodeTokenKind.KEYWORD]!![0].end))
        assertEquals("main", code.substring(byKind[CodeTokenKind.FUNCTION]!![0].start, byKind[CodeTokenKind.FUNCTION]!![0].end))
        assertEquals("42", code.substring(byKind[CodeTokenKind.NUMBER]!![0].start, byKind[CodeTokenKind.NUMBER]!![0].end))
        assertEquals("\"{}\"", code.substring(byKind[CodeTokenKind.STRING]!![0].start, byKind[CodeTokenKind.STRING]!![0].end))
        assertEquals("// answer", code.substring(byKind[CodeTokenKind.COMMENT]!![0].start, byKind[CodeTokenKind.COMMENT]!![0].end))
    }

    @Test
    fun tokenizerHandlesPythonAndJson() {
        val python = tokenizeCode("def f(x): # note\n    return \"ok\"", "python")
        assertEquals(true, python.any { it.kind == CodeTokenKind.COMMENT })
        assertEquals(true, python.any { it.kind == CodeTokenKind.KEYWORD })
        assertEquals(true, python.any { it.kind == CodeTokenKind.STRING })

        val json = tokenizeCode("{\"a\": 1, \"ok\": true}", "json")
        assertEquals(2, json.count { it.kind == CodeTokenKind.STRING })
        assertEquals(1, json.count { it.kind == CodeTokenKind.NUMBER })
        assertEquals(1, json.count { it.kind == CodeTokenKind.KEYWORD })
    }

    @Test
    fun tabIndentsAndShiftTabOutdents() {
        val value = TextFieldValue("line", TextRange(0))
        val indented = indentAtCursor(value)
        assertEquals("    line", indented.text)
        assertEquals(4, indented.selection.start)

        val outdented = outdentAtCursor(indented)
        assertEquals("line", outdented.text)
        assertEquals(0, outdented.selection.start)

        val midLine = indentAtCursor(TextFieldValue("line", TextRange(4)))
        assertEquals("line    ", midLine.text)
        assertEquals(8, midLine.selection.start)
    }

    @Test
    fun autoIndentContinuesListsAndIndentation() {
        val list = autoIndent("- item", TextFieldValue("- item\n", TextRange(7)))
        assertEquals("- item\n- ", list.text)
        assertEquals(9, list.selection.start)

        val task = autoIndent("- [x] done", TextFieldValue("- [x] done\n", TextRange(11)))
        assertEquals("- [x] done\n- [ ] ", task.text)

        val indented = autoIndent("    code", TextFieldValue("    code\n", TextRange(9)))
        assertEquals("    code\n    ", indented.text)

        val plain = autoIndent("text", TextFieldValue("text\n", TextRange(5)))
        assertEquals("text\n", plain.text)
    }
}
