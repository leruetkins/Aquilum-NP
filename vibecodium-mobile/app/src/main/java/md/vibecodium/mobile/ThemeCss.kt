package md.vibecodium.mobile

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cbrt
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Small, deliberately narrow CSS reader for Obsidian themes.
 *
 * It understands the part of CSS that carries theme tokens: root rules
 * (`:root`, `html`, `body`, `.theme-dark`, `.theme-light`, `.is-mobile`,
 * `.is-phone`, `.is-tablet`) inside `@media`/`@supports`/`@layer`, custom
 * properties, `var()`, `calc()`, and the color syntaxes used by the community
 * catalog. Selectors that target Obsidian's DOM are ignored: the native shell
 * has no DOM to apply them to.
 */
object ThemeCss {
    private const val MAX_CSS_BYTES = 2_000_000
    private const val BASE_FONT_PX = 16.0

    data class Result(
        val palette: ThemePalette,
        val variables: Map<String, String>,
        val warnings: List<String>,
    )

    fun resolve(
        css: String,
        dark: Boolean,
        isTablet: Boolean,
        isPhone: Boolean,
    ): Result {
        val warnings = mutableListOf<String>()
        if (css.toByteArray(Charsets.UTF_8).size > MAX_CSS_BYTES) {
            warnings += "Theme is larger than ${MAX_CSS_BYTES / 1_000_000} MB; only its root tokens were read."
        }
        val source = stripComments(css.take(MAX_CSS_BYTES))
        val context = MatchContext(
            dark = dark,
            isTablet = isTablet,
            isPhone = isPhone,
            classes = buildSet {
                add(if (dark) "theme-dark" else "theme-light")
                add("is-mobile")
                add("is-android")
                add("is-focused")
                add("show-view-header")
                add("show-ribbon")
                add("show-inline-title")
                if (isPhone) add("is-phone")
                if (isTablet) add("is-tablet")
            },
        )
        val chosen = LinkedHashMap<String, Chosen>()
        val order = intArrayOf(0)
        parseInto(source, context, chosen, order)

        val raw = chosen.mapValues { it.value.value }
        val merged = LinkedHashMap<String, String>(baseVariables(dark))
        merged.putAll(raw)
        val variables = LinkedHashMap<String, String>(merged.size)
        for (name in merged.keys) {
            val value = resolveVariable(name, merged, LinkedHashSet(), 0)
            if (!value.isNullOrBlank()) variables[name] = evaluateCalcs(value)
        }

        val base = if (dark) ThemePalette.defaultDark() else ThemePalette.defaultLight()
        val palette = buildPalette(variables, dark, base)
        if (chosen.isEmpty()) {
            warnings += "No root theme variables were found; the built-in palette is used."
        }
        val unsupported = countUnsupportedRules(source, context)
        if (unsupported > 0) {
            warnings += "$unsupported rule${if (unsupported == 1) "" else "s"} target Obsidian-only selectors and were skipped."
        }
        return Result(palette, variables, warnings.distinct())
    }

    // ----------------------------------------------------------------- parse

    private data class MatchContext(
        val dark: Boolean,
        val isTablet: Boolean,
        val isPhone: Boolean,
        val classes: Set<String>,
    )

    private data class Chosen(val value: String, val specificity: Int, val order: Int)

    private fun parseInto(
        css: String,
        context: MatchContext,
        out: MutableMap<String, Chosen>,
        order: IntArray,
    ) {
        var i = 0
        val n = css.length
        while (i < n) {
            while (i < n && (css[i].isWhitespace() || css[i] == ';')) i++
            if (i >= n) break
            if (css[i] == '}') {
                i++
                continue
            }
            val preludeStart = i
            var brace = -1
            var semicolon = -1
            var quote: Char? = null
            var parens = 0
            var brackets = 0
            while (i < n) {
                val c = css[i]
                if (quote != null) {
                    if (c == '\\') {
                        i += 2
                        continue
                    }
                    if (c == quote) quote = null
                } else {
                    when (c) {
                        '"', '\'' -> quote = c
                        '(' -> parens++
                        ')' -> parens = (parens - 1).coerceAtLeast(0)
                        '[' -> brackets++
                        ']' -> brackets = (brackets - 1).coerceAtLeast(0)
                        '{' -> if (parens == 0 && brackets == 0) {
                            brace = i
                        }
                        ';' -> if (parens == 0 && brackets == 0) {
                            semicolon = i
                        }
                    }
                }
                if (brace >= 0 || semicolon >= 0) break
                i++
            }
            if (semicolon >= 0 && (brace < 0 || semicolon < brace)) {
                i = semicolon + 1
                continue
            }
            if (brace < 0) {
                // A trailing statement without a block at the end of the file.
                break
            }
            val prelude = css.substring(preludeStart, brace).trim()
            val end = matchingBrace(css, brace)
            if (end < 0) break
            val body = css.substring(brace + 1, end)
            i = end + 1
            order[0]++

            if (prelude.startsWith("@")) {
                val name = prelude.substring(1).substringBefore('(').substringBefore(' ').trim().lowercase()
                when (name) {
                    "media" -> if (mediaMatches(prelude, context)) {
                        parseInto(body, context, out, order)
                    }
                    "supports", "layer", "container", "scope" -> parseInto(body, context, out, order)
                    else -> Unit
                }
                continue
            }

            val selectors = splitTopLevel(prelude, ',')
            var best = -1
            for (selector in selectors) {
                val specificity = matchRootSelector(selector.trim(), context)
                if (specificity != null && specificity > best) best = specificity
            }
            if (best < 0) continue

            for ((name, value) in parseDeclarations(body)) {
                if (!name.startsWith("--")) continue
                val previous = out[name]
                if (previous == null ||
                    best > previous.specificity ||
                    (best == previous.specificity && order[0] >= previous.order)
                ) {
                    out[name] = Chosen(value, best, order[0])
                }
            }
        }
    }

    private fun mediaMatches(prelude: String, context: MatchContext): Boolean {
        val lower = prelude.lowercase()
        if (lower.contains("print") && !lower.contains("screen")) return false
        val scheme = Regex("prefers-color-scheme\\s*:\\s*(dark|light)").find(lower)
        if (scheme != null) {
            return if (scheme.groupValues[1] == "dark") context.dark else !context.dark
        }
        return true
    }

