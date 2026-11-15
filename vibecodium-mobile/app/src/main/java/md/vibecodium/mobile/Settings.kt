package md.vibecodium.mobile

import org.json.JSONArray
import org.json.JSONObject

private const val DEFAULT_DAILY_NOTE_FORMAT = "{{YYYY}}-{{MM}}-{{DD}}"

/** One heading palette entry, the same Okabe-Ito set the desktop app offers. */
data class HeadingColorPreset(val id: String, val hex: String, val rgb: Int)

object HeadingColors {
    val presets = listOf(
        HeadingColorPreset("orange", "#E69F00", 0xE69F00),
        HeadingColorPreset("sky_blue", "#56B4E9", 0x56B4E9),
        HeadingColorPreset("bluish_green", "#009E73", 0x009E73),
        HeadingColorPreset("yellow", "#F0E442", 0xF0E442),
        HeadingColorPreset("blue", "#0072B2", 0x0072B2),
        HeadingColorPreset("vermilion", "#D55E00", 0xD55E00),
        HeadingColorPreset("reddish_purple", "#CC79A7", 0xCC79A7),
        HeadingColorPreset("red", "#F44336", 0xF44336),
    )

    private val fallback = presets.first { it.id == "blue" }

    /** Resolves a preset id, a hex string or an alias to an opaque RGB value. */
    fun rgb(name: String): Int = parseHex(name) ?: findPreset(name).rgb

    /** Normalizes a preset id or a custom colour to `#RRGGBB`. */
    fun hex(name: String): String =
        parseHex(name)?.let { "#%06X".format(it) } ?: findPreset(name).hex

    fun parseHex(value: String): Int? {
        val clean = value.trim().removePrefix("#")
        if (clean.length != 6 || clean.any { it.digitToIntOrNull(16) == null }) return null
        return clean.toIntOrNull(16)
    }

    private fun findPreset(name: String): HeadingColorPreset {
        presets.firstOrNull {
            it.id.equals(name, ignoreCase = true) || it.hex.equals(name, ignoreCase = true)
        }?.let { return it }
        return when (name.lowercase()) {
            "slate" -> presets.first { it.id == "sky_blue" }
            "teal", "emerald" -> presets.first { it.id == "bluish_green" }
            "indigo" -> fallback
            "purple", "violet", "fuchsia" -> presets.first { it.id == "reddish_purple" }
            "crimson", "rose" -> presets.first { it.id == "red" }
            else -> fallback
        }
    }
}

/**
 * One heading style, mirroring the desktop `HeadingStyle` block. [fontSize] is
 * the size in dp at [AppSettingsJson.DEFAULT_EDITOR_FONT_DP]; the editor scales
 * it with the configured body size, and [colorName] is a
 * [HeadingColors] preset id or a custom `#RRGGBB` value.
 */
data class HeadingStyle(
    val fontSize: Int,
    val colorName: String = "blue",
    val bold: Boolean = true,
    val italic: Boolean = false,
)

/** Per-level heading styles; the defaults match the sizes the editor was tuned for. */
data class HeadingsSettings(
    val h1: HeadingStyle = HeadingStyle(19, "blue"),
    val h2: HeadingStyle = HeadingStyle(18, "sky_blue"),
    val h3: HeadingStyle = HeadingStyle(17, "bluish_green"),
    val h4: HeadingStyle = HeadingStyle(16, "orange"),
    val h5: HeadingStyle = HeadingStyle(15, "vermilion"),
    val h6: HeadingStyle = HeadingStyle(14, "reddish_purple"),
) {
    fun style(level: Int): HeadingStyle = when (level) {
        1 -> h1
        2 -> h2
        3 -> h3
        4 -> h4
        5 -> h5
        else -> h6
    }

    fun with(level: Int, style: HeadingStyle): HeadingsSettings = when (level) {
        1 -> copy(h1 = style)
        2 -> copy(h2 = style)
        3 -> copy(h3 = style)
        4 -> copy(h4 = style)
        5 -> copy(h5 = style)
        else -> copy(h6 = style)
    }

    /** Compact payload for the native editor, one array entry per level. */
    fun toNativeJson(): String {
        val styles = JSONArray()
        for (level in 1..6) {
            val style = style(level)
            styles.put(
                JSONObject()
                    .put("size", style.fontSize)
                    .put("color", HeadingColors.rgb(style.colorName))
                    .put("bold", style.bold)
                    .put("italic", style.italic),
            )
        }
        return styles.toString()
    }
}

/**
 * Daily-note preferences. [format] and [folder] use the same names, defaults
 * and `{{TOKEN}}` syntax as the desktop `dailyNotes` settings block. [enabled]
 * and [template] are mobile-only extras; the desktop JSON reader ignores them.
 */
