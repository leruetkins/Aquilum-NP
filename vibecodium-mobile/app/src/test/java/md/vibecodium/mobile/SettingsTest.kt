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
            headings = HeadingsSettings(
                h1 = HeadingStyle(26, "#123456", bold = false, italic = true),
            ),
        )
        assertEquals(original, AppSettingsJson.decode(AppSettingsJson.encode(original)))
    }

    @Test
    fun headingDefaultsMatchTheDesktopPaletteAndTunedSizes() {
        val headings = HeadingsSettings()
        assertEquals(19, headings.h1.fontSize)
        assertEquals("blue", headings.h1.colorName)
        assertTrue(headings.h1.bold)
        assertEquals(0x56B4E9, HeadingColors.rgb(headings.h2.colorName))
        assertEquals("#0072B2", HeadingColors.hex(headings.h1.colorName))
        assertEquals(0x123456, HeadingColors.parseHex("#123456"))
    }

    @Test
    fun nativeHeadingPayloadCarriesEveryLevel() {
        val payload = org.json.JSONArray(HeadingsSettings().toNativeJson())
        assertEquals(6, payload.length())
        val first = payload.getJSONObject(0)
        assertEquals(19, first.getInt("size"))
        assertEquals(0x0072B2, first.getInt("color"))
        assertEquals(true, first.getBoolean("bold"))
        assertEquals(false, first.getBoolean("italic"))
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

    @Test
    fun vaultSyncRoundTripKeepsEveryField() {
        val original = VaultSyncSettings(
            paused = true,
            deviceName = "Pixel",
            allowImages = false,
            allowAudio = false,
            allowVideo = true,
            allowPdf = false,
            allowOther = false,
            excludedFolders = listOf("Archive", "work/private"),
            preventSleep = true,
        )
        assertEquals(original, VaultSyncJson.decode(VaultSyncJson.encode(original)))
    }

    @Test
    fun vaultSyncFoldersAreNormalizedAndDeduplicated() {
        val settings = VaultSyncJson.decode(
            """{"excludedFolders":["/Archive/","Archive","  work/private  ",""]}""",
        )
        assertEquals(listOf("Archive", "work/private"), settings.excludedFolders)
    }

    @Test
    fun filterExcludesOnlyTheFolderOrItsChildren() {
        val filter = SyncFilter(excludedFolders = listOf("Archive"))
        assertEquals(false, filter.accepts("Archive/note.md"))
        assertEquals(false, filter.accepts("Archive"))
        assertEquals(true, filter.accepts("Archives/note.md"))
        assertEquals(true, filter.accepts("note.md"))
    }

    @Test
    fun filterSwitchesFileFamiliesButAlwaysAllowsNotes() {
        val filter = SyncFilter(
            allowImages = false,
            allowAudio = false,
            allowVideo = true,
            allowPdf = false,
            allowOther = false,
        )
        assertEquals(false, filter.accepts("pic.png"))
        assertEquals(false, filter.accepts("clip.mp3"))
        assertEquals(true, filter.accepts("clip.mp4"))
        assertEquals(false, filter.accepts("doc.pdf"))
        assertEquals(false, filter.accepts("script.kts"))
        assertEquals(true, filter.accepts("note.md"))
    }
}
