package md.vibecodium.mobile

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Owns installed themes, the community catalog and CSS resolution. All file
 * and network work is suspend/IO; the controller only stores the result.
 */
class ThemeManager(context: Context) {
    private val store = ThemeStore(context)
    private val catalog = ThemeCatalogClient(context)

    suspend fun installed(): List<InstalledTheme> = withContext(Dispatchers.IO) {
        store.listInstalled()
    }

    suspend fun catalog(force: Boolean = false): List<ThemeEntry> = catalog.catalog(force)

    suspend fun install(entry: ThemeEntry): InstalledTheme = withContext(Dispatchers.IO) {
        val payload = catalog.install(entry)
        store.install(entry, payload.manifestText, payload.cssText, payload.legacy)
        // Parse once here, at install time, so every later start only reads
        // the small config file. A failed config write is not fatal: the next
        // ensureConfig call will rebuild it.
        runCatching { store.writeConfig(entry.name, buildConfig(entry.name)) }
        store.listInstalled().firstOrNull { it.name == entry.name }
            ?: error("Theme “${entry.name}” was installed but could not be read back.")
    }

    suspend fun remove(name: String) = withContext(Dispatchers.IO) {
        store.remove(name)
    }

    suspend fun updates(
        installed: List<InstalledTheme>,
        entries: List<ThemeEntry>,
    ): Map<String, String> = catalog.updates(installed, entries)

    fun readCss(name: String): String? = store.readCss(name)

    /** Cheap change detector for config invalidation: size + mtime. */
    fun cssStamp(name: String): String = store.cssStamp(name)

    /** Synchronous read of the pre-parsed theme artifact (warm start). */
    fun readConfig(name: String, dark: Boolean? = null): ThemeConfig? =
        store.readConfig(name, dark)

    /** Builds the artifact from theme.css; called on install or first use. */
    fun buildConfig(name: String): ThemeConfig {
        val css = if (name.isBlank()) "" else store.readCss(name)
        val stamp = store.cssStamp(name)
        val warnings = mutableListOf<String>()
        if (name.isNotBlank() && css == null) {
            warnings += "Theme “$name” is missing its theme.css; using the built-in palette."
        }
        if (css.isNullOrBlank()) {
            return ThemeConfig(
                schema = ThemeConfig.SCHEMA,
                cssStamp = stamp,
                warnings = warnings,
                phoneLight = ThemePalette.defaultLight(),
                phoneDark = ThemePalette.defaultDark(),
                tabletLight = ThemePalette.defaultLight(),
                tabletDark = ThemePalette.defaultDark(),
            )
        }
        val phoneLight = ThemeCss.resolve(css, dark = false, isTablet = false, isPhone = true).also {
            warnings += it.warnings
        }.palette
        val phoneDark = ThemeCss.resolve(css, dark = true, isTablet = false, isPhone = true).also {
            warnings += it.warnings
        }.palette
        val tabletLight = ThemeCss.resolve(css, dark = false, isTablet = true, isPhone = false).also {
            warnings += it.warnings
        }.palette
        val tabletDark = ThemeCss.resolve(css, dark = true, isTablet = true, isPhone = false).also {
            warnings += it.warnings
        }.palette
        return ThemeConfig(
            schema = ThemeConfig.SCHEMA,
            cssStamp = stamp,
            warnings = collapseWarnings(warnings),
            phoneLight = phoneLight,
            phoneDark = phoneDark,
            tabletLight = tabletLight,
            tabletDark = tabletDark,
        )
    }

    /**
     * Returns the parsed artifact, rebuilding and persisting it when the
     * theme.css stamp or schema is stale.
     */
    suspend fun ensureConfig(name: String, dark: Boolean? = null): ThemeConfig = withContext(Dispatchers.IO) {
        store.readConfig(name, dark) ?: buildConfig(name).also { config ->
            if (name.isNotBlank() && store.cssFile(name) != null) {
                store.writeConfig(name, config)
            }
        }
    }

    companion object {
        /**
         * Light and dark resolution report the same selector rule counts with
         * slightly different numbers. Keep one line with the larger count so
         * the Appearance section stays readable.
         */
        fun collapseWarnings(warnings: List<String>): List<String> {
            var skipped = 0
            val other = LinkedHashSet<String>()
            for (warning in warnings) {
                val match = Regex("(\\d+) rule[s]? target Obsidian-only selectors").find(warning)
                if (match != null) {
                    skipped = maxOf(skipped, match.groupValues[1].toIntOrNull() ?: 0)
                } else {
                    other += warning
                }
            }
            if (skipped > 0) {
                other += "$skipped rules target Obsidian-only selectors and were skipped."
            }
            return other.toList()
        }
    }
}
