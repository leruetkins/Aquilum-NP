package md.vibecodium.mobile.ui

import android.graphics.Typeface
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.sp
import md.vibecodium.mobile.LocalCodeColors
import md.vibecodium.mobile.ThemePalette
import md.vibecodium.mobile.ThemePalette.Companion.defaultDark
import md.vibecodium.mobile.ThemePalette.Companion.defaultLight

/** Current resolved theme, available to shell composables without prop drilling. */
val LocalVibecodiumPalette = staticCompositionLocalOf { defaultDark() }

private val DefaultDarkScheme: ColorScheme by lazy { ThemePalette.defaultDark().colorScheme() }
private val DefaultLightScheme: ColorScheme by lazy { ThemePalette.defaultLight().colorScheme() }

@Composable
fun VibecodiumTheme(palette: ThemePalette, content: @Composable () -> Unit) {
    val isDefaultDark = palette === ThemePalette.defaultDark()
    val isDefaultLight = palette === ThemePalette.defaultLight()
    val isDefault = isDefaultDark || isDefaultLight
    val needsTypography = !isDefault && (
        !isGenericFontFamily(palette.fontText) ||
            !isGenericFontFamily(palette.fontInterface) ||
            palette.fontSize != 16f ||
            palette.lineHeight != 1.5f
        )
    val textFamily = remember(palette.fontText) { resolveFontFamily(palette.fontText) }
    val uiFamily = remember(palette.fontInterface) { resolveFontFamily(palette.fontInterface) }
    val colorScheme = remember(palette) { when {
        isDefaultDark -> DefaultDarkScheme
        isDefaultLight -> DefaultLightScheme
        else -> palette.colorScheme()
    } }
    val typography = remember(
        needsTypography,
        palette.fontText,
        palette.fontInterface,
        palette.fontSize,
        palette.lineHeight,
        textFamily,
        uiFamily,
    ) {
        if (!needsTypography) null else palette.typography(textFamily, uiFamily)
    }
    CompositionLocalProvider(
        LocalVibecodiumPalette provides palette,
        LocalCodeColors provides palette.codeColors,
        LocalContentColor provides palette.text,
        LocalTextSelectionColors provides TextSelectionColors(
            handleColor = palette.accent,
            backgroundColor = palette.selection,
        ),
    ) {
        if (typography == null) {
            MaterialTheme(colorScheme = colorScheme, content = content)
        } else {
            MaterialTheme(colorScheme = colorScheme, typography = typography, content = content)
        }
    }
}

private fun isGenericFontFamily(value: String): Boolean {
    val name = value.trim().trim('"', '\'').lowercase()
    return name.isBlank() || name == "??" ||
        name.startsWith("sans-serif") || name.startsWith("serif") ||
        name.startsWith("monospace") || name.startsWith("system-ui") ||
        name.startsWith("-apple-system") || name.startsWith("ui-")
}

private fun ThemePalette.typography(text: FontFamily, ui: FontFamily): Typography {
    val base = Typography()
    val size = fontSize.sp
    val line = (fontSize * lineHeight).sp
    return base.copy(
        displayLarge = base.displayLarge.copy(fontFamily = text),
        displayMedium = base.displayMedium.copy(fontFamily = text),
        displaySmall = base.displaySmall.copy(fontFamily = text),
        headlineLarge = base.headlineLarge.copy(fontFamily = text),
        headlineMedium = base.headlineMedium.copy(fontFamily = text),
        headlineSmall = base.headlineSmall.copy(fontFamily = text),
        titleLarge = base.titleLarge.copy(fontFamily = ui),
        titleMedium = base.titleMedium.copy(fontFamily = ui),
        titleSmall = base.titleSmall.copy(fontFamily = ui),
        bodyLarge = base.bodyLarge.copy(fontFamily = text, fontSize = size, lineHeight = line),
        bodyMedium = base.bodyMedium.copy(fontFamily = text, fontSize = size, lineHeight = line),
        bodySmall = base.bodySmall.copy(fontFamily = text),
        labelLarge = base.labelLarge.copy(fontFamily = ui),
        labelMedium = base.labelMedium.copy(fontFamily = ui),
        labelSmall = base.labelSmall.copy(fontFamily = ui),
    )
}

