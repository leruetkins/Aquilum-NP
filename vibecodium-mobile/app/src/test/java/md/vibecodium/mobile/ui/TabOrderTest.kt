package md.vibecodium.mobile.ui

import md.vibecodium.mobile.Tab
import org.junit.Assert.assertEquals
import org.junit.Test

class TabOrderTest {
    private fun tab(path: String) = Tab(path = path, title = path, text = "")

    @Test
    fun newestTabComesFirst() {
        val tabs = listOf(tab("a"), tab("b"), tab("c"))
        assertEquals(listOf(2, 1, 0), newestFirstTabOrder(tabs))
    }

    @Test
    fun newlyOpenedTabGoesToTheTop() {
        val tabs = listOf(tab("a"), tab("b"))
        val withNew = tabs + tab("c")
        assertEquals(listOf(2, 1, 0), newestFirstTabOrder(withNew))
    }

    @Test
    fun emptyTabListHasNoOrder() {
        assertEquals(emptyList<Int>(), newestFirstTabOrder(emptyList()))
    }
}