    /**
     * Returns CSS specificity if [selector] is a simple compound selector that
     * can apply to the document root, or null when it targets the Obsidian DOM.
     */
    private fun matchRootSelector(selector: String, context: MatchContext): Int? {
        if (selector.isEmpty()) return null
        if (hasTopLevelCombinator(selector)) return null
        var i = 0
        var specificity = 0
        var matched = false

        fun readName(): String {
            val start = i
            while (i < selector.length && (selector[i].isLetterOrDigit() || selector[i] == '-' || selector[i] == '_')) i++
            return selector.substring(start, i)
        }

        if (selector.startsWith("body")) {
            i = 4
            specificity += 1
            matched = true
        } else if (selector.startsWith("html")) {
            i = 4
            specificity += 1
            matched = true
        } else if (selector.startsWith(":root")) {
            i = 5
            specificity += 10
            matched = true
        }

        while (i < selector.length) {
            when (val c = selector[i]) {
                '.' -> {
                    i++
                    val name = readName()
                    if (name.isEmpty() || name !in context.classes) return null
                    specificity += 10
                    matched = true
                }
                ':' -> {
                    if (selector.startsWith(":not(", i)) {
                        val end = matchingParen(selector, i + 4)
                        if (end < 0) return null
                        val inner = selector.substring(i + 5, end)
                        var innerBest = -1
                        for (part in splitTopLevel(inner, ',')) {
                            val spec = matchRootSelector(part.trim(), context)
                            if (spec != null && spec > innerBest) innerBest = spec
                        }
                        if (innerBest >= 0) return null // :not(...) matched, so the selector fails
                        specificity += 10
                        i = end + 1
                    } else if (selector.startsWith(":is(", i) || selector.startsWith(":where(", i)) {
                        val isWhere = selector.startsWith(":where(", i)
                        val offset = if (isWhere) 7 else 4
                        val end = matchingParen(selector, i + offset)
                        if (end < 0) return null
                        val inner = selector.substring(i + offset + 1, end)
                        var innerBest = -1
                        for (part in splitTopLevel(inner, ',')) {
                            val spec = matchRootSelector(part.trim(), context)
                            if (spec != null && spec > innerBest) innerBest = spec
                        }
                        if (innerBest < 0) return null
                        if (!isWhere) specificity += innerBest
                        i = end + 1
                    } else if (selector.startsWith(":root", i)) {
                        i += 5
                        specificity += 10
                        matched = true
                    } else {
                        // Unknown pseudo-class (:hover, [data-x]) has no stable
                        // meaning for a root token; ignore this selector.
                        return null
                    }
                }
                else -> return null
            }
        }
        return if (matched) specificity else null
    }

    private fun hasTopLevelCombinator(selector: String): Boolean {
        var quote: Char? = null
        var parens = 0
        var brackets = 0
        for ((index, c) in selector.withIndex()) {
            if (quote != null) {
                if (c == '\\') continue
                if (c == quote) quote = null
                continue
            }
            when (c) {
                '"', '\'' -> quote = c
                '(' -> parens++
                ')' -> parens = (parens - 1).coerceAtLeast(0)
                '[' -> brackets++
                ']' -> brackets = (brackets - 1).coerceAtLeast(0)
                ' ', '\t', '\n', '>', '+', '~' -> if (parens == 0 && brackets == 0) {
                    // Whitespace before a compound is a descendant combinator.
                    if (c.isWhitespace()) {
                        val rest = selector.substring(index).trimStart()
                        if (rest.isEmpty() || rest.startsWith(',') || rest.startsWith(')')) continue
                        return true
                    }
                    return true
                }
            }
        }
        return false
    }

    private fun countUnsupportedRules(css: String, context: MatchContext): Int {
        var count = 0
        var i = 0
        while (i < css.length) {
            while (i < css.length && (css[i].isWhitespace() || css[i] == ';')) i++
            if (i >= css.length) break
            val start = i
            var brace = -1
            var quote: Char? = null
            var parens = 0
            while (i < css.length) {
                val c = css[i]
                if (quote != null) {
                    if (c == '\\') {
                        i += 2
                        continue
                    }
                    if (c == quote) quote = null
                } else {
                    when (c) {
                        '"', '\'' -> quote = c
                        '(' -> parens++
                        ')' -> parens = (parens - 1).coerceAtLeast(0)
                        '{' -> if (parens == 0) brace = i
                        '}' -> if (parens == 0 && brace < 0) {
                            i++
                        }
                    }
                }
                if (brace >= 0) break
                i++
            }
            if (brace < 0) break
            val prelude = css.substring(start, brace).trim()
            val end = matchingBrace(css, brace)
            if (end < 0) break
            val body = css.substring(brace + 1, end)
            i = end + 1
            if (prelude.startsWith("@")) {
                if (prelude.startsWith("@media") && mediaMatches(prelude, context) ||
                    prelude.startsWith("@supports") || prelude.startsWith("@layer")
                ) {
                    count += countUnsupportedRules(body, context)
                }
            } else {
                val selectors = splitTopLevel(prelude, ',').map { it.trim() }.filter { it.isNotEmpty() }
                if (selectors.isNotEmpty() && selectors.none { matchRootSelector(it, context) != null }) {
                    if (parseDeclarations(body).isNotEmpty()) count++
                }
            }
        }
        return count
    }

    private fun parseDeclarations(body: String): List<Pair<String, String>> {
        val result = mutableListOf<Pair<String, String>>()
        for (part in splitTopLevel(body, ';')) {
            val colon = findTopLevelColon(part) ?: continue
            val name = part.substring(0, colon).trim()
            var value = part.substring(colon + 1).trim()
            if (value.endsWith("!important", ignoreCase = true)) {
                value = value.dropLast("!important".length).trim()
            }
            if (name.isNotEmpty() && value.isNotEmpty()) result += name to value
        }
        return result
    }

    // ------------------------------------------------------------ variables

    private fun resolveVariable(
        name: String,
        raw: Map<String, String>,
        stack: MutableSet<String>,
        depth: Int,
    ): String? {
        if (depth > 24 || name in stack) return null
        val value = raw[name] ?: return null
        stack += name
        val result = substituteVars(value, raw, stack, depth + 1)
        stack -= name
        return result
    }

