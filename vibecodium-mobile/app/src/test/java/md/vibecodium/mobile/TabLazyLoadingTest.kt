package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TabLazyLoadingTest {
    @Test
    fun defaultTabIsLoaded() {
        val tab = Tab(
            path = "Notes/demo.md",
            title = "demo",
            text = "Hello world",
        )
        assertTrue(tab.isLoaded)
        assertEquals("Hello world", tab.text)
    }

    @Test
    fun unloadedTabHasEmptyTextAndIsLoadedFalse() {
        val tab = Tab(
            path = "Notes/other.md",
            title = "other",
            text = "",
            isLoaded = false,
        )
        assertFalse(tab.isLoaded)
        assertEquals("", tab.text)
    }

    @Test
    fun loadedCopyUpdatesTextAndFlag() {
        val unloaded = Tab(
            path = "Notes/other.md",
            title = "other",
            text = "",
            isLoaded = false,
        )
        val loaded = unloaded.copy(text = "Loaded content", isLoaded = true)
        assertTrue(loaded.isLoaded)
        assertEquals("Loaded content", loaded.text)
    }
}
