package md.vibecodium.mobile

import org.json.JSONArray
import org.json.JSONObject

data class NoteItem(
    val path: String,
    val name: String,
    val basename: String,
    val extension: String,
)

data class SearchHit(
    val path: String,
    val name: String,
    val line: Int,
    val snippet: String,
)

data class Heading(val level: Int, val text: String, val line: Int)

data class MarkdownSpan(
    val kind: String,
    /** Char offsets relative to [MarkdownLine.content]. */
    val start: Int,
    val end: Int,
)

/** One source line classified by the shared Rust markdown engine. */
data class MarkdownLine(
    val raw: String,
    val kind: String,
    val level: Int = 0,
    val indent: Int = 0,
    val marker: String = "",
    val content: String = "",
    val checked: Boolean? = null,
    val num: String? = null,
    val language: String? = null,
    val isOpening: Boolean? = null,
    val blockId: Int? = null,
    val callout: String? = null,
    val prefixChars: Int = 0,
    val spans: List<MarkdownSpan> = emptyList(),
    val cells: List<String> = emptyList(),
    val target: String? = null,
    val alt: String? = null,
)

data class DocStats(val lines: Int, val words: Int, val chars: Int)

/**
 * Above this size the source view opens without live markdown spans; the
 * preview builds them only when the user explicitly asks for it.
 */
const val LIVE_MODEL_CHAR_LIMIT = 120_000
const val LIVE_MODEL_LINE_LIMIT = 4_000

/** One-shot result of opening a note: stats, outline and the shared line model. */
data class MarkdownAnalysis(
    val stats: DocStats,
    val headings: List<Heading>,
    val lines: List<MarkdownLine>,
)

data class ExplorerEntry(
    val path: String,
    val name: String,
    val isNote: Boolean,
    val extension: String,
    val isCode: Boolean = false,
)

data class OpenVaultInfo(val name: String, val root: String, val notes: Int)

data class Tab(
    val path: String,
    val title: String,
    val text: String,
    val dirty: Boolean = false,
    val selection: Int = 0,
    /** Null for markdown notes; a language id for code files. */
    val language: String? = null,
    /** SAF document URI, persisted so a tab reopens without walking the tree. */
    val uri: String? = null,
)

enum class Drawer { Left, Right }

enum class DrawerMode { Files, Search, Bookmarks, Recent }

enum class EditorMode { Source, Preview }

data class SyncUiState(
    val connected: Boolean = false,
    val accountName: String = "",
    val clientId: String = "",
    val busy: Boolean = false,
    val progress: String = "",
    val message: String = "",
    val lastSync: Long = 0L,
    val autoSync: Boolean = false,
)

data class UiState(
    val vaultName: String? = null,
    val vaultRoot: String? = null,
    val vaultKind: String? = null,
    val notes: List<NoteItem> = emptyList(),
    val entries: List<ExplorerEntry> = emptyList(),
    /** Prebuilt tree, so opening the Files drawer never rebuilds it. */
    val explorerTree: List<ExplorerNode> = emptyList(),
    val folders: List<String> = emptyList(),
    val collapsedFolders: Set<String> = emptySet(),
    /** Instant file-name filter in the Files drawer. */
    val fileQuery: String = "",
    val tabs: List<Tab> = emptyList(),
    val activeTab: Int = -1,
    val bookmarks: Set<String> = emptySet(),
    val editorMode: EditorMode = EditorMode.Source,
    val lines: List<MarkdownLine> = emptyList(),
    val markdownText: String = "",
    val drawer: Drawer? = null,
    val drawerMode: DrawerMode = DrawerMode.Files,
    val tabSwitcher: Boolean = false,
    val syncOpen: Boolean = false,
    val sync: SyncUiState = SyncUiState(),
    val searchQuery: String = "",
    val searchHits: List<SearchHit> = emptyList(),
    val headings: List<Heading> = emptyList(),
    val stats: DocStats = DocStats(0, 0, 0),
    val history: List<String> = emptyList(),
    val historyIndex: Int = -1,
    val deletePath: String? = null,
    val viewerPath: String? = null,
    val renamePath: String? = null,
    val renameValue: String = "",
    val loading: Boolean = false,
    val message: String? = null,
)

