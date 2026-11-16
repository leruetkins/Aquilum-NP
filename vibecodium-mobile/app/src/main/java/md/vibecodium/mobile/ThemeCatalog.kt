package md.vibecodium.mobile

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

data class ThemeInstallPayload(
    val manifestText: String,
    val cssText: String,
    val legacy: Boolean,
)

/**
 * Obsidian's community theme catalog and installer, ported to plain HTTP.
 *
 * URLs and fallbacks intentionally mirror the Obsidian 1.15 mobile client:
 * catalog -> GitHub HEAD; install -> GitHub Releases; on 404 -> raw HEAD;
 * on network failure -> releases.obsidian.md proxy.
 */
class ThemeCatalogClient(private val context: Context) {
    private val cacheFile = File(context.cacheDir, "vibecodium-theme-catalog.json")
    private val stampFile = File(context.cacheDir, "vibecodium-theme-catalog.stamp")
    private val statsPrefs = context.getSharedPreferences("vibecodium-theme-stats", Context.MODE_PRIVATE)

    @Volatile
    private var memory: List<ThemeEntry>? = null
    @Volatile
    private var memoryAt = 0L

    suspend fun catalog(force: Boolean = false): List<ThemeEntry> = withContext(Dispatchers.IO) {
        val now = System.currentTimeMillis()
        memory?.takeIf { !force && now - memoryAt < CATALOG_TTL_MS }?.let { return@withContext it }
        if (!force && cacheFile.isFile && stampFile.isFile) {
            val stamp = stampFile.readText().toLongOrNull() ?: 0L
            if (now - stamp < CATALOG_TTL_MS) {
                parseCached(cacheFile.readText())?.let {
                    memory = it
                    memoryAt = stamp
                    return@withContext it
                }
            }
        }

        val catalogText = getGithubText(CATALOG_URL)
            ?: cacheFile.takeIf(File::isFile)?.readText()?.takeIf { it.isNotBlank() }
            ?: error("Could not load community themes. Check your network.")
        val entries = parseCatalog(catalogText)
        if (entries.isEmpty()) error("Could not parse community themes.")
        val downloads = runCatching { downloadCounts() }.getOrDefault(emptyMap())
        val withStats = entries.map { entry ->
            if (downloads.containsKey(entry.name)) entry.copy(downloads = downloads.getValue(entry.name)) else entry
        }
        cacheFile.writeText(serializeCatalog(withStats).toString())
        stampFile.writeText(now.toString())
        memory = withStats
        memoryAt = now
        withStats
    }

    suspend fun install(entry: ThemeEntry): ThemeInstallPayload = withContext(Dispatchers.IO) {
        val headManifest = manifest(entry)
        val version = headManifest?.let { resolveVersion(entry, it) }
        val payload = if (headManifest == null || version == null) {
            val css = getGithubText(rawUrl(entry.repo, "obsidian.css"))
                ?: error("Theme “${entry.name}” has no installable files.")
            val synthetic = JSONObject()
                .put("name", entry.name)
                .put("version", "0.0.0")
                .put("minAppVersion", "0.16.0")
                .put("author", entry.author)
                .toString(2)
            ThemeInstallPayload(synthetic, css, legacy = true)
        } else {
            var manifestText = getGithubText(releaseUrl(entry.repo, version, "manifest.json"))
            var cssText = getGithubText(releaseUrl(entry.repo, version, "theme.css"))
            if (manifestText.isNullOrBlank() || cssText.isNullOrBlank()) {
                manifestText = getGithubText(rawUrl(entry.repo, "manifest.json"))
                cssText = getGithubText(rawUrl(entry.repo, "theme.css"))
            }
            val manifest = manifestText ?: error("Failed to install theme “${entry.name}”.")
            val css = cssText ?: error("Failed to install theme “${entry.name}”.")
            val parsed = runCatching { JSONObject(manifest) }.getOrNull()
                ?: error("Theme name mismatch.")
            if (parsed.optString("name") != entry.name) error("Theme name mismatch.")
            ThemeInstallPayload(manifest, css, legacy = false)
        }
        recordDownload(entry.name)
        payload
    }

