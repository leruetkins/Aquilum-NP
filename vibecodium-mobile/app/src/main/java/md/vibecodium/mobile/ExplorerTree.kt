package md.vibecodium.mobile

/** A folder or file node in the explorer tree. */
data class ExplorerNode(
    val name: String,
    val path: String,
    val isFolder: Boolean,
    val entry: ExplorerEntry?,
    val children: List<ExplorerNode>,
)

/** One visible row after applying the collapsed-folder set. */
data class ExplorerRow(val node: ExplorerNode, val depth: Int)

/**
 * Builds the folder tree from flat repository data. Folders are included even
 * when they contain no files, and folders sort before files at every level.
 */
fun buildExplorerTree(
    entries: List<ExplorerEntry>,
    folders: List<String>,
): List<ExplorerNode> {
    val root = MutableNode(name = "", path = "", isFolder = true)

    fun ensureFolder(path: String): MutableNode {
        var current = root
        var built = ""
        for (part in path.split('/').filter { it.isNotEmpty() }) {
            built = if (built.isEmpty()) part else "$built/$part"
            current = current.folder(part, built)
        }
        return current
    }

    for (folder in folders) {
        if (folder.isNotBlank()) ensureFolder(folder)
    }

    for (entry in entries) {
        val parts = entry.path.split('/').filter { it.isNotEmpty() }
        if (parts.isEmpty()) continue
        val parent = ensureFolder(parts.dropLast(1).joinToString("/"))
        parent.file(parts.last(), entry.path, entry)
    }

    return root.freeze()
}

/**
 * Keeps files whose name or path matches [query] plus the folders needed to
 * reach them. A matching folder keeps its whole subtree, which is what a file
 * picker normally does when the user searches for a folder.
 */
fun filterExplorerTree(tree: List<ExplorerNode>, query: String): List<ExplorerNode> {
    val needle = query.trim().lowercase()
    if (needle.isEmpty()) return tree

    fun filter(nodes: List<ExplorerNode>): List<ExplorerNode> = nodes.mapNotNull { node ->
        val selfMatch = if (node.isFolder) {
            node.name.lowercase().contains(needle)
        } else {
            node.path.lowercase().contains(needle)
        }
        val children = if (selfMatch && node.isFolder) node.children else filter(node.children)
        if (selfMatch || children.isNotEmpty()) {
            node.copy(children = children)
        } else {
            null
        }
    }

    return filter(tree)
}

/** Depth-first rows, skipping children of collapsed folders. */
fun flattenExplorerTree(
    tree: List<ExplorerNode>,
    collapsedFolders: Set<String>,
): List<ExplorerRow> {
    val rows = mutableListOf<ExplorerRow>()

    fun visit(nodes: List<ExplorerNode>, depth: Int) {
        for (node in nodes) {
            rows.add(ExplorerRow(node, depth))
            if (node.isFolder && node.path !in collapsedFolders) {
                visit(node.children, depth + 1)
            }
        }
    }

    visit(tree, 0)
    return rows
}

private class MutableNode(
    val name: String,
    val path: String,
    val isFolder: Boolean,
    val entry: ExplorerEntry? = null,
    val children: MutableList<MutableNode> = mutableListOf(),
) {
    private val folders = HashMap<String, MutableNode>()
    private val files = HashSet<String>()

    fun folder(name: String, path: String): MutableNode {
        folders[name]?.let { return it }
        val node = MutableNode(name = name, path = path, isFolder = true)
        folders[name] = node
        children.add(node)
        return node
    }

    fun file(name: String, path: String, entry: ExplorerEntry) {
        if (!files.add(name)) return
        children.add(
            MutableNode(
                name = name,
                path = path,
                isFolder = false,
                entry = entry,
            ),
        )
    }

    fun freeze(): List<ExplorerNode> = children
        .sortedWith(
            compareByDescending<MutableNode> { it.isFolder }
                .thenBy { it.name.lowercase() },
        )
        .map { child ->
            ExplorerNode(
                name = child.name,
                path = child.path,
                isFolder = child.isFolder,
                entry = child.entry,
                children = child.freeze(),
            )
        }
}
