package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SettingsTest {
    @Test
    fun defaultsMatchThePreviousShellConstants() {
        val settings = AppSettings()
        assertEquals(1f, settings.textScale, 0f)
        assertTrue(settings.autoHideChrome)
        assertEquals(false, settings.navbarEnabled)
        assertEquals(EditorMode.Source, settings.defaultEditorMode)
        assertEquals(600, settings.autoSaveDelayMs)
    }

    @Test
    fun roundTripKeepsEveryField() {
        val original = AppSettings(
            textScale = 1.15f,
            autoHideChrome = false,
            navbarEnabled = true,
            defaultEditorMode = EditorMode.Preview,
            autoSaveDelayMs = 300,
        )
        assertEquals(original, AppSettingsJson.decode(AppSettingsJson.encode(original)))
    }

    @Test
    fun blankOrBrokenJsonFallsBackToDefaults() {
        assertEquals(AppSettings(), AppSettingsJson.decode(null))
        assertEquals(AppSettings(), AppSettingsJson.decode(""))
        assertEquals(AppSettings(), AppSettingsJson.decode("{not json"))
    }

    @Test
    fun textScaleIsClampedToTheSupportedRange() {
        val high = AppSettingsJson.decode("""{"textScale":9}""")
        val low = AppSettingsJson.decode("""{"textScale":0.1}""")
        assertEquals(AppSettingsJson.MAX_TEXT_SCALE, high.textScale, 0f)
        assertEquals(AppSettingsJson.MIN_TEXT_SCALE, low.textScale, 0f)
    }

    @Test
    fun unknownEditorModeFallsBackToSource() {
        val settings = AppSettingsJson.decode("""{"defaultEditorMode":"Sideways"}""")
        assertEquals(EditorMode.Source, settings.defaultEditorMode)
    }
}
