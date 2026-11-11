package md.vibecodium.mobile

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

data class SavedTab(
    val path: String,
    val selection: Int,
    val uri: String? = null,
    val mode: EditorMode = EditorMode.Source,
)

data class SavedTabs(val tabs: List<SavedTab>, val active: Int)

object TabStore {
    fun save(context: Context, key: String, tabs: List<Tab>, active: Int) {
        val array = JSONArray()
        for (tab in tabs) {
            val value = JSONObject()
                .put("path", tab.path)
                .put("selection", tab.selection)
                .put("mode", tab.mode.name)
            tab.uri?.let { value.put("uri", it) }
            array.put(value)
        }
        val root = JSONObject().put("active", active).put("tabs", array)
        prefs(context).edit().putString("tabs:$key", root.toString()).apply()
    }

    fun load(context: Context, key: String): SavedTabs? {
        val raw = prefs(context).getString("tabs:$key", null) ?: return null
        return runCatching {
            val root = JSONObject(raw)
            val array = root.optJSONArray("tabs") ?: JSONArray()
            val tabs = List(array.length()) { index ->
                val value = array.getJSONObject(index)
                SavedTab(
                    path = value.getString("path"),
                    selection = value.optInt("selection", 0),
                    uri = value.optString("uri").takeIf { it.isNotBlank() },
                    mode = runCatching {
                        EditorMode.valueOf(value.optString("mode", EditorMode.Source.name))
                    }.getOrDefault(EditorMode.Source),
                )
            }
            SavedTabs(tabs, root.optInt("active", 0))
        }.getOrNull()
    }

    fun clear(context: Context, key: String) {
        prefs(context).edit().remove("tabs:$key").apply()
    }
}

object BookmarkStore {
    fun load(context: Context, key: String): Set<String> {
        val raw = prefs(context).getString("bookmarks:$key", null) ?: return emptySet()
        return runCatching {
            val array = JSONArray(raw)
            buildSet {
                for (index in 0 until array.length()) {
                    add(array.getString(index))
                }
            }
        }.getOrDefault(emptySet())
    }

    fun save(context: Context, key: String, paths: Set<String>) {
        val array = JSONArray()
        for (path in paths) {
            array.put(path)
        }
        prefs(context).edit().putString("bookmarks:$key", array.toString()).apply()
    }
}

/** Collapsed folders, so the tree keeps its shape between launches. */
object FolderStore {
    fun load(context: Context, key: String): Set<String> {
        val raw = prefs(context).getString("folders:$key", null) ?: return emptySet()
        return runCatching {
            val array = JSONArray(raw)
            buildSet {
                for (index in 0 until array.length()) {
                    add(array.getString(index))
                }
            }
        }.getOrDefault(emptySet())
    }

    fun save(context: Context, key: String, collapsed: Set<String>) {
        val array = JSONArray()
        for (path in collapsed) {
            array.put(path)
        }
        prefs(context).edit().putString("folders:$key", array.toString()).apply()
    }
}

/** Recently opened notes, most recent first, persisted per vault. */
object RecentStore {
    private const val PREFIX = "recent:"
    private const val LIMIT = 50

    fun load(context: Context, key: String): List<String> {
        val raw = prefs(context).getString(PREFIX + key, null) ?: return emptyList()
        return runCatching {
            val array = JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val path = array.optString(index)
                    if (path.isNotBlank() && path !in this) add(path)
                }
            }.take(LIMIT)
        }.getOrDefault(emptyList())
    }

    fun remember(context: Context, key: String, path: String): List<String> {
        if (path.isBlank()) return load(context, key)
        val updated = (listOf(path) + load(context, key).filterNot { it == path }).take(LIMIT)
        save(context, key, updated)
        return updated
    }

    fun rename(context: Context, key: String, oldPath: String, newPath: String): List<String> {
        val updated = load(context, key)
            .map { if (it == oldPath) newPath else it }
            .distinct()
            .take(LIMIT)
        save(context, key, updated)
        return updated
    }

    private fun save(context: Context, key: String, paths: List<String>) {
        val array = JSONArray()
        for (path in paths) array.put(path)
        prefs(context).edit().putString(PREFIX + key, array.toString()).apply()
    }
}

/** App-wide preferences, stored as one JSON blob under a stable key. */
object SettingsStore {
    private const val KEY = "settings:global"

    fun load(context: Context): AppSettings =
        AppSettingsJson.decode(prefs(context).getString(KEY, null))

    fun save(context: Context, settings: AppSettings) {
        prefs(context).edit().putString(KEY, AppSettingsJson.encode(settings)).apply()
    }
}

/** Per-vault sync filter and device preferences. */
object VaultSyncStore {
    private const val PREFIX = "sync-settings:"

    fun load(context: Context, key: String): VaultSyncSettings =
        VaultSyncJson.decode(prefs(context).getString(PREFIX + key, null))

    fun save(context: Context, key: String, settings: VaultSyncSettings) {
        prefs(context).edit().putString(PREFIX + key, VaultSyncJson.encode(settings)).apply()
    }
}

internal fun prefs(context: Context) =
    context.getSharedPreferences("vibecodium", Context.MODE_PRIVATE)
