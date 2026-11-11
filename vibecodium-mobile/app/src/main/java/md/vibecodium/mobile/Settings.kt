package md.vibecodium.mobile

import org.json.JSONObject

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
)

/**
 * Stored as one JSON blob so unknown fields can be ignored and new fields get
 * their defaults without a key-by-key migration.
 */
object AppSettingsJson {
    const val MIN_TEXT_SCALE = 0.85f
    const val MAX_TEXT_SCALE = 1.3f

    fun encode(settings: AppSettings): String = JSONObject()
        .put("textScale", settings.textScale.toDouble())
        .put("autoHideChrome", settings.autoHideChrome)
        .put("navbarEnabled", settings.navbarEnabled)
        .put("defaultEditorMode", settings.defaultEditorMode.name)
        .put("autoSaveDelayMs", settings.autoSaveDelayMs)
        .toString()

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
            )
        }.getOrDefault(AppSettings())
    }
}
