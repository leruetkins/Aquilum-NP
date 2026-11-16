package md.vibecodium.mobile

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.json.JSONObject

/** Base color scheme selected in Settings → Appearance. */
enum class ThemeMode { System, Dark, Light }

/** Independent theme slots; changing the base scheme picks one of these. */
enum class ThemeSlot { Light, Dark }

data class ThemeManifest(
    val name: String,
    val version: String,
    val minAppVersion: String? = null,
    val author: String? = null,
    val authorUrl: String? = null,
)

/** One row of `community-css-themes.json`, plus local install state. */
data class ThemeEntry(
    val name: String,
    val author: String = "",
    val repo: String = "",
    val screenshot: String = "",
    val modes: List<String> = emptyList(),
    val legacy: Boolean = false,
    val downloads: Int = 0,
    val installedVersion: String? = null,
    val updateVersion: String? = null,
) {
    val installed: Boolean get() = installedVersion != null
    val hasUpdate: Boolean get() = updateVersion != null && updateVersion != installedVersion
    val modesLabel: String
        get() = when {
            modes.contains("dark") && modes.contains("light") -> "Dark & light"
            modes.contains("dark") -> "Dark"
            modes.contains("light") -> "Light"
            else -> ""
        }
}

data class InstalledTheme(
    val name: String,
    val cssFile: java.io.File,
    val manifest: ThemeManifest?,
    val entry: ThemeEntry? = null,
    val legacy: Boolean = false,
) {
    val version: String get() = manifest?.version ?: "0.0.0"
    val author: String get() = entry?.author ?: manifest?.author.orEmpty()
}

/**
 * Resolved Obsidian theme tokens. The same object feeds Compose (Material
 * color scheme, code colors, editor preview) and the native surface through
 * [toNativeJson]. Sizes are already converted to dp/sp; `fontText` and friends
 * keep the CSS family list so the UI can pick the first available Typeface.
 */
