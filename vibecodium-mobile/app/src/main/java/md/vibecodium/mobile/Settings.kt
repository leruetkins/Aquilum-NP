package md.vibecodium.mobile

import org.json.JSONArray
import org.json.JSONObject

private const val DEFAULT_DAILY_NOTE_FORMAT = "{{YYYY}}-{{MM}}-{{DD}}"

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
    val autoHideChrome: Boolean = true,
    val navbarEnabled: Boolean = false,
    val defaultEditorMode: EditorMode = EditorMode.Source,
    val autoSaveDelayMs: Int = 600,
    val dailyNotes: DailyNoteSettings = DailyNoteSettings(),
)

/**
 * Stored as one JSON blob so unknown fields can be ignored and new fields get
 * their defaults without a key-by-key migration.
 */
object AppSettingsJson {
    const val MIN_TEXT_SCALE = 0.85f
    const val MAX_TEXT_SCALE = 1.3f

    fun encode(settings: AppSettings): String {
        val dailyNotes = JSONObject()
            .put("enabled", settings.dailyNotes.enabled)
            .put("format", settings.dailyNotes.format)
            .put("folder", settings.dailyNotes.folder)
            .put("template", settings.dailyNotes.template)
        return JSONObject()
            .put("textScale", settings.textScale.toDouble())
            .put("autoHideChrome", settings.autoHideChrome)
            .put("navbarEnabled", settings.navbarEnabled)
            .put("defaultEditorMode", settings.defaultEditorMode.name)
            .put("autoSaveDelayMs", settings.autoSaveDelayMs)
            .put("dailyNotes", dailyNotes)
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
                autoHideChrome = value.optBoolean("autoHideChrome", true),
                navbarEnabled = value.optBoolean("navbarEnabled", false),
                defaultEditorMode = runCatching {
                    EditorMode.valueOf(value.optString("defaultEditorMode", EditorMode.Source.name))
                }.getOrDefault(EditorMode.Source),
                autoSaveDelayMs = value.optInt("autoSaveDelayMs", 600).coerceIn(0, 5000),
                dailyNotes = dailyNoteSettings(value.optJSONObject("dailyNotes")),
            )
        }.getOrDefault(AppSettings())
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
