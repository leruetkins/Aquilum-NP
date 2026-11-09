package md.vibecodium.mobile.ui

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import md.vibecodium.mobile.CodeColors
import md.vibecodium.mobile.tokenizeCode

/** Full code block annotation for the preview. */
fun annotateCode(code: String, language: String, colors: CodeColors): AnnotatedString =
    buildAnnotatedString {
        append(code)
        for (token in tokenizeCode(code, language)) {
            addStyle(
                SpanStyle(
                    color = colors.color(token.kind),
                    fontFamily = FontFamily.Monospace,
                ),
                token.start,
                token.end,
            )
        }
    }

/** Syntax highlighting for a whole code file in the source editor. */
class CodeSourceHighlight(
    private val language: String,
    private val colors: CodeColors,
) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText =
        TransformedText(annotateCode(text.text, language, colors), OffsetMapping.Identity)
}

/** Adds syntax styles for one source-editor line, offset into the whole text. */
fun AnnotatedString.Builder.addCodeLineStyles(
    line: String,
    language: String,
    lineStart: Int,
    colors: CodeColors,
) {
    for (token in tokenizeCode(line, language)) {
        addStyle(
            SpanStyle(
                color = colors.color(token.kind),
                fontFamily = FontFamily.Monospace,
            ),
            lineStart + token.start,
            lineStart + token.end,
        )
    }
}
