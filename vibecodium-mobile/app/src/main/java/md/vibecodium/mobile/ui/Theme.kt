package md.vibecodium.mobile.ui

import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import md.vibecodium.mobile.DarkCodeColors
import md.vibecodium.mobile.LocalCodeColors

// Values taken from the Obsidian 1.15.0 mobile dark stylesheet:
// `.theme-dark` plus the `.is-mobile.theme-dark` overrides in app.css.
private val BackgroundPrimary = Color(0xFF000000)
private val BackgroundPrimaryAlt = Color(0xFF111111)
private val BackgroundSecondary = Color(0xFF1E1E1E)
private val BackgroundSecondaryAlt = Color(0xFF282828)
private val Base25 = Color(0xFF2E2E2E)
private val Base30 = Color(0xFF333333)
private val TextNormal = Color(0xFFDADADA)
private val TextMuted = Color(0xFFB3B3B3)
private val Accent = Color(0xFF8A5CF5)
private val AccentHover = Color(0xFFA68AF9)
private val Error = Color(0xFFFB464C)

private val ObsidianDark = darkColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    primaryContainer = Color(0xFF352A5C),
    onPrimaryContainer = Color(0xFFE8DDFF),
    secondary = TextMuted,
    onSecondary = Color(0xFF000000),
    secondaryContainer = Base30,
    onSecondaryContainer = TextNormal,
    tertiary = Color(0xFF53DFDD),
    onTertiary = Color(0xFF000000),
    tertiaryContainer = Color(0xFF1E3A3A),
    onTertiaryContainer = Color(0xFFCFF7F6),
    background = BackgroundPrimary,
    onBackground = TextNormal,
    surface = BackgroundSecondary,
    onSurface = TextNormal,
    surfaceVariant = Base25,
    onSurfaceVariant = TextMuted,
    // A neutral tint stops Surface(tonalElevation) from blending a purple
    // overlay into the navbar, drawers and dialogs.
    surfaceTint = BackgroundSecondary,
    inverseSurface = TextNormal,
    inverseOnSurface = BackgroundSecondary,
    inversePrimary = AccentHover,
    error = Error,
    onError = Color.White,
    errorContainer = Color(0xFF5C1F22),
    onErrorContainer = Color(0xFFFFDAD6),
    outline = Base30,
    outlineVariant = Base25,
    scrim = Color.Black,
    surfaceDim = BackgroundPrimary,
    surfaceBright = Base30,
    surfaceContainerLowest = BackgroundPrimary,
    surfaceContainerLow = BackgroundPrimaryAlt,
    surfaceContainer = BackgroundSecondary,
    surfaceContainerHigh = BackgroundSecondaryAlt,
    surfaceContainerHighest = Base30,
)

@Composable
fun VibecodiumTheme(content: @Composable () -> Unit) {
    CompositionLocalProvider(
        LocalCodeColors provides DarkCodeColors,
        LocalContentColor provides TextNormal,
        LocalTextSelectionColors provides TextSelectionColors(
            handleColor = Accent,
            backgroundColor = Accent.copy(alpha = 0.30f),
        ),
    ) {
        MaterialTheme(colorScheme = ObsidianDark, content = content)
    }
}
