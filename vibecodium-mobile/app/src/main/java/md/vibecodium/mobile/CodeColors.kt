package md.vibecodium.mobile

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Colors for code blocks and inline code, mirroring Obsidian's code roles. */
data class CodeColors(
    val background: Color,
    val normal: Color,
    val comment: Color,
    val keyword: Color,
    val string: Color,
    val number: Color,
    val function: Color,
    val type: Color,
    val punctuation: Color,
) {
    fun color(kind: CodeTokenKind): Color = when (kind) {
        CodeTokenKind.COMMENT -> comment
        CodeTokenKind.KEYWORD -> keyword
        CodeTokenKind.STRING -> string
        CodeTokenKind.NUMBER -> number
        CodeTokenKind.FUNCTION -> function
        CodeTokenKind.TYPE -> type
        CodeTokenKind.PUNCTUATION -> punctuation
    }
}

val DarkCodeColors = CodeColors(
    background = Color(0xFF111111),
    normal = Color(0xFFDADADA),
    comment = Color(0xFF666666),
    keyword = Color(0xFFFA99CD),
    string = Color(0xFF44CF6E),
    number = Color(0xFFE9973F),
    function = Color(0xFFE0DE71),
    type = Color(0xFF53DFDD),
    punctuation = Color(0xFFB3B3B3),
)

val LocalCodeColors = staticCompositionLocalOf { DarkCodeColors }
