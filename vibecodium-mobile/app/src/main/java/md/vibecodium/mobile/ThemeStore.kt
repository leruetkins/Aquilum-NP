package md.vibecodium.mobile

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Installed themes live in app-private storage, not in the vault. That keeps
 * them working for SAF vaults (many providers hide dot-folders) and avoids
 * mixing app state into the user's notes. The layout is still Obsidian's:
 *
 *   filesDir/themes/<Name>/manifest.json
 *   filesDir/themes/<Name>/theme.css
 *
 * A root-level `<Name>.css` is accepted as a legacy theme, matching Obsidian.
 */
class ThemeStore(context: Context) {
    private val root = File(context.filesDir, "themes")

    fun listInstalled(): List<InstalledTheme> = synchronized(this) {
        root.mkdirs()
        val result = mutableListOf<InstalledTheme>()
        for (dir in root.listFiles().orEmpty()) {
            if (!dir.isDirectory || dir.name.startsWith(".")) continue
            val css = File(dir, "theme.css")
            if (!css.isFile) continue
            val manifest = readManifest(File(dir, "manifest.json")) ?: continue
            if (manifest.name != dir.name) continue
            val meta = readCatalogInfo(File(dir, CATALOG_INFO))
            result += InstalledTheme(
                name = manifest.name,
                cssFile = css,
                manifest = manifest,
                entry = meta,
                legacy = meta?.legacy == true,
            )
        }
        for (file in root.listFiles().orEmpty()) {
            if (!file.isFile || file.name.startsWith(".")) continue
            if (!file.extension.equals("css", ignoreCase = true)) continue
            result += InstalledTheme(
                name = file.nameWithoutExtension,
                cssFile = file,
                manifest = null,
                legacy = true,
            )
        }
        result.sortedBy { it.name.lowercase() }
    }

    fun cssFile(name: String): File? {
        if (name.isBlank()) return null
        val dir = File(root, name)
        val themed = File(dir, "theme.css")
        if (themed.isFile) return themed
        val legacy = File(root, "$name.css")
        return if (legacy.isFile) legacy else null
    }

    fun readCss(name: String): String? = cssFile(name)?.takeIf(File::isFile)?.readText()

    /** Cheap change detector: size + mtime. */
    fun cssStamp(name: String): String {
        if (name.isBlank()) return ""
        val file = cssFile(name) ?: return "missing"
        return "${file.length()}:${file.lastModified()}"
    }

    fun configFile(name: String): File {
        val dir = File(root, name)
        return if (dir.isDirectory) File(dir, THEME_CONFIG) else File(root, "$name.vibecodium.json")
    }

    fun readConfig(name: String, dark: Boolean? = null): ThemeConfig? {
        if (name.isBlank()) return null
        val file = configFile(name)
        if (!file.isFile) return null
        val json = runCatching { JSONObject(file.readText()) }.getOrNull() ?: return null
        return ThemeConfig.fromJson(json, expectedStamp = cssStamp(name), preferredDark = dark)
    }

    fun writeConfig(name: String, config: ThemeConfig) {
        if (name.isBlank() || cssFile(name) == null) return
        val target = configFile(name)
        val parent = target.parentFile ?: return
        parent.mkdirs()
        val temp = File(parent, ".${target.name}.tmp")
        temp.writeText(config.toJson().toString())
        if (!temp.renameTo(target)) {
            target.writeText(temp.readText())
            temp.delete()
        }
    }

    fun install(entry: ThemeEntry, manifestText: String, cssText: String, legacy: Boolean) {
        val safe = safeName(entry.name)
        if (safe != entry.name) error("Theme name contains unsupported characters")
        val manifest = readManifestText(manifestText)
            ?: error("Theme manifest is not valid JSON")
        if (manifest.name != entry.name) error("Theme name mismatch")
        val dir = File(root, entry.name)
        val temp = File(root, ".${entry.name}.tmp")
        temp.deleteRecursively()
        if (!temp.mkdirs()) error("Could not create the theme folder")
        try {
            File(temp, "manifest.json").writeText(manifestText)
            File(temp, "theme.css").writeText(cssText)
            File(temp, CATALOG_INFO).writeText(catalogInfoJson(entry, legacy).toString())
            if (dir.exists()) dir.deleteRecursively()
            if (!temp.renameTo(dir)) {
                dir.mkdirs()
                File(temp, "manifest.json").copyTo(File(dir, "manifest.json"), overwrite = true)
                File(temp, "theme.css").copyTo(File(dir, "theme.css"), overwrite = true)
                File(temp, CATALOG_INFO).copyTo(File(dir, CATALOG_INFO), overwrite = true)
                temp.deleteRecursively()
            }
            File(root, "${entry.name}.css").delete()
        } catch (error: Throwable) {
            temp.deleteRecursively()
            throw error
        }
    }

    fun remove(name: String) {
        val dir = File(root, name)
        if (dir.isDirectory) dir.deleteRecursively()
        File(root, "$name.css").delete()
    }

    fun readManifest(file: File): ThemeManifest? {
        if (!file.isFile) return null
        return runCatching { readManifestText(file.readText()) }.getOrNull()
    }

    private fun readManifestText(text: String): ThemeManifest? = runCatching {
        val json = JSONObject(text)
        val name = json.optString("name").trim()
        if (name.isEmpty()) return@runCatching null
        ThemeManifest(
            name = name,
            version = json.optString("version").ifBlank { "0.0.0" },
            minAppVersion = json.optString("minAppVersion").takeIf { it.isNotBlank() },
            author = json.optString("author").takeIf { it.isNotBlank() },
            authorUrl = json.optString("authorUrl").takeIf { it.isNotBlank() },
        )
    }.getOrNull()

    private fun readCatalogInfo(file: File): ThemeEntry? {
        if (!file.isFile) return null
        return runCatching {
            val json = JSONObject(file.readText())
            ThemeEntry(
                name = json.optString("name"),
                author = json.optString("author"),
                repo = json.optString("repo"),
                screenshot = json.optString("screenshot"),
                modes = json.optJSONArray("modes").toStringList(),
                legacy = json.optBoolean("legacy"),
            )
        }.getOrNull()
    }

    private fun catalogInfoJson(entry: ThemeEntry, legacy: Boolean): JSONObject = JSONObject()
        .put("name", entry.name)
        .put("author", entry.author)
        .put("repo", entry.repo)
        .put("screenshot", entry.screenshot)
        .put("modes", JSONArray(entry.modes))
        .put("legacy", legacy)

    private fun safeName(value: String): String {
        val trimmed = value.trim()
        if (trimmed.isEmpty() || trimmed == "." || trimmed == "..") return ""
        if (trimmed.contains('/') || trimmed.contains('\\')) return ""
        if (trimmed.any { it.code < 0x20 }) return ""
        return trimmed
    }

    private fun JSONArray?.toStringList(): List<String> {
        if (this == null) return emptyList()
        return buildList {
            for (index in 0 until length()) {
                val value = optString(index).trim()
                if (value.isNotEmpty()) add(value)
            }
        }
    }

    companion object {
        private const val CATALOG_INFO = "vibecodium.json"
        private const val THEME_CONFIG = "vibecodium-theme.json"
    }
}