    private fun substituteVars(
        text: String,
        raw: Map<String, String>,
        stack: MutableSet<String>,
        depth: Int,
    ): String? {
        if (depth > 24) return text
        var out = text
        repeat(32) {
            val start = indexOfIgnoreCase(out, "var(")
            if (start < 0) return out
            val end = matchingParen(out, start + 3)
            if (end < 0) return out
            val inner = out.substring(start + 4, end)
            val parts = splitTopLevel(inner, ',')
            val ref = parts.firstOrNull()?.trim().orEmpty()
            val fallback = if (parts.size > 1) parts.drop(1).joinToString(",") else ""
            val resolved = if (ref.startsWith("--")) {
                resolveVariable(ref, raw, stack, depth + 1)
            } else {
                null
            }
            val replacement = resolved?.takeIf { it.isNotBlank() }
                ?: substituteVars(fallback, raw, stack, depth + 1)
                ?: ""
            out = out.substring(0, start) + replacement + out.substring(end + 1)
        }
        return out
    }

    private fun evaluateCalcs(text: String): String {
        var out = text
        repeat(32) {
            val start = indexOfIgnoreCase(out, "calc(")
            if (start < 0) return out
            val end = matchingParen(out, start + 4)
            if (end < 0) return out
            val expression = out.substring(start + 5, end)
            val value = CalcParser(expression).parse() ?: return out
            out = out.substring(0, start) + value + out.substring(end + 1)
        }
        return out
    }

    // --------------------------------------------------------------- palette

    /**
     * Obsidian's `body`/`.theme-dark`/`.theme-light` defaults. Themes often
     * override only `--color-base-*` or `--accent-h/s/l` and rely on these
     * semantic aliases, so they must exist before theme variables resolve.
     */
    private fun baseVariables(dark: Boolean): Map<String, String> {
        val base = if (dark) {
            mapOf(
                "--color-red" to "#fb464c",
                "--color-orange" to "#e9973f",
                "--color-yellow" to "#e0de71",
                "--color-green" to "#44cf6e",
                "--color-cyan" to "#53dfdd",
                "--color-blue" to "#027aff",
                "--color-purple" to "#a882ff",
                "--color-pink" to "#fa99cd",
                "--color-base-00" to "#1C1C1C",
                "--color-base-05" to "#212121",
                "--color-base-10" to "#232323",
                "--color-base-20" to "#282828",
                "--color-base-25" to "#2e2e2e",
                "--color-base-30" to "#333333",
                "--color-base-35" to "#3f3f3f",
                "--color-base-40" to "#555555",
                "--color-base-50" to "#666666",
                "--color-base-60" to "#999999",
                "--color-base-70" to "#b3b3b3",
                "--color-base-100" to "#dadada",
                "--background-secondary-alt" to "var(--color-base-30)",
            )
        } else {
            mapOf(
                "--color-red" to "#e93147",
                "--color-orange" to "#ec7500",
                "--color-yellow" to "#e0ac00",
                "--color-green" to "#08b94e",
                "--color-cyan" to "#00bfbc",
                "--color-blue" to "#086ddd",
                "--color-purple" to "#7852ee",
                "--color-pink" to "#d53984",
                "--color-base-00" to "#ffffff",
                "--color-base-05" to "#fcfcfc",
                "--color-base-10" to "#fafafa",
                "--color-base-20" to "#f6f6f6",
                "--color-base-25" to "#efefef",
                "--color-base-30" to "#e4e4e4",
                "--color-base-35" to "#dadada",
                "--color-base-40" to "#bdbdbd",
                "--color-base-50" to "#ababab",
                "--color-base-60" to "#707070",
                "--color-base-70" to "#5c5c5c",
                "--color-base-100" to "#222222",
                "--background-secondary-alt" to "var(--color-base-05)",
            )
        }
        val accent1 = if (dark) {
            "hsl(calc(var(--accent-h) - 3), calc(var(--accent-s) * 1.02), calc(var(--accent-l) * 1.15))"
        } else {
            "hsl(calc(var(--accent-h) - 1), calc(var(--accent-s) * 1.01), calc(var(--accent-l) * 1.075))"
        }
        val accent2 = if (dark) {
            "hsl(calc(var(--accent-h) - 5), calc(var(--accent-s) * 1.05), calc(var(--accent-l) * 1.29))"
        } else {
            "hsl(calc(var(--accent-h) - 3), calc(var(--accent-s) * 1.02), calc(var(--accent-l) * 1.15))"
        }
        return base + mapOf(
            "--accent-h" to "258",
            "--accent-s" to "88%",
            "--accent-l" to "66%",
            "--color-accent" to "hsl(var(--accent-h), var(--accent-s), var(--accent-l))",
            "--color-accent-1" to accent1,
            "--color-accent-2" to accent2,
            "--background-primary" to "var(--color-base-00)",
            "--background-primary-alt" to "var(--color-base-10)",
            "--background-secondary" to "var(--color-base-20)",
            "--text-normal" to "var(--color-base-100)",
            "--text-muted" to "var(--color-base-70)",
            "--text-faint" to "var(--color-base-50)",
            "--text-accent" to "var(--color-accent)",
            "--text-accent-hover" to "var(--color-accent-2)",
            "--interactive-accent" to if (dark) "var(--color-accent)" else "var(--color-accent-1)",
            "--interactive-accent-hover" to "var(--color-accent-2)",
            "--background-modifier-border" to "var(--color-base-30)",
            "--text-selection" to "color-mix(in oklch, var(--interactive-accent) 20%, transparent)",
            "--link-color" to "var(--text-accent)",
            "--tag-color" to "var(--text-accent)",
            "--tag-background" to "color-mix(in oklch, var(--interactive-accent) 10%, transparent)",
            "--text-highlight-bg" to "color-mix(in oklch, var(--color-yellow) 30%, transparent)",
            "--highlight-background" to "var(--text-highlight-bg)",
            "--code-background" to "var(--background-primary-alt)",
            "--code-normal" to "var(--text-normal)",
            "--code-comment" to "var(--text-faint)",
            "--code-function" to "var(--color-yellow)",
            "--code-keyword" to "var(--color-pink)",
            "--code-operator" to "var(--color-red)",
            "--code-property" to "var(--color-cyan)",
            "--code-punctuation" to "var(--text-muted)",
            "--code-string" to "var(--color-green)",
            "--code-tag" to "var(--color-red)",
            "--code-value" to "var(--color-purple)",
            "--file-line-width" to "700px",
            "--line-height-normal" to "1.5",
            "--radius-s" to "4px",
            "--radius-m" to "8px",
            "--font-text-size" to "16px",
            "--h1-size" to "1.618em",
            "--h2-size" to "1.462em",
            "--h3-size" to "1.318em",
            "--h4-size" to "1.188em",
            "--h5-size" to "1.076em",
            "--h6-size" to "1em",
            "--titlebar-background" to "var(--background-secondary)",
            "--status-bar-background" to "var(--background-secondary)",
            "--nav-item-color" to "var(--text-muted)",
        )
    }

