package md.vibecodium.mobile

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeCssTest {
    @Test
    fun rootTokensResolveForDarkAndLight() {
        val css = """
            :root, body {
                --background-primary: #101010;
                --background-secondary: #181818;
                --text-normal: #eeeeee;
                --accent-h: 200;
                --accent-s: 50%;
                --accent-l: 40%;
                --color-accent: hsl(var(--accent-h), var(--accent-s), var(--accent-l));
                --file-line-width: 640px;
            }
            .theme-dark {
                --background-primary: #000000;
                --text-normal: #dadada;
                --interactive-accent: var(--color-accent);
            }
            .theme-light {
                --background-primary: #ffffff;
                --text-normal: #222222;
            }
            body.is-mobile {
                --file-line-width: 480px;
            }
        """.trimIndent()

        val dark = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertEquals(0xFF000000.toInt(), dark.palette.background.toArgb())
        assertEquals(0xFFDADADA.toInt(), dark.palette.text.toArgb())
        assertEquals(480f, dark.palette.fileLineWidth.value)
        // hsl(200, 50%, 40%) is a blue-teal accent.
        assertTrue(dark.palette.accent.blue > dark.palette.accent.red)

        val light = ThemeCss.resolve(css, dark = false, isTablet = false, isPhone = true)
        assertEquals(0xFFFFFFFF.toInt(), light.palette.background.toArgb())
        assertEquals(0xFF222222.toInt(), light.palette.text.toArgb())
    }

    @Test
    fun calcAndColorMixAreEvaluated() {
        val css = """
            body {
                --accent-h: 258;
                --accent-s: 88%;
                --accent-l: 66%;
                --color-accent-1: hsl(calc(var(--accent-h) - 3), calc(var(--accent-s) * 1.02), calc(var(--accent-l) * 1.15));
                --text-selection: color-mix(in srgb, #ff0000 50%, transparent);
                --highlight-background: color-mix(in oklch, #00ff00 30%, transparent);
                --text-highlight-bg: color-mix(in oklch, #00ff00 30%, transparent);
            }
            .theme-dark {
                --interactive-accent-hover: var(--color-accent-1);
            }
        """.trimIndent()
        val result = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertEquals(0x80FF0000.toInt(), result.palette.selection.toArgb())
        assertTrue(result.palette.accentHover.green > 0.5f)
        assertTrue(result.palette.highlight.green > 0.5f)
        assertTrue(result.palette.highlight.alpha < 0.5f)
        assertTrue(result.variables.containsKey("--color-accent-1"))
        assertTrue(result.variables.getValue("--color-accent-1").startsWith("hsl("))
    }

    @Test
    fun obsidianSelectorsAreSkippedWithAWarning() {
        val css = """
            .workspace-tab-header { --tab-background-active: #ff0000; }
            body { --background-primary: #101010; }
        """.trimIndent()
        val result = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertEquals(0xFF101010.toInt(), result.palette.background.toArgb())
        assertTrue(result.warnings.any { it.contains("Obsidian-only") })
    }

    @Test
    fun themeOnlyAccentPartsStillWork() {
        val css = """
            .theme-dark {
                --accent-h: 10;
                --accent-s: 100%;
                --accent-l: 50%;
            }
        """.trimIndent()
        val result = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertTrue(result.palette.accent.red > 0.9f)
        assertTrue(result.palette.accent.green < 0.2f)
    }

    @Test
    fun atomThemeOverridesBothModes() {
        val css = resource("atom.css")
        val dark = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertEquals(0xFF272B34.toInt(), dark.palette.background.toArgb())
        assertEquals(0xFFDCDDDE.toInt(), dark.palette.text.toArgb())

        val light = ThemeCss.resolve(css, dark = false, isTablet = false, isPhone = true)
        assertEquals(0xFFFAFAFA.toInt(), light.palette.background.toArgb())
        assertEquals(0xFF383A42.toInt(), light.palette.text.toArgb())
    }

    @Test
    fun thingsThemeResolvesPaletteOnlyVariables() {
        // Things defines --color-base-* in combined `body.theme-dark.is-mobile`
        // rules and relies on Obsidian's semantic aliases for the rest.
        val css = resource("things.css")
        val dark = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true)
        assertEquals(0xFF1C2127.toInt(), dark.palette.background.toArgb())
        assertEquals(0xFFDADADA.toInt(), dark.palette.text.toArgb())

        val light = ThemeCss.resolve(css, dark = false, isTablet = false, isPhone = true)
        assertEquals(0xFFFFFFFF.toInt(), light.palette.background.toArgb())
    }

    @Test
    fun themeConfigRoundTripsAndChecksStamp() {
        val config = ThemeConfig(
            schema = ThemeConfig.SCHEMA,
            cssStamp = "123:456",
            warnings = listOf("warning"),
            phoneLight = ThemePalette.defaultLight(),
            phoneDark = ThemePalette.defaultDark(),
            tabletLight = ThemePalette.defaultLight(),
            tabletDark = ThemePalette.defaultDark(),
        )
        val parsed = ThemeConfig.fromJson(config.toJson(), expectedStamp = "123:456")
        assertNotNull(parsed)
        assertEquals(
            config.phoneLight.background.toArgb(),
            parsed!!.phoneLight.background.toArgb(),
        )
        assertEquals(config.warnings, parsed.warnings)
        assertNull(ThemeConfig.fromJson(config.toJson(), expectedStamp = "999:999"))
    }

    private fun resource(name: String): String = checkNotNull(
        javaClass.getResourceAsStream("/themes/$name"),
    ).bufferedReader().use { it.readText() }
}
