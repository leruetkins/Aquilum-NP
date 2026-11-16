package md.vibecodium.mobile

import org.json.JSONArray
import org.json.JSONObject

/**
 * Parsed theme artifact. Generated once per installed theme (and regenerated
 * when `theme.css` changes or the schema evolves), so normal startup only
 * reads a few kilobytes of JSON and never runs the CSS parser.
 */
data class ThemeConfig(
    val schema: Int,
    val cssStamp: String,
    val warnings: List<String>,
    val phoneLight: ThemePalette,
    val phoneDark: ThemePalette,
    val tabletLight: ThemePalette,
    val tabletDark: ThemePalette,
) {
    fun palette(dark: Boolean, isTablet: Boolean): ThemePalette = when {
        isTablet && dark -> tabletDark
        isTablet -> tabletLight
        dark -> phoneDark
        else -> phoneLight
    }

    fun toJson(): JSONObject = JSONObject()
        .put("schema", schema)
        .put("cssStamp", cssStamp)
        .put("warnings", JSONArray(warnings))
        .put(
            "phone",
            JSONObject()
                .put("light", phoneLight.toCacheJson())
                .put("dark", phoneDark.toCacheJson()),
        )
        .put(
            "tablet",
            JSONObject()
                .put("light", tabletLight.toCacheJson())
                .put("dark", tabletDark.toCacheJson()),
        )

    companion object {
        const val SCHEMA = 1

        fun fromJson(
            json: JSONObject,
            expectedStamp: String? = null,
            preferredDark: Boolean? = null,
        ): ThemeConfig? = runCatching {
            if (json.optInt("schema", 0) != SCHEMA) return@runCatching null
            val stamp = json.optString("cssStamp")
            if (expectedStamp != null && stamp != expectedStamp) return@runCatching null
            val phone = json.optJSONObject("phone") ?: return@runCatching null
            val tablet = json.optJSONObject("tablet") ?: return@runCatching null
            fun palette(parent: JSONObject, key: String): ThemePalette =
                ThemePalette.fromCacheJson(parent.optJSONObject(key) ?: error("missing $key"))
                    ?: error("invalid $key")
            val warningsJson = json.optJSONArray("warnings")
            val warnings = buildList {
                if (warningsJson != null) {
                    for (index in 0 until warningsJson.length()) {
                        warningsJson.optString(index).takeIf { it.isNotBlank() }?.let(::add)
                    }
                }
            }
            ThemeConfig(
                schema = SCHEMA,
                cssStamp = stamp,
                warnings = warnings,
                phoneLight = if (preferredDark != true) {
                    palette(phone, "light")
                } else {
                    ThemePalette.defaultLight()
                },
                phoneDark = if (preferredDark != false) {
                    palette(phone, "dark")
                } else {
                    ThemePalette.defaultDark()
                },
                tabletLight = if (preferredDark != true) {
                    palette(tablet, "light")
                } else {
                    ThemePalette.defaultLight()
                },
                tabletDark = if (preferredDark != false) {
                    palette(tablet, "dark")
                } else {
                    ThemePalette.defaultDark()
                },
            )
        }.getOrNull()
    }
}