    private fun buildPalette(
        vars: Map<String, String>,
        dark: Boolean,
        base: ThemePalette,
    ): ThemePalette {
        fun raw(vararg names: String): String? = names.firstNotNullOfOrNull { vars[it]?.takeIf(String::isNotBlank) }
        fun color(vararg names: String): Color? = raw(*names)?.let { parseColor(it, dark) }
        fun dimension(vararg names: String): Float? = raw(*names)?.let { parseDimension(it) }
        fun number(vararg names: String): Float? = raw(*names)?.let { parseNumber(it) }
        fun font(vararg names: String): String? = raw(*names)?.let { firstFontFamily(it) }
        fun emphasis(vararg names: String): Float? = raw(*names)?.let { parseEmphasis(it, base.fontSize) }

        val accentParts = accentFromParts(vars)
        val accent = color("--interactive-accent", "--color-accent", "--text-accent") ?: accentParts ?: base.accent
        val accentHover = color("--interactive-accent-hover", "--color-accent-2") ?: accent.lighten(0.14f)
        val background = color("--background-primary") ?: base.background
        val text = color("--text-normal") ?: base.text

        val palette = base.copy(
            dark = dark,
            background = background,
            backgroundAlt = color("--background-primary-alt") ?: base.backgroundAlt,
            surface = color("--background-secondary") ?: base.surface,
            surfaceAlt = color("--background-secondary-alt") ?: base.surfaceAlt,
            text = text,
            textMuted = color("--text-muted", "--text-secondary") ?: base.textMuted,
            textFaint = color("--text-faint") ?: base.textFaint,
            accent = accent,
            accentHover = accentHover,
            onAccent = color("--text-on-accent") ?: base.onAccent,
            error = color("--text-error", "--color-red") ?: base.error,
            outline = color("--background-modifier-border", "--divider-color") ?: base.outline,
            selection = color("--text-selection") ?: accent.copy(alpha = 0.30f),
            overlay = color("--background-modifier-cover") ?: base.overlay,
            codeBackground = color("--code-background") ?: base.codeBackground,
            codeNormal = color("--code-normal") ?: text,
            codeComment = color("--code-comment") ?: base.codeComment,
            codeKeyword = color("--code-keyword") ?: base.codeKeyword,
            codeString = color("--code-string") ?: base.codeString,
            codeNumber = color("--code-value", "--code-number") ?: base.codeNumber,
            codeFunction = color("--code-function") ?: base.codeFunction,
            codeType = color("--code-tag", "--code-type") ?: base.codeType,
            codePunctuation = color("--code-punctuation") ?: base.codePunctuation,
            heading = color("--text-title", "--text-normal") ?: text,
            marker = color("--text-faint", "--text-muted") ?: base.marker,
            link = color("--link-color", "--text-accent") ?: accent,
            tag = color("--tag-color", "--text-accent") ?: accent,
            inlineCode = color("--code-string", "--code-normal") ?: base.inlineCode,
            inlineCodeBackground = color("--code-background") ?: base.inlineCodeBackground,
            highlight = color("--text-highlight-bg", "--highlight-background") ?: base.highlight,
            titlebar = color("--titlebar-background", "--background-secondary") ?: base.titlebar,
            statusBar = color("--status-bar-background", "--background-secondary") ?: base.statusBar,
            navText = color("--nav-item-color", "--text-normal") ?: text,
            navMuted = color("--nav-item-color", "--text-muted") ?: base.navMuted,
            fileLineWidth = (dimension("--file-line-width") ?: base.fileLineWidth.value).dp,
            lineHeight = number("--line-height-normal") ?: base.lineHeight,
            radiusSmall = (dimension("--radius-s") ?: base.radiusSmall.value).dp,
            radiusMedium = (dimension("--radius-m") ?: base.radiusMedium.value).dp,
            fontSize = dimension("--font-text-size") ?: base.fontSize,
            h1Size = emphasis("--h1-size") ?: base.h1Size,
            h2Size = emphasis("--h2-size") ?: base.h2Size,
            h3Size = emphasis("--h3-size") ?: base.h3Size,
            h4Size = emphasis("--h4-size") ?: base.h4Size,
            h5Size = emphasis("--h5-size") ?: base.h5Size,
            h6Size = emphasis("--h6-size") ?: base.h6Size,
            fontText = font("--font-text-theme") ?: base.fontText,
            fontInterface = font("--font-interface-theme") ?: base.fontInterface,
            fontMonospace = font("--font-monospace-theme") ?: base.fontMonospace,
        )
        return palette
    }

    private fun accentFromParts(vars: Map<String, String>): Color? {
        val hue = vars["--accent-h"]?.let { parseNumber(it) } ?: return null
        val saturation = vars["--accent-s"]?.let { parseNumber(it) } ?: return null
        val lightness = vars["--accent-l"]?.let { parseNumber(it) } ?: return null
        return hslToColor(hue.toDouble(), saturation / 100.0, lightness / 100.0, 1.0)
    }

    private fun Color.lighten(delta: Float): Color = Color(
        red = (red + delta).coerceAtMost(1f),
        green = (green + delta).coerceAtMost(1f),
        blue = (blue + delta).coerceAtMost(1f),
        alpha = alpha,
    )

    private fun parseDimension(value: String): Float? {
        val text = evaluateCalcs(value).trim()
        val match = Regex("^([+-]?(?:\\d+\\.?\\d*|\\.\\d+))([a-zA-Z%]*)$").find(text) ?: return null
        val amount = match.groupValues[1].toDoubleOrNull() ?: return null
        return when (match.groupValues[2].lowercase()) {
            "px", "" -> amount.toFloat()
            "dp" -> amount.toFloat()
            "rem", "em" -> (amount * BASE_FONT_PX).toFloat()
            "pt" -> (amount * 1.3333).toFloat()
            "pc" -> (amount * 16.0).toFloat()
            "in" -> (amount * 96.0).toFloat()
            "cm" -> (amount * 37.795).toFloat()
            "mm" -> (amount * 3.7795).toFloat()
            else -> null
        }
    }