    suspend fun updates(
        installed: List<InstalledTheme>,
        entries: List<ThemeEntry>,
    ): Map<String, String> = withContext(Dispatchers.IO) {
        val byName = entries.associateBy { it.name }
        val result = mutableMapOf<String, String>()
        for (theme in installed) {
            val entry = byName[theme.name] ?: theme.entry ?: continue
            val latest = runCatching { checkUpdate(theme, entry) }.getOrNull() ?: continue
            result[theme.name] = latest
        }
        result
    }

    private suspend fun checkUpdate(theme: InstalledTheme, entry: ThemeEntry): String? {
        val manifest = manifest(entry)
        if (manifest == null) {
            if (!theme.legacy) return null
            val remote = getGithubText(rawUrl(entry.repo, "obsidian.css")) ?: return null
            val local = runCatching { theme.cssFile.readText() }.getOrNull() ?: return null
            return if (remote != local) "legacy" else null
        }
        val latest = resolveVersion(entry, manifest) ?: return null
        if (theme.legacy) return latest
        return if (compareVersions(theme.version, latest) < 0) latest else null
    }

    private fun manifest(entry: ThemeEntry): ThemeManifest? {
        if (entry.repo.isBlank()) return null
        val text = getGithubText(rawUrl(entry.repo, "manifest.json")) ?: return null
        return parseManifest(text)?.takeIf { it.name == entry.name }
    }

    private fun resolveVersion(entry: ThemeEntry, manifest: ThemeManifest): String? {
        val minimum = manifest.minAppVersion
        if (minimum == null || compareVersions(APP_API_VERSION, minimum) >= 0) {
            return manifest.version.ifBlank { null }
        }
        val text = getGithubText(rawUrl(entry.repo, "versions.json")) ?: return null
        val json = runCatching { JSONObject(text) }.getOrNull() ?: return null
        var best: String? = null
        for (release in json.keys()) {
            val required = json.optString(release)
            if (required.isBlank() || compareVersions(APP_API_VERSION, required) < 0) continue
            if (best == null || compareVersions(release, best) > 0) best = release
        }
        return best
    }

    private fun downloadCounts(): Map<String, Int> {
        val response = request(STATS_URL) ?: return emptyMap()
        if (response.status !in 200..299) return emptyMap()
        val json = runCatching { JSONObject(response.body ?: "") }.getOrNull() ?: return emptyMap()
        val result = mutableMapOf<String, Int>()
        for (name in json.keys()) {
            val item = json.optJSONObject(name) ?: continue
            result[name] = item.optInt("download", 0)
        }
        return result
    }

    private fun recordDownload(name: String) {
        val key = "sent:$name"
        if (statsPrefs.getBoolean(key, false)) return
        statsPrefs.edit().putBoolean(key, true).apply()
        runCatching {
            request(
                url = "$STATS_URL/${URLEncoder.encode(name, "UTF-8")}/download",
                method = "POST",
                body = ByteArray(0),
            )
        }
    }

    // ------------------------------------------------------------------- HTTP

    private data class HttpResponse(val status: Int, val body: String?)

    private fun getGithubText(url: String): String? {
        val direct = runCatching { request(url) }.getOrNull()
        if (direct != null && direct.status in 200..299) return direct.body
        if (direct != null && direct.status == 404) return null
        if (!url.startsWith("https://github.com/") && !url.startsWith("https://raw.githubusercontent.com/")) {
            return null
        }
        val proxied = "https://releases.obsidian.md/proxy?url=" +
            URLEncoder.encode(url, "UTF-8")
        val response = runCatching { request(proxied) }.getOrNull() ?: return null
        return if (response.status in 200..299) response.body else null
    }

