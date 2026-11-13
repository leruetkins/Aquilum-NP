package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ExplorerTreeTest {
    private fun entry(path: String, note: Boolean = path.endsWith(".md")) = ExplorerEntry(
        path = path,
        name = path.substringAfterLast('/'),
        isNote = note,
        extension = path.substringAfterLast('.', "").lowercase(),
    )

    @Test
    fun buildsFoldersFirstAndKeepsEmptyFolders() {
        val tree = buildExplorerTree(
            entries = listOf(
                entry("root-note.md"),
                entry("Projects/alpha.md"),
                entry("Projects/Sub/beta.md"),
                entry("Projects/picture.png", note = false),
            ),
            folders = listOf("Projects", "Projects/Sub", "Empty"),
        )

        assertEquals(listOf("Empty", "Projects"), tree.filter { it.isFolder }.map { it.name })
        val projects = tree.first { it.name == "Projects" }
        assertEquals(listOf("Sub"), projects.children.filter { it.isFolder }.map { it.name })
        assertEquals(listOf("alpha.md", "picture.png"), projects.children.filterNot { it.isFolder }.map { it.name })
        assertTrue(tree.any { it.name == "root-note.md" && !it.isFolder })
        assertTrue(projects.children.first { it.name == "Sub" }.children.any { it.name == "beta.md" })
    }

    @Test
    fun flattenHonoursCollapsedFolders() {
        val tree = buildExplorerTree(
            entries = listOf(
                entry("Projects/alpha.md"),
                entry("Projects/Sub/beta.md"),
            ),
            folders = listOf("Projects", "Projects/Sub"),
        )

        val expanded = flattenExplorerTree(tree, emptySet())
        assertEquals(
            listOf("Projects", "Sub", "beta.md", "alpha.md"),
            expanded.map { it.node.name },
        )
        assertEquals(listOf(0, 1, 2, 1), expanded.map { it.depth })

        val collapsed = flattenExplorerTree(tree, setOf("Projects"))
        assertEquals(listOf("Projects"), collapsed.map { it.node.name })
        assertFalse(collapsed.any { it.node.name == "alpha.md" })
    }

    @Test
    fun filterKeepsMatchesAndTheirAncestorsOnly() {
        val tree = buildExplorerTree(
            entries = listOf(
                entry("root-note.md"),
                entry("Projects/alpha.md"),
                entry("Projects/Sub/beta.md"),
            ),
            folders = listOf("Projects", "Projects/Sub"),
        )

        val byFile = filterExplorerTree(tree, "beta")
        assertEquals(listOf("Projects"), byFile.map { it.name })
        assertEquals(listOf("Sub"), byFile[0].children.map { it.name })
        assertEquals(listOf("beta.md"), byFile[0].children[0].children.map { it.name })

        // A matching folder is shown as a row; unrelated children stay hidden.
        val byFolder = filterExplorerTree(tree, "projects")
        assertEquals(listOf("Projects"), byFolder.map { it.name })
        assertEquals(emptyList<String>(), byFolder[0].children.map { it.name })
        assertFalse(byFolder.any { it.name == "root-note.md" })

        val byChild = filterExplorerTree(tree, "alpha")
        assertEquals(listOf("Projects"), byChild.map { it.name })
        assertEquals(listOf("alpha.md"), byChild[0].children.map { it.name })

        assertEquals(
            setOf("Projects", "Projects/Sub"),
            filteredAncestors(byFile),
        )
        assertEquals(emptySet<String>(), filteredAncestors(byFolder))
        assertEquals(setOf("Projects"), filteredAncestors(byChild))
    }

    @Test
    fun renamePathUnderMovesOnlyTheRenamedFolderSubtree() {
        assertEquals("Archive", renamePathUnder("Projects", "Projects", "Archive"))
        assertEquals("Archive/Sub/note.md", renamePathUnder("Projects/Sub/note.md", "Projects", "Archive"))
        assertEquals("Drafts/note.md", renamePathUnder("Drafts/note.md", "Projects", "Archive"))
        assertEquals("Projects2/note.md", renamePathUnder("Projects2/note.md", "Projects", "Archive"))
        assertEquals("renamed.md", renamePathUnder("note.md", "note.md", "renamed.md"))
    }

    @Test
    fun duplicateNamesStayInTheSameFolderAndKeepExtensions() {
        assertEquals("note (copy).md", duplicatePath("note.md"))
        assertEquals("Archive/note (copy).md", duplicatePath("Archive/note.md"))
        assertEquals("Projects (copy)", duplicatePath("Projects"))
        assertEquals("noext (copy)", duplicatePath("noext"))

        assertEquals(
            "Archive/note (copy) 2.md",
            uniquePath("Archive/note (copy).md", setOf("Archive/note (copy).md")),
        )
        assertEquals("note.md", uniquePath("note.md", emptySet()))
    }

    @Test
    fun moveKeepsTheNameInTheTargetFolder() {
        assertEquals("Archive/note.md", movePathTo("Projects/note.md", "Archive"))
        assertEquals("note.md", movePathTo("Projects/note.md", ""))
        assertEquals("Archive/Projects", movePathTo("Projects", "Archive"))
    }
}