    private fun parseNumber(value: String): Float? {
        val text = evaluateCalcs(value).trim()
        val match = Regex("^([+-]?(?:\\d+\\.?\\d*|\\.\\d+))(?:[a-zA-Z%]*)$").find(text) ?: return null
        return match.groupValues[1].toDoubleOrNull()?.toFloat()
    }

    private fun parseEmphasis(value: String, base: Float): Float? {
        val text = evaluateCalcs(value).trim()
        val match = Regex("^([+-]?(?:\\d+\\.?\\d*|\\.\\d+))([a-zA-Z%]*)$").find(text) ?: return null
        val amount = match.groupValues[1].toDoubleOrNull() ?: return null
        return when (match.groupValues[2].lowercase()) {
            "px", "dp", "" -> amount.toFloat()
            "rem", "em" -> (amount * base).toFloat()
            "%" -> (amount / 100.0 * base).toFloat()
            else -> null
        }
    }

    private fun firstFontFamily(value: String): String? {
        for (part in splitTopLevel(value, ',')) {
            val name = part.trim().trim('"', '\'').trim()
            if (name.isEmpty() || name == "??" || name.equals("italic", true)) continue
            return name
        }
        return null
    }

    // ----------------------------------------------------------------- colors

    private data class Rgba(val r: Double, val g: Double, val b: Double, val a: Double)

    private fun parseColor(value: String, dark: Boolean): Color? {
        val text = value.trim()
        if (text.isEmpty()) return null
        parseColorInternal(text, dark)?.let { return it.toComposeColor() }
        return null
    }

    private fun parseColorInternal(text: String, dark: Boolean): Rgba? {
        val value = text.trim()
        if (value.isEmpty()) return null
        if (value.startsWith("#")) return parseHex(value)
        val lower = value.lowercase()
        if (lower == "transparent") return Rgba(0.0, 0.0, 0.0, 0.0)
        NAMED_COLORS[lower]?.let { return it }
        if (lower == "currentcolor" || lower == "inherit" || lower == "initial") return null

        val open = value.indexOf('(')
        if (open < 0 || !value.endsWith(")")) return null
        val name = value.substring(0, open).trim().lowercase()
        val inner = value.substring(open + 1, value.length - 1).trim()
        return when (name) {
            "rgb", "rgba" -> parseRgbFunction(inner)
            "hsl", "hsla" -> parseHslFunction(inner)
            "oklch" -> parseOklchFunction(inner)
            "oklab" -> parseOklabFunction(inner)
            "light-dark" -> {
                val parts = splitTopLevel(inner, ',')
                if (parts.size < 2) null else parseColorInternal(if (dark) parts[0] else parts[1], dark)
            }
            "color-mix" -> parseColorMix(inner, dark)
            else -> null
        }
    }

    private fun parseHex(value: String): Rgba? {
        val hex = value.removePrefix("#")
        return when (hex.length) {
            3 -> Rgba(
                hex[0].digit().times(17) / 255.0,
                hex[1].digit().times(17) / 255.0,
                hex[2].digit().times(17) / 255.0,
                1.0,
            )
            4 -> Rgba(
                hex[0].digit().times(17) / 255.0,
                hex[1].digit().times(17) / 255.0,
                hex[2].digit().times(17) / 255.0,
                hex[3].digit().times(17) / 255.0,
            )
            6 -> Rgba(
                hex.substring(0, 2).toIntOrNull(16)?.div(255.0) ?: return null,
                hex.substring(2, 4).toIntOrNull(16)?.div(255.0) ?: return null,
                hex.substring(4, 6).toIntOrNull(16)?.div(255.0) ?: return null,
                1.0,
            )
            8 -> Rgba(
                hex.substring(0, 2).toIntOrNull(16)?.div(255.0) ?: return null,
                hex.substring(2, 4).toIntOrNull(16)?.div(255.0) ?: return null,
                hex.substring(4, 6).toIntOrNull(16)?.div(255.0) ?: return null,
                hex.substring(6, 8).toIntOrNull(16)?.div(255.0) ?: return null,
            )
            else -> null
        }
    }

    private fun Char.digit(): Int = when (this) {
        in '0'..'9' -> this - '0'
        in 'a'..'f' -> this - 'a' + 10
        in 'A'..'F' -> this - 'A' + 10
        else -> 0
    }

    private fun parseRgbFunction(inner: String): Rgba? {
        val (channels, alphaText) = splitAlpha(inner)
        val parts = splitFunctionArgs(channels)
        if (parts.size < 3) return null
        val r = channelValue(parts[0]) ?: return null
        val g = channelValue(parts[1]) ?: return null
        val b = channelValue(parts[2]) ?: return null
        val a = alphaText?.let { parseAlpha(it) }
            ?: parts.getOrNull(3)?.let { parseAlpha(it) }
            ?: 1.0
        return Rgba(r, g, b, a)
    }

    private fun channelValue(text: String): Double? {
        val trimmed = text.trim()
        return if (trimmed.endsWith("%")) {
            trimmed.dropLast(1).toDoubleOrNull()?.div(100.0)
        } else {
            trimmed.toDoubleOrNull()?.div(255.0)
        }
    }

    private fun parseAlpha(text: String): Double? {
        val trimmed = text.trim()
        return if (trimmed.endsWith("%")) {
            trimmed.dropLast(1).toDoubleOrNull()?.div(100.0)
        } else {
            trimmed.toDoubleOrNull()
        }
    }

    private fun parseHslFunction(inner: String): Rgba? {
        val (channels, alphaText) = splitAlpha(inner)
        val parts = splitFunctionArgs(channels)
        if (parts.size < 3) return null
        val hue = parseAngle(parts[0]) ?: return null
        val saturation = parsePercent(parts[1]) ?: return null
        val lightness = parsePercent(parts[2]) ?: return null
        val alpha = alphaText?.let { parseAlpha(it) }
            ?: parts.getOrNull(3)?.let { parseAlpha(it) }
            ?: 1.0
        return hslToRgba(hue, saturation, lightness, alpha)
    }

    private fun parseAngle(text: String): Double? {
        val trimmed = text.trim().lowercase()
        val match = Regex("^([+-]?(?:\\d+\\.?\\d*|\\.\\d+))([a-z]*)$").find(trimmed) ?: return null
        val amount = match.groupValues[1].toDoubleOrNull() ?: return null
        return when (match.groupValues[2]) {
            "deg", "" -> amount
            "rad" -> amount * 180.0 / PI
            "grad" -> amount * 0.9
            "turn" -> amount * 360.0
            else -> null
        }
    }