data class DailyNoteSettings(
    val enabled: Boolean = true,
    val format: String = DEFAULT_DAILY_NOTE_FORMAT,
    /** Folder relative to the vault root. Empty means the vault root. */
    val folder: String = "",
    /** Vault-relative path of the markdown note copied into a new daily note. */
    val template: String = "",
)

/**
 * Phone-shell preferences. Defaults mirror the constants the shell used before
 * settings existed, so an upgrade without a stored blob changes nothing.
 */
data class AppSettings(
    val textScale: Float = 1f,
    /** Editor body text size in dp; its line pitch is derived from it. */
    val editorFontDp: Int = AppSettingsJson.DEFAULT_EDITOR_FONT_DP,
    /** Font for Markdown prose and other non-code text. */
    val textFontFace: EditorFontFace = EditorFontFace.Mono,
    /** Font for fenced/inline code and for code files. */
    val codeFontFace: EditorFontFace = EditorFontFace.Mono,
    val autoHideChrome: Boolean = true,
    val navbarEnabled: Boolean = false,
    val defaultEditorMode: EditorMode = EditorMode.Source,
    val autoSaveDelayMs: Int = 600,
    val dailyNotes: DailyNoteSettings = DailyNoteSettings(),
    val headings: HeadingsSettings = HeadingsSettings(),
)

/**
 * Stored as one JSON blob so unknown fields can be ignored and new fields get
 * their defaults without a key-by-key migration.
 */
object AppSettingsJson {
    const val MIN_TEXT_SCALE = 0.85f
    const val MAX_TEXT_SCALE = 1.3f

    /** Editor text size range, matching the desktop `editor.fontSizeBase` steps. */
    const val MIN_EDITOR_FONT_DP = 12
    const val MAX_EDITOR_FONT_DP = 24
    const val DEFAULT_EDITOR_FONT_DP = 15

    /** Line pitch the editor uses at [DEFAULT_EDITOR_FONT_DP]. */
    private const val DEFAULT_EDITOR_LINE_DP = 22f

    /** Line pitch that keeps the text/line proportion when the size changes. */
    fun editorLineDp(fontDp: Int): Float =
        DEFAULT_EDITOR_LINE_DP * fontDp / DEFAULT_EDITOR_FONT_DP

    fun encode(settings: AppSettings): String {
        val dailyNotes = JSONObject()
            .put("enabled", settings.dailyNotes.enabled)
            .put("format", settings.dailyNotes.format)
            .put("folder", settings.dailyNotes.folder)
            .put("template", settings.dailyNotes.template)
        val headings = JSONObject()
        for (level in 1..6) {
            val style = settings.headings.style(level)
            headings.put(
                "h$level",
                JSONObject()
                    .put("fontSize", style.fontSize)
                    .put("colorName", style.colorName)
                    .put("bold", style.bold)
                    .put("italic", style.italic),
            )
        }
        return JSONObject()
            .put("textScale", settings.textScale.toDouble())
            .put("editorFontDp", settings.editorFontDp)
            .put("textFontFace", settings.textFontFace.name)
            .put("codeFontFace", settings.codeFontFace.name)
            .put("autoHideChrome", settings.autoHideChrome)
            .put("navbarEnabled", settings.navbarEnabled)
            .put("defaultEditorMode", settings.defaultEditorMode.name)
            .put("autoSaveDelayMs", settings.autoSaveDelayMs)
            .put("dailyNotes", dailyNotes)
            .put("headings", headings)
            .toString()
    }

    fun decode(raw: String?): AppSettings {
        if (raw.isNullOrBlank()) return AppSettings()
        return runCatching {
            val value = JSONObject(raw)
            AppSettings(
                textScale = value.optDouble("textScale", 1.0)
                    .toFloat()
                    .takeIf { it.isFinite() }
                    ?.coerceIn(MIN_TEXT_SCALE, MAX_TEXT_SCALE)
                    ?: 1f,
                editorFontDp = value.optInt("editorFontDp", DEFAULT_EDITOR_FONT_DP)
                    .coerceIn(MIN_EDITOR_FONT_DP, MAX_EDITOR_FONT_DP),
                textFontFace = runCatching {
                    EditorFontFace.valueOf(
                        value.optString("textFontFace", EditorFontFace.Mono.name),
                    )
                }.getOrDefault(EditorFontFace.Mono),
                codeFontFace = runCatching {
                    EditorFontFace.valueOf(
                        value.optString("codeFontFace", EditorFontFace.Mono.name),
                    )
                }.getOrDefault(EditorFontFace.Mono),
                autoHideChrome = value.optBoolean("autoHideChrome", true),
                navbarEnabled = value.optBoolean("navbarEnabled", false),
                defaultEditorMode = runCatching {
                    EditorMode.valueOf(value.optString("defaultEditorMode", EditorMode.Source.name))
                }.getOrDefault(EditorMode.Source),
                autoSaveDelayMs = value.optInt("autoSaveDelayMs", 600).coerceIn(0, 5000),
                dailyNotes = dailyNoteSettings(value.optJSONObject("dailyNotes")),
                headings = headingsSettings(value.optJSONObject("headings")),
            )
        }.getOrDefault(AppSettings())
    }