data class ThemePalette(
    val dark: Boolean,
    val background: Color,
    val backgroundAlt: Color,
    val surface: Color,
    val surfaceAlt: Color,
    val text: Color,
    val textMuted: Color,
    val textFaint: Color,
    val accent: Color,
    val accentHover: Color,
    val onAccent: Color,
    val error: Color,
    val outline: Color,
    val selection: Color,
    /** Obsidian's `--background-modifier-cover`, used behind drawers. */
    val overlay: Color,
    val codeBackground: Color,
    val codeNormal: Color,
    val codeComment: Color,
    val codeKeyword: Color,
    val codeString: Color,
    val codeNumber: Color,
    val codeFunction: Color,
    val codeType: Color,
    val codePunctuation: Color,
    val heading: Color,
    val marker: Color,
    val link: Color,
    val tag: Color,
    val inlineCode: Color,
    val inlineCodeBackground: Color,
    val highlight: Color,
    val titlebar: Color,
    val statusBar: Color,
    val navText: Color,
    val navMuted: Color,
    val fileLineWidth: Dp,
    val lineHeight: Float,
    val radiusSmall: Dp,
    val radiusMedium: Dp,
    val fontSize: Float,
    val h1Size: Float,
    val h2Size: Float,
    val h3Size: Float,
    val h4Size: Float,
    val h5Size: Float,
    val h6Size: Float,
    val fontText: String,
    val fontInterface: String,
    val fontMonospace: String,
) {
    val codeColors: CodeColors
        get() = CodeColors(
            background = codeBackground,
            normal = codeNormal,
            comment = codeComment,
            keyword = codeKeyword,
            string = codeString,
            number = codeNumber,
            function = codeFunction,
            type = codeType,
            punctuation = codePunctuation,
        )

    /** JSON contract consumed by `CoreBridge.nativeThemeSetPalette`. */
    fun toNativeJson(): String = JSONObject()
        .put("background", background.rgb())
        .put("surface", surface.rgb())
        .put("surfaceAlt", surfaceAlt.rgb())
        .put("text", text.rgb())
        .put("textMuted", textMuted.rgb())
        .put("textFaint", textFaint.rgb())
        .put("accent", accent.rgb())
        .put("accentHover", accentHover.rgb())
        .put("border", outline.rgb())
        .put("selection", selection.blendOver(background).rgb())
        .put("codeBackground", codeBackground.rgb())
        .put("codeNormal", codeNormal.rgb())
        .put("codeComment", codeComment.rgb())
        .put("codeKeyword", codeKeyword.rgb())
        .put("codeString", codeString.rgb())
        .put("codeNumber", codeNumber.rgb())
        .put("codeFunction", codeFunction.rgb())
        .put("codeType", codeType.rgb())
        .put("codePunctuation", codePunctuation.rgb())
        .put("heading", heading.rgb())
        .put("marker", marker.rgb())
        .put("link", link.rgb())
        .put("tag", tag.rgb())
        .put("inlineCode", inlineCode.rgb())
        .put("inlineCodeBackground", inlineCodeBackground.blendOver(codeBackground).rgb())
        .put("highlight", highlight.blendOver(background).rgb())
        .put("treeBackground", surfaceAlt.rgb())
        .put("treeText", text.rgb())
        .put("treeMuted", textMuted.rgb())
        .put("treeFolder", accent.rgb())
        .put("activeRow", surfaceAlt.lighten(0.08f).rgb())
        .put("tagBackground", surface.rgb())
        .put("editorBackground", background.rgb())
        .put("scrollbar", textFaint.rgb())
        .toString()

    /** Full token snapshot for the warm-start cache (unlike the native RGB JSON). */
    fun toCacheJson(): JSONObject = JSONObject()
        .put("dark", dark)
        .put("background", background.toArgb())
        .put("backgroundAlt", backgroundAlt.toArgb())
        .put("surface", surface.toArgb())
        .put("surfaceAlt", surfaceAlt.toArgb())
        .put("text", text.toArgb())
        .put("textMuted", textMuted.toArgb())
        .put("textFaint", textFaint.toArgb())
        .put("accent", accent.toArgb())
        .put("accentHover", accentHover.toArgb())
        .put("onAccent", onAccent.toArgb())
        .put("error", error.toArgb())
        .put("outline", outline.toArgb())
        .put("selection", selection.toArgb())
        .put("overlay", overlay.toArgb())
        .put("codeBackground", codeBackground.toArgb())
        .put("codeNormal", codeNormal.toArgb())
        .put("codeComment", codeComment.toArgb())
        .put("codeKeyword", codeKeyword.toArgb())
        .put("codeString", codeString.toArgb())
        .put("codeNumber", codeNumber.toArgb())
        .put("codeFunction", codeFunction.toArgb())
        .put("codeType", codeType.toArgb())
        .put("codePunctuation", codePunctuation.toArgb())
        .put("heading", heading.toArgb())
        .put("marker", marker.toArgb())
        .put("link", link.toArgb())
        .put("tag", tag.toArgb())
        .put("inlineCode", inlineCode.toArgb())
        .put("inlineCodeBackground", inlineCodeBackground.toArgb())
        .put("highlight", highlight.toArgb())
        .put("titlebar", titlebar.toArgb())
        .put("statusBar", statusBar.toArgb())
        .put("navText", navText.toArgb())
        .put("navMuted", navMuted.toArgb())
        .put("fileLineWidth", fileLineWidth.value)
        .put("lineHeight", lineHeight)
        .put("radiusSmall", radiusSmall.value)
        .put("radiusMedium", radiusMedium.value)
        .put("fontSize", fontSize)
        .put("h1Size", h1Size)
        .put("h2Size", h2Size)
        .put("h3Size", h3Size)
        .put("h4Size", h4Size)
        .put("h5Size", h5Size)
        .put("h6Size", h6Size)
        .put("fontText", fontText)
        .put("fontInterface", fontInterface)
        .put("fontMonospace", fontMonospace)

    private fun Color.rgb(): Int = toArgb() and 0xFFFFFF

    /** The native painter draws opaque rects; flatten theme alpha first. */
    private fun Color.blendOver(background: Color): Color {
        val a = alpha
        if (a >= 0.999f) return this
        return Color(
            red = red * a + background.red * (1f - a),
            green = green * a + background.green * (1f - a),
            blue = blue * a + background.blue * (1f - a),
            alpha = 1f,
        )
    }

    private fun Color.lighten(delta: Float): Color = Color(
        red = (red + delta).coerceAtMost(1f),
        green = (green + delta).coerceAtMost(1f),
        blue = (blue + delta).coerceAtMost(1f),
        alpha = alpha,
    )

    /** Accent chosen in Settings overrides the theme's accent tokens. */
    fun withAccent(argb: Int?): ThemePalette {
        if (argb == null) return this
        val accent = Color(argb)
        val hover = lighten(accent)
        return copy(
            accent = accent,
            accentHover = hover,
            onAccent = if (contrast(accent) > 0.55) Color.Black else Color.White,
            link = accent,
            tag = accent,
        )
    }

    companion object {
        fun fromCacheJson(json: JSONObject): ThemePalette? = runCatching {
            fun color(name: String): Color = Color(json.getInt(name))
            fun dimension(name: String): Dp = json.optDouble(name, 0.0).toFloat().dp
            ThemePalette(
                dark = json.optBoolean("dark"),
                background = color("background"),
                backgroundAlt = color("backgroundAlt"),
                surface = color("surface"),
                surfaceAlt = color("surfaceAlt"),
                text = color("text"),
                textMuted = color("textMuted"),
                textFaint = color("textFaint"),
                accent = color("accent"),
                accentHover = color("accentHover"),
                onAccent = color("onAccent"),
                error = color("error"),
                outline = color("outline"),
                selection = color("selection"),
                overlay = color("overlay"),
                codeBackground = color("codeBackground"),
                codeNormal = color("codeNormal"),
                codeComment = color("codeComment"),
                codeKeyword = color("codeKeyword"),
                codeString = color("codeString"),
                codeNumber = color("codeNumber"),
                codeFunction = color("codeFunction"),
                codeType = color("codeType"),
                codePunctuation = color("codePunctuation"),
                heading = color("heading"),
                marker = color("marker"),
                link = color("link"),
                tag = color("tag"),
                inlineCode = color("inlineCode"),
                inlineCodeBackground = color("inlineCodeBackground"),
                highlight = color("highlight"),
                titlebar = color("titlebar"),
                statusBar = color("statusBar"),
                navText = color("navText"),
                navMuted = color("navMuted"),
                fileLineWidth = dimension("fileLineWidth"),
                lineHeight = json.optDouble("lineHeight", 1.5).toFloat(),
                radiusSmall = dimension("radiusSmall"),
                radiusMedium = dimension("radiusMedium"),
                fontSize = json.optDouble("fontSize", 16.0).toFloat(),
                h1Size = json.optDouble("h1Size", 25.9).toFloat(),
                h2Size = json.optDouble("h2Size", 23.4).toFloat(),
                h3Size = json.optDouble("h3Size", 21.1).toFloat(),
                h4Size = json.optDouble("h4Size", 19.0).toFloat(),
                h5Size = json.optDouble("h5Size", 17.2).toFloat(),
                h6Size = json.optDouble("h6Size", 16.0).toFloat(),
                fontText = json.optString("fontText", "sans-serif"),
                fontInterface = json.optString("fontInterface", "sans-serif"),
                fontMonospace = json.optString("fontMonospace", "monospace"),
            )
        }.getOrNull()

        private fun lighten(color: Color): Color {
            val delta = 0.14f
            return Color(
                red = (color.red + delta).coerceAtMost(1f),
                green = (color.green + delta).coerceAtMost(1f),
                blue = (color.blue + delta).coerceAtMost(1f),
                alpha = color.alpha,
            )
        }

        private fun contrast(color: Color): Double {
            val luminance = 0.2126 * color.red + 0.7152 * color.green + 0.0722 * color.blue
            return luminance.toDouble()
        }

        private fun createDefaultDark(): ThemePalette = ThemePalette(
            dark = true,
            background = Color(0xFF000000),
            backgroundAlt = Color(0xFF111111),
            surface = Color(0xFF1E1E1E),
            surfaceAlt = Color(0xFF282828),
            text = Color(0xFFDADADA),
            textMuted = Color(0xFFB3B3B3),
            textFaint = Color(0xFF666666),
            accent = Color(0xFF8A5CF5),
            accentHover = Color(0xFFA68AF9),
            onAccent = Color.White,
            error = Color(0xFFFB464C),
            outline = Color(0xFF333333),
            selection = Color(0x4D8A5CF5),
            overlay = Color(0x66000000),
            codeBackground = Color(0xFF111111),
            codeNormal = Color(0xFFDADADA),
            codeComment = Color(0xFF666666),
            codeKeyword = Color(0xFFFA99CD),
            codeString = Color(0xFF44CF6E),
            codeNumber = Color(0xFFE9973F),
            codeFunction = Color(0xFFE0DE71),
            codeType = Color(0xFF53DFDD),
            codePunctuation = Color(0xFFB3B3B3),
            heading = Color(0xFFDADADA),
            marker = Color(0xFF666666),
            link = Color(0xFF8A5CF5),
            tag = Color(0xFF8A5CF5),
            inlineCode = Color(0xFF53DFDD),
            inlineCodeBackground = Color(0xFF111111),
            highlight = Color(0xFF2A2140),
            titlebar = Color(0xFF1E1E1E),
            statusBar = Color(0xFF1E1E1E),
            navText = Color(0xFFDADADA),
            navMuted = Color(0xFFB3B3B3),
            fileLineWidth = 700.dp,
            lineHeight = 1.5f,
            radiusSmall = 4.dp,
            radiusMedium = 8.dp,
            fontSize = 16f,
            h1Size = 1.618f * 16f,
            h2Size = 1.462f * 16f,
            h3Size = 1.318f * 16f,
            h4Size = 1.196f * 16f,
            h5Size = 1.091f * 16f,
            h6Size = 1f * 16f,
            fontText = "sans-serif",
            fontInterface = "sans-serif",
            fontMonospace = "monospace",
        )

        private fun createDefaultLight(): ThemePalette = ThemePalette(
            dark = false,
            background = Color(0xFFFFFFFF),
            backgroundAlt = Color(0xFFFAFAFA),
            surface = Color(0xFFF6F6F6),
            surfaceAlt = Color(0xFFFCFCFC),
            text = Color(0xFF222222),
            textMuted = Color(0xFF5C5C5C),
            textFaint = Color(0xFFABABAB),
            accent = Color(0xFF7852EE),
            accentHover = Color(0xFF8A5CF5),
            onAccent = Color.White,
            error = Color(0xFFE93147),
            outline = Color(0xFFE4E4E4),
            selection = Color(0x4D7852EE),
            overlay = Color(0x66DCDCDC),
            codeBackground = Color(0xFFFAFAFA),
            codeNormal = Color(0xFF222222),
            codeComment = Color(0xFF707070),
            codeKeyword = Color(0xFFD53984),
            codeString = Color(0xFF08B94E),
            codeNumber = Color(0xFFEC7500),
            codeFunction = Color(0xFF7852EE),
            codeType = Color(0xFF00BFBC),
            codePunctuation = Color(0xFF5C5C5C),
            heading = Color(0xFF222222),
            marker = Color(0xFFABABAB),
            link = Color(0xFF086DDD),
            tag = Color(0xFF7852EE),
            inlineCode = Color(0xFF086DDD),
            inlineCodeBackground = Color(0xFFFAFAFA),
            highlight = Color(0x337852EE),
            titlebar = Color(0xFFF6F6F6),
            statusBar = Color(0xFFF6F6F6),
            navText = Color(0xFF222222),
            navMuted = Color(0xFF5C5C5C),
            fileLineWidth = 700.dp,
            lineHeight = 1.5f,
            radiusSmall = 4.dp,
            radiusMedium = 8.dp,
            fontSize = 16f,
            h1Size = 1.618f * 16f,
            h2Size = 1.462f * 16f,
            h3Size = 1.318f * 16f,
            h4Size = 1.196f * 16f,
            h5Size = 1.091f * 16f,
            h6Size = 1f * 16f,
            fontText = "sans-serif",
            fontInterface = "sans-serif",
            fontMonospace = "monospace",
        )
        private val defaultDarkPalette: ThemePalette by lazy { createDefaultDark() }
        private val defaultLightPalette: ThemePalette by lazy { createDefaultLight() }

        fun defaultDark(): ThemePalette = defaultDarkPalette
        fun defaultLight(): ThemePalette = defaultLightPalette

    }
}

/** Helper used by Compose code that wants a theme token as text size. */
fun ThemePalette.headingSize(level: Int): Float = when (level) {
    1 -> h1Size
    2 -> h2Size
    3 -> h3Size
    4 -> h4Size
    5 -> h5Size
    else -> h6Size
}