    private fun parsePercent(text: String): Double? {
        val trimmed = text.trim()
        return if (trimmed.endsWith("%")) {
            trimmed.dropLast(1).toDoubleOrNull()?.div(100.0)
        } else {
            trimmed.toDoubleOrNull()?.let { if (abs(it) > 1.0) it / 100.0 else it }
        }
    }

    private fun parseOklchFunction(inner: String): Rgba? {
        val (channels, alphaText) = splitAlpha(inner)
        val parts = splitFunctionArgs(channels)
        if (parts.size < 3) return null
        val lightness = parsePercentOrNumber(parts[0]) ?: return null
        val chroma = parsePercentOrNumber(parts[1])?.let { if (parts[1].contains('%')) it * 0.4 else it } ?: return null
        val hue = parseAngle(parts[2]) ?: return null
        val alpha = alphaText?.let { parseAlpha(it) } ?: 1.0
        return oklabToRgba(lightness, chroma * cos(Math.toRadians(hue)), chroma * sin(Math.toRadians(hue)), alpha)
    }

    private fun parseOklabFunction(inner: String): Rgba? {
        val (channels, alphaText) = splitAlpha(inner)
        val parts = splitFunctionArgs(channels)
        if (parts.size < 3) return null
        val lightness = parsePercentOrNumber(parts[0]) ?: return null
        val a = parsePercentOrNumber(parts[1]) ?: return null
        val b = parsePercentOrNumber(parts[2]) ?: return null
        val alpha = alphaText?.let { parseAlpha(it) } ?: 1.0
        val scale = if (parts[1].contains('%') || parts[2].contains('%')) 0.4 else 1.0
        return oklabToRgba(lightness, a * scale, b * scale, alpha)
    }

    private fun parsePercentOrNumber(text: String): Double? {
        val trimmed = text.trim()
        return if (trimmed.endsWith("%")) {
            trimmed.dropLast(1).toDoubleOrNull()?.div(100.0)
        } else {
            trimmed.toDoubleOrNull()
        }
    }

    private fun parseColorMix(inner: String, dark: Boolean): Rgba? {
        val parts = splitTopLevel(inner, ',')
        if (parts.size < 3) return null
        val space = parts[0].trim().removePrefix("in").trim().lowercase()
        val colors = mutableListOf<Pair<Rgba, Double>>()
        for (part in parts.drop(1)) {
            val match = Regex("^(.*?)\\s+([\\d.]+)%\\s*$").find(part.trim())
            val colorText = if (match != null) match.groupValues[1] else part.trim()
            val weight = if (match != null) match.groupValues[2].toDoubleOrNull() ?: 100.0 else 100.0
            val color = parseColorInternal(colorText, dark) ?: return null
            colors += color to weight
        }
        if (colors.size < 2) return null
        var (first, firstWeight) = colors[0]
        var (second, secondWeight) = colors[1]
        if (parts.size == 3) {
            // Only one weight is commonly given; infer the other.
            if (!parts[1].contains('%') && parts[2].contains('%')) firstWeight = 100.0 - secondWeight
            if (parts[1].contains('%') && !parts[2].contains('%')) secondWeight = 100.0 - firstWeight
        }
        val total = firstWeight + secondWeight
        if (total <= 0.0) return null
        firstWeight /= total
        secondWeight /= total
        return when {
            space.contains("oklab") || space.contains("oklch") -> {
                val alpha = first.a * firstWeight + second.a * secondWeight
                if (alpha <= 0.000001) {
                    Rgba(0.0, 0.0, 0.0, 0.0)
                } else {
                    val a = toOklab(first)
                    val b = toOklab(second)
                    val fa = first.a * firstWeight / alpha
                    val fb = second.a * secondWeight / alpha
                    oklabToRgba(
                        a[0] * fa + b[0] * fb,
                        a[1] * fa + b[1] * fb,
                        a[2] * fa + b[2] * fb,
                        alpha,
                    )
                }
            }
            space.contains("srgb-linear") -> mixLinear(first, firstWeight, second, secondWeight)
            else -> mixSrgb(first, firstWeight, second, secondWeight)
        }
    }

    private fun mixSrgb(a: Rgba, wa: Double, b: Rgba, wb: Double): Rgba {
        val alpha = a.a * wa + b.a * wb
        if (alpha <= 0.000001) return Rgba(0.0, 0.0, 0.0, 0.0)
        return Rgba(
            (a.r * a.a * wa + b.r * b.a * wb) / alpha,
            (a.g * a.a * wa + b.g * b.a * wb) / alpha,
            (a.b * a.a * wa + b.b * b.a * wb) / alpha,
            alpha,
        )
    }

    private fun mixLinear(a: Rgba, wa: Double, b: Rgba, wb: Double): Rgba {
        val alpha = a.a * wa + b.a * wb
        if (alpha <= 0.000001) return Rgba(0.0, 0.0, 0.0, 0.0)
        val fa = a.a * wa / alpha
        val fb = b.a * wb / alpha
        fun mixChannel(x: Double, y: Double): Double =
            linearToSrgb(srgbToLinear(x) * fa + srgbToLinear(y) * fb)
        return Rgba(mixChannel(a.r, b.r), mixChannel(a.g, b.g), mixChannel(a.b, b.b), alpha)
    }

    private fun hslToRgba(hue: Double, saturation: Double, lightness: Double, alpha: Double): Rgba {
        val h = ((hue % 360.0) + 360.0) % 360.0
        val c = (1.0 - abs(2.0 * lightness - 1.0)) * saturation
        val x = c * (1.0 - abs((h / 60.0) % 2.0 - 1.0))
        val m = lightness - c / 2.0
        val (r, g, b) = when {
            h < 60 -> Triple(c, x, 0.0)
            h < 120 -> Triple(x, c, 0.0)
            h < 180 -> Triple(0.0, c, x)
            h < 240 -> Triple(0.0, x, c)
            h < 300 -> Triple(x, 0.0, c)
            else -> Triple(c, 0.0, x)
        }
        return Rgba((r + m).coerceIn(0.0, 1.0), (g + m).coerceIn(0.0, 1.0), (b + m).coerceIn(0.0, 1.0), alpha)
    }

    private fun hslToColor(hue: Double, saturation: Double, lightness: Double, alpha: Double): Color =
        hslToRgba(hue, saturation, lightness, alpha).toComposeColor()

