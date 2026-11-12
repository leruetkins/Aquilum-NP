package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDateTime

class DailyNotesTest {
    private val now = LocalDateTime.of(2026, 11, 11, 9, 5, 7)

    @Test
    fun expandsEveryDesktopToken() {
        assertEquals("2026-11-11", expandDailyNoteFormat("{{YYYY}}-{{MM}}-{{DD}}", now))
        assertEquals("11.11.26", expandDailyNoteFormat("{{D}}.{{M}}.{{YY}}", now))
        assertEquals("09:05:07", expandDailyNoteFormat("{{HH}}:{{mm}}:{{ss}}", now))
        assertEquals("9-5-7", expandDailyNoteFormat("{{H}}-{{m}}-{{s}}", now))
        assertEquals(
            "11 November 2026",
            expandDailyNoteFormat("{{D}} {{MMMM}} {{YYYY}}", now),
        )
        assertEquals("Nov 11, 2026", expandDailyNoteFormat("{{MMM}} {{D}}, {{YYYY}}", now))
    }

    @Test
    fun keepsUnknownAndUnclosedPlaceholdersVerbatim() {
        assertEquals("План на 11 {{MONTH}}", expandDailyNoteFormat("План на {{DD}} {{MONTH}}", now))
        assertEquals("no tokens", expandDailyNoteFormat("no tokens", now))
        assertEquals("open {{YYYY", expandDailyNoteFormat("open {{YYYY", now))
    }

    @Test
    fun fileStemMatchesDesktopSanitizing() {
        assertEquals("2026-11-11", dailyNoteFileStem("{{YYYY}}-{{MM}}-{{DD}}", now))
        assertEquals("09-05", dailyNoteFileStem("{{HH}}:{{mm}}", now))
        assertEquals("2026-11-11", dailyNoteFileStem("{{YYYY}}-{{MM}}-{{DD}}.md", now))
        assertEquals("Заметка", dailyNoteFileStem("  Заметка  ", now))
        assertEquals("2026-11-11", dailyNoteFileStem("...", now))
    }

    @Test
    fun folderSegmentsCannotEscapeTheVault() {
        assertEquals("Daily", normalizeDailyNoteFolder("Daily"))
        assertEquals("a/b", normalizeDailyNoteFolder(" /a/../b// "))
        assertEquals("", normalizeDailyNoteFolder(".."))
    }

    @Test
    fun pathJoinsTheConfiguredFolderAndStem() {
        val settings = DailyNoteSettings(folder = "Journal/Daily")
        assertEquals("Journal/Daily/2026-11-11.md", dailyNotePath(settings, now))
        assertEquals("2026-11-11.md", dailyNotePath(DailyNoteSettings(), now))
    }

    @Test
    fun desktopDailyNotesJsonDecodesWithMobileDefaults() {
        val settings = AppSettingsJson.decode(
            """{"dailyNotes":{"format":"{{DD}}.{{MM}}.{{YYYY}}","folder":"Daily"}}""",
        )
        assertEquals("{{DD}}.{{MM}}.{{YYYY}}", settings.dailyNotes.format)
        assertEquals("Daily", settings.dailyNotes.folder)
        assertTrue(settings.dailyNotes.enabled)
        assertEquals("", settings.dailyNotes.template)
    }

    @Test
    fun dailyNotesRoundTripKeepsEveryField() {
        val original = DailyNoteSettings(
            enabled = false,
            format = "{{YYYY}}/{{MM}}",
            folder = "Journal",
            template = "Templates/Daily.md",
        )
        val decoded = AppSettingsJson.decode(
            AppSettingsJson.encode(AppSettings(dailyNotes = original)),
        )
        assertEquals(original, decoded.dailyNotes)
    }
}