    private fun headingsSettings(value: JSONObject?): HeadingsSettings {
        val defaults = HeadingsSettings()
        if (value == null) return defaults
        return HeadingsSettings(
            h1 = headingStyle(value.optJSONObject("h1"), defaults.h1),
            h2 = headingStyle(value.optJSONObject("h2"), defaults.h2),
            h3 = headingStyle(value.optJSONObject("h3"), defaults.h3),
            h4 = headingStyle(value.optJSONObject("h4"), defaults.h4),
            h5 = headingStyle(value.optJSONObject("h5"), defaults.h5),
            h6 = headingStyle(value.optJSONObject("h6"), defaults.h6),
        )
    }

    private fun headingStyle(value: JSONObject?, defaults: HeadingStyle): HeadingStyle {
        if (value == null) return defaults
        return HeadingStyle(
            fontSize = value.optInt("fontSize", defaults.fontSize).coerceIn(6, 64),
            colorName = value.optString("colorName", defaults.colorName),
            bold = value.optBoolean("bold", defaults.bold),
            italic = value.optBoolean("italic", defaults.italic),
        )
    }

    private fun dailyNoteSettings(value: JSONObject?): DailyNoteSettings {
        val defaults = DailyNoteSettings()
        if (value == null) return defaults
        return DailyNoteSettings(
            enabled = value.optBoolean("enabled", defaults.enabled),
            format = value.optString("format", defaults.format),
            folder = value.optString("folder", defaults.folder),
            template = value.optString("template", defaults.template),
        )
    }
}

/** The file families the selective-sync switches control. */
enum class SyncFileCategory { Images, Audio, Video, Pdf, Other }

/**
 * Per-vault sync preferences. Obsidian keeps its sync filter per remote vault,
 * so these are stored under the vault key instead of [AppSettings].
 */
data class VaultSyncSettings(
    val paused: Boolean = false,
    val deviceName: String = "",
    val allowImages: Boolean = true,
    val allowAudio: Boolean = true,
    val allowVideo: Boolean = true,
    val allowPdf: Boolean = true,
    val allowOther: Boolean = true,
    val excludedFolders: List<String> = emptyList(),
    val preventSleep: Boolean = false,
) {
    fun allows(category: SyncFileCategory): Boolean = when (category) {
        SyncFileCategory.Images -> allowImages
        SyncFileCategory.Audio -> allowAudio
        SyncFileCategory.Video -> allowVideo
        SyncFileCategory.Pdf -> allowPdf
        SyncFileCategory.Other -> allowOther
    }

    fun withCategory(category: SyncFileCategory, enabled: Boolean): VaultSyncSettings =
        when (category) {
            SyncFileCategory.Images -> copy(allowImages = enabled)
            SyncFileCategory.Audio -> copy(allowAudio = enabled)
            SyncFileCategory.Video -> copy(allowVideo = enabled)
            SyncFileCategory.Pdf -> copy(allowPdf = enabled)
            SyncFileCategory.Other -> copy(allowOther = enabled)
        }
}

object VaultSyncJson {
    fun encode(settings: VaultSyncSettings): String {
        val folders = JSONArray()
        for (folder in settings.excludedFolders) folders.put(folder)
        return JSONObject()
            .put("paused", settings.paused)
            .put("deviceName", settings.deviceName)
            .put("images", settings.allowImages)
            .put("audio", settings.allowAudio)
            .put("video", settings.allowVideo)
            .put("pdf", settings.allowPdf)
            .put("other", settings.allowOther)
            .put("excludedFolders", folders)
            .put("preventSleep", settings.preventSleep)
            .toString()
    }

    fun decode(raw: String?): VaultSyncSettings {
        if (raw.isNullOrBlank()) return VaultSyncSettings()
        return runCatching {
            val value = JSONObject(raw)
            val folders = value.optJSONArray("excludedFolders") ?: JSONArray()
            VaultSyncSettings(
                paused = value.optBoolean("paused", false),
                deviceName = value.optString("deviceName", "").take(64),
                allowImages = value.optBoolean("images", true),
                allowAudio = value.optBoolean("audio", true),
                allowVideo = value.optBoolean("video", true),
                allowPdf = value.optBoolean("pdf", true),
                allowOther = value.optBoolean("other", true),
                excludedFolders = buildList {
                    for (index in 0 until folders.length()) {
                        val folder = folders.optString(index).trim().trim('/')
                        if (folder.isNotEmpty() && folder !in this) add(folder)
                    }
                },
                preventSleep = value.optBoolean("preventSleep", false),
            )
        }.getOrDefault(VaultSyncSettings())
    }
}