    private fun srgbToLinear(value: Double): Double =
        if (value <= 0.04045) value / 12.92 else ((value + 0.055) / 1.055).pow(2.4)

    private fun linearToSrgb(value: Double): Double =
        if (value <= 0.0031308) value * 12.92 else 1.055 * value.pow(1.0 / 2.4) - 0.055

    private fun toOklab(color: Rgba): DoubleArray {
        val r = srgbToLinear(color.r)
        val g = srgbToLinear(color.g)
        val b = srgbToLinear(color.b)
        val l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
        val m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
        val s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
        val l_ = cbrt(l)
        val m_ = cbrt(m)
        val s_ = cbrt(s)
        return doubleArrayOf(
            0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
            1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
            0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
        )
    }

    private fun oklabToRgba(l: Double, a: Double, b: Double, alpha: Double): Rgba {
        val l_ = l + 0.3963377774 * a + 0.2158037573 * b
        val m_ = l - 0.1055613458 * a - 0.0638541728 * b
        val s_ = l - 0.0894841775 * a - 1.2914855480 * b
        val l3 = l_.pow(3.0)
        val m3 = m_.pow(3.0)
        val s3 = s_.pow(3.0)
        val r = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3
        val g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3
        val bl = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3
        return Rgba(
            linearToSrgb(r).coerceIn(0.0, 1.0),
            linearToSrgb(g).coerceIn(0.0, 1.0),
            linearToSrgb(bl).coerceIn(0.0, 1.0),
            alpha.coerceIn(0.0, 1.0),
        )
    }

    private fun Rgba.toComposeColor(): Color = Color(
        red = r.coerceIn(0.0, 1.0).toFloat(),
        green = g.coerceIn(0.0, 1.0).toFloat(),
        blue = b.coerceIn(0.0, 1.0).toFloat(),
        alpha = a.coerceIn(0.0, 1.0).toFloat(),
    )

    private fun splitAlpha(inner: String): Pair<String, String?> {
        val parts = splitTopLevel(inner, '/')
        return if (parts.size >= 2) parts[0] to parts.drop(1).joinToString("/") else inner to null
    }

    private fun splitFunctionArgs(text: String): List<String> {
        val commas = splitTopLevel(text, ',')
        return if (commas.size >= 3) commas else splitTopLevelWhitespace(text)
    }

    private fun splitTopLevelWhitespace(text: String): List<String> {
        val result = mutableListOf<String>()
        val current = StringBuilder()
        var quote: Char? = null
        var parens = 0
        for (c in text) {
            if (quote != null) {
                current.append(c)
                if (c == quote) quote = null
                continue
            }
            when (c) {
                '"', '\'' -> {
                    quote = c
                    current.append(c)
                }
                '(' -> {
                    parens++
                    current.append(c)
                }
                ')' -> {
                    parens = (parens - 1).coerceAtLeast(0)
                    current.append(c)
                }
                else -> if (c.isWhitespace() && parens == 0) {
                    if (current.isNotEmpty()) {
                        result += current.toString()
                        current.clear()
                    }
                } else {
                    current.append(c)
                }
            }
        }
        if (current.isNotEmpty()) result += current.toString()
        return result.filter { it.isNotEmpty() }
    }

    // ------------------------------------------------------------------ utils

    private fun stripComments(css: String): String {
        val out = StringBuilder(css.length)
        var i = 0
        var quote: Char? = null
        while (i < css.length) {
            val c = css[i]
            if (quote != null) {
                out.append(c)
                if (c == '\\' && i + 1 < css.length) {
                    out.append(css[i + 1])
                    i += 2
                    continue
                }
                if (c == quote) quote = null
                i++
                continue
            }
            if (c == '/' && i + 1 < css.length && css[i + 1] == '*') {
                val end = css.indexOf("*/", i + 2)
                i = if (end < 0) css.length else end + 2
                out.append(' ')
                continue
            }
            if (c == '"' || c == '\'') quote = c
            out.append(c)
            i++
        }
        return out.toString()
    }

    private fun matchingBrace(text: String, open: Int): Int {
        var depth = 0
        var quote: Char? = null
        var i = open
        while (i < text.length) {
            val c = text[i]
            if (quote != null) {
                if (c == '\\') {
                    i += 2
                    continue
                }
                if (c == quote) quote = null
            } else {
                when (c) {
                    '"', '\'' -> quote = c
                    '{' -> depth++
                    '}' -> {
                        depth--
                        if (depth == 0) return i
                    }
                }
            }
            i++
        }
        return -1
    }

    private fun matchingParen(text: String, open: Int): Int {
        var depth = 0
        var quote: Char? = null
        var i = open
        while (i < text.length) {
            val c = text[i]
            if (quote != null) {
                if (c == '\\') {
                    i += 2
                    continue
                }
                if (c == quote) quote = null
            } else {
                when (c) {
                    '"', '\'' -> quote = c
                    '(' -> depth++
                    ')' -> {
                        depth--
                        if (depth == 0) return i
                    }
                }
            }
            i++
        }
        return -1
    }

    private fun splitTopLevel(text: String, delimiter: Char): List<String> {
        val result = mutableListOf<String>()
        val current = StringBuilder()
        var quote: Char? = null
        var parens = 0
        var brackets = 0
        var i = 0
        while (i < text.length) {
            val c = text[i]
            if (quote != null) {
                current.append(c)
                if (c == '\\' && i + 1 < text.length) {
                    current.append(text[i + 1])
                    i += 2
                    continue
                }
                if (c == quote) quote = null
                i++
                continue
            }
            when (c) {
                '"', '\'' -> {
                    quote = c
                    current.append(c)
                }
                '(' -> {
                    parens++
                    current.append(c)
                }
                ')' -> {
                    parens = (parens - 1).coerceAtLeast(0)
                    current.append(c)
                }
                '[' -> {
                    brackets++
                    current.append(c)
                }
                ']' -> {
                    brackets = (brackets - 1).coerceAtLeast(0)
                    current.append(c)
                }
                else -> if (c == delimiter && parens == 0 && brackets == 0) {
                    result += current.toString()
                    current.clear()
                } else {
                    current.append(c)
                }
            }
            i++
        }
        result += current.toString()
        return result
    }