    private fun request(url: String, method: String = "GET", body: ByteArray? = null): HttpResponse? {
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = method
            connectTimeout = 15_000
            readTimeout = 25_000
            instanceFollowRedirects = true
            setRequestProperty("User-Agent", "Vibecodium/0.1 (Obsidian theme manager)")
            setRequestProperty("Accept", "application/json,text/css,text/plain,*/*")
            setRequestProperty("Accept-Encoding", "identity")
            if (body != null) {
                doOutput = true
                setFixedLengthStreamingMode(body.size)
                outputStream.use { it.write(body) }
            }
        }
        return try {
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }
            HttpResponse(status, text)
        } catch (_: Throwable) {
            null
        } finally {
            connection.disconnect()
        }
    }

    // ------------------------------------------------------------------ parse

    private fun parseCatalog(text: String): List<ThemeEntry> {
        val array = runCatching { JSONArray(text) }.getOrNull() ?: return emptyList()
        val result = ArrayList<ThemeEntry>(array.length())
        for (index in 0 until array.length()) {
            val item = array.optJSONObject(index) ?: continue
            val name = item.optString("name").trim()
            val author = item.optString("author").trim()
            val repo = item.optString("repo").trim()
            val modes = item.optJSONArray("modes").toStringList()
            if (name.isEmpty() || author.isEmpty() || repo.isEmpty() || modes.isEmpty()) continue
            result += ThemeEntry(
                name = name,
                author = author,
                repo = repo,
                screenshot = item.optString("screenshot").trim(),
                modes = modes,
                legacy = item.optBoolean("legacy"),
            )
        }
        return result
    }

    private fun parseCached(text: String): List<ThemeEntry>? {
        val array = runCatching { JSONArray(text) }.getOrNull() ?: return null
        val result = ArrayList<ThemeEntry>(array.length())
        for (index in 0 until array.length()) {
            val item = array.optJSONObject(index) ?: continue
            val name = item.optString("name").trim()
            val repo = item.optString("repo").trim()
            if (name.isEmpty() || repo.isEmpty()) continue
            result += ThemeEntry(
                name = name,
                author = item.optString("author").trim(),
                repo = repo,
                screenshot = item.optString("screenshot").trim(),
                modes = item.optJSONArray("modes").toStringList(),
                legacy = item.optBoolean("legacy"),
                downloads = item.optInt("downloads", 0),
            )
        }
        return result.takeIf { it.isNotEmpty() }
    }

    private fun serializeCatalog(entries: List<ThemeEntry>): JSONArray {
        val array = JSONArray()
        for (entry in entries) {
            array.put(
                JSONObject()
                    .put("name", entry.name)
                    .put("author", entry.author)
                    .put("repo", entry.repo)
                    .put("screenshot", entry.screenshot)
                    .put("modes", JSONArray(entry.modes))
                    .put("legacy", entry.legacy)
                    .put("downloads", entry.downloads),
            )
        }
        return array
    }

    private fun parseManifest(text: String): ThemeManifest? = runCatching {
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

    private fun JSONArray?.toStringList(): List<String> {
        if (this == null) return emptyList()
        return buildList {
            for (index in 0 until length()) {
                val value = optString(index).trim()
                if (value.isNotEmpty()) add(value)
            }
        }
    }

    private fun rawUrl(repo: String, file: String): String =
        "https://raw.githubusercontent.com/$repo/HEAD/$file"

    private fun releaseUrl(repo: String, version: String, file: String): String =
        "https://github.com/$repo/releases/download/$version/$file"

    private fun compareVersions(left: String, right: String): Int {
        val a = left.split('.').map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        val b = right.split('.').map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        val size = maxOf(a.size, b.size)
        for (index in 0 until size) {
            val x = a.getOrElse(index) { 0 }
            val y = b.getOrElse(index) { 0 }
            if (x != y) return x.compareTo(y)
        }
        return 0
    }

    companion object {
        private const val APP_API_VERSION = "1.15.0"
        private const val CATALOG_TTL_MS = 5 * 60 * 1000L
        private const val CATALOG_URL =
            "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/HEAD/community-css-themes.json"
        private const val STATS_URL = "https://releases.obsidian.md/stats/theme"
    }
}