/**
 * Maps the first CSS family from `--font-*-theme` to an Android typeface.
 * Unknown family names fall back to the default font instead of failing.
 */
private fun resolveFontFamily(css: String): FontFamily {
    val name = css.trim().trim('"', '\'').lowercase()
    return when {
        name.isBlank() || name == "??" || name == "inherit" -> FontFamily.Default
        name.startsWith("sans-serif") || name.startsWith("system-ui") ||
            name.startsWith("-apple-system") || name.startsWith("ui-sans-serif") -> FontFamily.SansSerif
        name.startsWith("serif") -> FontFamily.Serif
        name.startsWith("monospace") || name.startsWith("ui-monospace") -> FontFamily.Monospace
        else -> runCatching {
            FontFamily(Typeface.create(css.trim().trim('"', '\''), Typeface.NORMAL))
        }.getOrDefault(FontFamily.Default)
    }
}

/**
 * Maps the Obsidian token bundle to Material 3 roles. Surface elevations are
 * neutral because the shell draws its own Obsidian-like chrome; a tinted
 * `surfaceTint` would blend purple into every raised container.
 */
fun ThemePalette.colorScheme(): ColorScheme {
    val builder = if (dark) {
        darkColorScheme(
            primary = accent,
            onPrimary = onAccent,
            primaryContainer = surfaceAlt,
            onPrimaryContainer = text,
            secondary = textMuted,
            onSecondary = background,
            secondaryContainer = surfaceAlt,
            onSecondaryContainer = text,
            tertiary = codeType,
            onTertiary = background,
            tertiaryContainer = surface,
            onTertiaryContainer = text,
            background = background,
            onBackground = text,
            surface = surface,
            onSurface = text,
            surfaceVariant = surfaceAlt,
            onSurfaceVariant = textMuted,
            surfaceTint = surface,
            inverseSurface = text,
            inverseOnSurface = background,
            inversePrimary = accentHover,
            error = error,
            onError = Color.White,
            errorContainer = surfaceAlt,
            onErrorContainer = text,
            outline = outline,
            outlineVariant = outline.copy(alpha = 0.55f),
            scrim = Color.Black,
            surfaceDim = background,
            surfaceBright = surfaceAlt,
            surfaceContainerLowest = background,
            surfaceContainerLow = backgroundAlt,
            surfaceContainer = surface,
            surfaceContainerHigh = surfaceAlt,
            surfaceContainerHighest = surfaceAlt.lighten(0.05f),
        )
    } else {
        lightColorScheme(
            primary = accent,
            onPrimary = onAccent,
            primaryContainer = surfaceAlt,
            onPrimaryContainer = text,
            secondary = textMuted,
            onSecondary = background,
            secondaryContainer = surfaceAlt,
            onSecondaryContainer = text,
            tertiary = codeType,
            onTertiary = background,
            tertiaryContainer = surface,
            onTertiaryContainer = text,
            background = background,
            onBackground = text,
            surface = surface,
            onSurface = text,
            surfaceVariant = surfaceAlt,
            onSurfaceVariant = textMuted,
            surfaceTint = surface,
            inverseSurface = text,
            inverseOnSurface = background,
            inversePrimary = accentHover,
            error = error,
            onError = Color.White,
            errorContainer = surfaceAlt,
            onErrorContainer = text,
            outline = outline,
            outlineVariant = outline.copy(alpha = 0.55f),
            scrim = Color.Black,
            surfaceDim = surface,
            surfaceBright = background,
            surfaceContainerLowest = background,
            surfaceContainerLow = backgroundAlt,
            surfaceContainer = surface,
            surfaceContainerHigh = surfaceAlt,
            surfaceContainerHighest = backgroundAlt,
        )
    }
    return builder
}

private fun Color.lighten(delta: Float): Color = Color(
    red = (red + delta).coerceAtMost(1f),
    green = (green + delta).coerceAtMost(1f),
    blue = (blue + delta).coerceAtMost(1f),
    alpha = alpha,
)