    private fun findTopLevelColon(text: String): Int? {
        var quote: Char? = null
        var parens = 0
        for ((index, c) in text.withIndex()) {
            if (quote != null) {
                if (c == quote) quote = null
                continue
            }
            when (c) {
                '"', '\'' -> quote = c
                '(' -> parens++
                ')' -> parens = (parens - 1).coerceAtLeast(0)
                ':' -> if (parens == 0) return index
            }
        }
        return null
    }

    private fun indexOfIgnoreCase(text: String, needle: String): Int {
        if (needle.isEmpty()) return -1
        val limit = text.length - needle.length
        for (i in 0..limit) {
            if (text.regionMatches(i, needle, 0, needle.length, ignoreCase = true)) return i
        }
        return -1
    }

    // -------------------------------------------------------------- calc()

    private data class CalcNumber(val value: Double, val unit: String)

    private class CalcParser(private val text: String) {
        private var index = 0

        fun parse(): String? {
            val value = expression() ?: return null
            skipSpace()
            if (index != text.length) return null
            return format(value)
        }

        private fun expression(): CalcNumber? {
            var left = term() ?: return null
            while (true) {
                skipSpace()
                val op = if (index < text.length) text[index] else return left
                if (op != '+' && op != '-') return left
                index++
                val right = term() ?: return null
                left = if (op == '+') add(left, right) ?: return null else subtract(left, right) ?: return null
            }
        }

        private fun term(): CalcNumber? {
            var left = factor() ?: return null
            while (true) {
                skipSpace()
                val op = if (index < text.length) text[index] else return left
                if (op != '*' && op != '/') return left
                index++
                val right = factor() ?: return null
                left = if (op == '*') multiply(left, right) ?: return null else divide(left, right) ?: return null
            }
        }

        private fun factor(): CalcNumber? {
            skipSpace()
            if (index >= text.length) return null
            if (text[index] == '(') {
                index++
                val value = expression() ?: return null
                skipSpace()
                if (index >= text.length || text[index] != ')') return null
                index++
                return value
            }
            var sign = 1.0
            if (text[index] == '+' || text[index] == '-') {
                if (text[index] == '-') sign = -1.0
                index++
                skipSpace()
            }
            val start = index
            while (index < text.length && (text[index].isDigit() || text[index] == '.')) index++
            if (start == index) return null
            val number = text.substring(start, index).toDoubleOrNull() ?: return null
            val unitStart = index
            while (index < text.length && text[index].isLetter()) index++
            if (index < text.length && text[index] == '%') index++
            val unit = text.substring(unitStart, index)
            return CalcNumber(sign * number, unit)
        }

        private fun add(a: CalcNumber, b: CalcNumber): CalcNumber? = when {
            a.unit == b.unit -> CalcNumber(a.value + b.value, a.unit)
            a.unit.isEmpty() -> CalcNumber(a.value + b.value, b.unit)
            b.unit.isEmpty() -> CalcNumber(a.value + b.value, a.unit)
            else -> null
        }

        private fun subtract(a: CalcNumber, b: CalcNumber): CalcNumber? = when {
            a.unit == b.unit -> CalcNumber(a.value - b.value, a.unit)
            a.unit.isEmpty() -> CalcNumber(a.value - b.value, b.unit)
            b.unit.isEmpty() -> CalcNumber(a.value - b.value, a.unit)
            else -> null
        }

        private fun multiply(a: CalcNumber, b: CalcNumber): CalcNumber? = when {
            a.unit.isEmpty() -> CalcNumber(a.value * b.value, b.unit)
            b.unit.isEmpty() -> CalcNumber(a.value * b.value, a.unit)
            else -> null
        }

        private fun divide(a: CalcNumber, b: CalcNumber): CalcNumber? = when {
            b.unit.isEmpty() && b.value != 0.0 -> CalcNumber(a.value / b.value, a.unit)
            a.unit == b.unit && b.value != 0.0 -> CalcNumber(a.value / b.value, "")
            else -> null
        }

        private fun format(value: CalcNumber): String {
            val rounded = if (abs(value.value - value.value.toLong()) < 0.0001) {
                value.value.toLong().toString()
            } else {
                String.format(java.util.Locale.US, "%.4f", value.value).trimEnd('0').trimEnd('.')
            }
            return rounded + value.unit
        }

        private fun skipSpace() {
            while (index < text.length && text[index].isWhitespace()) index++
        }
    }

    private val NAMED_COLORS: Map<String, Rgba> = mapOf(
        "black" to Rgba(0.0, 0.0, 0.0, 1.0),
        "white" to Rgba(1.0, 1.0, 1.0, 1.0),
        "red" to Rgba(1.0, 0.0, 0.0, 1.0),
        "green" to Rgba(0.0, 128.0 / 255.0, 0.0, 1.0),
        "lime" to Rgba(0.0, 1.0, 0.0, 1.0),
        "blue" to Rgba(0.0, 0.0, 1.0, 1.0),
        "yellow" to Rgba(1.0, 1.0, 0.0, 1.0),
        "orange" to Rgba(1.0, 165.0 / 255.0, 0.0, 1.0),
        "purple" to Rgba(128.0 / 255.0, 0.0, 128.0 / 255.0, 1.0),
        "fuchsia" to Rgba(1.0, 0.0, 1.0, 1.0),
        "magenta" to Rgba(1.0, 0.0, 1.0, 1.0),
        "cyan" to Rgba(0.0, 1.0, 1.0, 1.0),
        "aqua" to Rgba(0.0, 1.0, 1.0, 1.0),
        "teal" to Rgba(0.0, 128.0 / 255.0, 128.0 / 255.0, 1.0),
        "navy" to Rgba(0.0, 0.0, 128.0 / 255.0, 1.0),
        "olive" to Rgba(128.0 / 255.0, 128.0 / 255.0, 0.0, 1.0),
        "maroon" to Rgba(128.0 / 255.0, 0.0, 0.0, 1.0),
        "silver" to Rgba(192.0 / 255.0, 192.0 / 255.0, 192.0 / 255.0, 1.0),
        "gray" to Rgba(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0),
        "grey" to Rgba(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0),
        "dimgray" to Rgba(105.0 / 255.0, 105.0 / 255.0, 105.0 / 255.0, 1.0),
        "dimgrey" to Rgba(105.0 / 255.0, 105.0 / 255.0, 105.0 / 255.0, 1.0),
        "whitesmoke" to Rgba(245.0 / 255.0, 245.0 / 255.0, 245.0 / 255.0, 1.0),
    )
}