/** Parses the JSON envelopes produced by `vibecodium-core`. */
object CoreJson {
    private fun envelope(raw: String): JSONObject {
        val objectValue = JSONObject(raw)
        if (!objectValue.optBoolean("ok")) {
            throw IllegalStateException(objectValue.optString("error", "core error"))
        }
        return objectValue
    }

    fun data(raw: String): JSONObject = envelope(raw).getJSONObject("data")

    fun openInfo(raw: String): OpenVaultInfo {
        val value = data(raw)
        return OpenVaultInfo(
            name = value.optString("name", "Vault"),
            root = value.optString("root"),
            notes = value.optInt("notes"),
        )
    }

    fun notes(raw: String): List<NoteItem> {
        val array = data(raw).optJSONArray("notes") ?: JSONArray()
        return List(array.length()) { index ->
            val value = array.getJSONObject(index)
            NoteItem(
                path = value.optString("path"),
                name = value.optString("name"),
                basename = value.optString("basename"),
                extension = value.optString("extension"),
            )
        }
    }

    fun noteText(raw: String): String = data(raw).optString("text")

    fun matches(raw: String): List<SearchHit> {
        val array = data(raw).optJSONArray("matches") ?: JSONArray()
        return List(array.length()) { index ->
            val value = array.getJSONObject(index)
            SearchHit(
                path = value.optString("path"),
                name = value.optString("name"),
                line = value.optInt("line"),
                snippet = value.optString("snippet"),
            )
        }
    }

    fun headings(raw: String): List<Heading> = headings(data(raw))

    fun stats(raw: String): DocStats = stats(data(raw))

    fun markdown(raw: String): List<MarkdownLine> = markdownLines(data(raw))

    fun analyze(raw: String): MarkdownAnalysis {
        val value = data(raw)
        return MarkdownAnalysis(
            stats = stats(value.getJSONObject("stats")),
            headings = headings(value),
            lines = markdownLines(value),
        )
    }

    private fun headings(value: JSONObject): List<Heading> {
        val array = value.optJSONArray("headings") ?: JSONArray()
        return List(array.length()) { index ->
            val heading = array.getJSONObject(index)
            Heading(
                level = heading.optInt("level", 1),
                text = heading.optString("text"),
                line = heading.optInt("line"),
            )
        }
    }

    private fun stats(value: JSONObject): DocStats = DocStats(
        lines = value.optInt("lines"),
        words = value.optInt("words"),
        chars = value.optInt("chars"),
    )

    private fun markdownLines(value: JSONObject): List<MarkdownLine> {
        val array = value.optJSONArray("lines") ?: JSONArray()
        return List(array.length()) { index ->
            val line = array.getJSONObject(index)
            val spansRaw = line.optString("spans")
            val spans = if (spansRaw.isEmpty()) {
                emptyList()
            } else {
                spansRaw.split(';').mapNotNull { token ->
                    val parts = token.split(':')
                    if (parts.size != 3) {
                        null
                    } else {
                        MarkdownSpan(
                            kind = parts[0],
                            start = parts[1].toIntOrNull() ?: 0,
                            end = parts[2].toIntOrNull() ?: 0,
                        )
                    }
                }
            }
            val cellsArray = line.optJSONArray("cells") ?: JSONArray()
            val cells = List(cellsArray.length()) { cellIndex ->
                cellsArray.optString(cellIndex)
            }
            MarkdownLine(
                raw = line.optString("raw"),
                kind = line.optString("kind"),
                level = line.optInt("level"),
                indent = line.optInt("indent"),
                marker = line.optString("marker"),
                content = line.optString("content"),
                checked = if (line.isNull("checked")) null else line.optBoolean("checked"),
                num = nullableString(line, "num"),
                language = nullableString(line, "language"),
                isOpening = if (line.isNull("is_opening")) null else line.optBoolean("is_opening"),
                blockId = if (line.isNull("block_id")) null else line.optInt("block_id"),
                callout = nullableString(line, "callout"),
                prefixChars = line.optInt("prefix_chars"),
                spans = spans,
                cells = cells,
                target = nullableString(line, "target"),
                alt = nullableString(line, "alt"),
            )
        }
    }
}

private fun nullableString(value: JSONObject, key: String): String? =
    if (value.isNull(key)) null else value.optString(key).takeIf { it.isNotBlank() }
