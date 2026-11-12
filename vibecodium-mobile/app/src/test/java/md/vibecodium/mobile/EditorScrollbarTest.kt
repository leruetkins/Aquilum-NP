package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.roundToInt

class EditorScrollbarTest {

    private fun computeMaxY(
        topInset: Float,
        lineCount: Int,
        lineHeight: Float,
        bottomInset: Float,
        viewportHeight: Float,
    ): Float {
        return (topInset + lineCount * lineHeight + bottomInset - viewportHeight).coerceAtLeast(0f)
    }

    private data class ScrollbarLayout(
        val visible: Boolean,
        val trackTop: Float,
        val trackHeight: Float,
        val thumbTop: Float,
        val thumbHeight: Float,
    )

    private fun computeScrollbarLayout(
        height: Float,
        bottomInset: Float,
        idleBottomInset: Float,
        density: Float,
        curScroll: Float,
        curMax: Float,
    ): ScrollbarLayout {
        if (curMax <= 0.5f) {
            return ScrollbarLayout(visible = false, 0f, 0f, 0f, 0f)
        }
        val trackTop = 6f * density
        val bottomOffset = if (bottomInset > idleBottomInset + 10f) bottomInset + 6f * density else 6f * density
        val trackH = (height - trackTop - bottomOffset).coerceAtLeast(1f)
        val visibleH = (height - (if (bottomInset > idleBottomInset + 10f) bottomInset else 0f)).coerceAtLeast(1f)
        val contentH = curMax + visibleH
        val ratio = (visibleH / contentH).coerceIn(0f, 1f)
        val minThumb = minOf(28f * density, trackH)
        val thumbH = (trackH * ratio).coerceIn(minThumb, trackH)
        val progress = (curScroll / curMax).coerceIn(0f, 1f)
        val thumbTop = trackTop + (trackH - thumbH) * progress
        return ScrollbarLayout(
            visible = true,
            trackTop = trackTop,
            trackHeight = trackH,
            thumbTop = thumbTop,
            thumbHeight = thumbH,
        )
    }

    private class ChromeStateSimulator(var autoHideChrome: Boolean = true) {
        var chromeHidden = false
        private var lastAnchor = 0f
        private var lastScrollVal = 0f

        fun onEditorScroll(scroll: Float, maxY: Float) {
            if (!autoHideChrome) {
                if (chromeHidden) chromeHidden = false
                lastAnchor = 0f
                lastScrollVal = scroll
            } else {
                val anchor = lastAnchor
                val prev = lastScrollVal
                lastScrollVal = scroll
                if (maxY <= 0.5f || scroll <= 2f) {
                    if (chromeHidden) chromeHidden = false
                    lastAnchor = 0f
                } else {
                    if (scroll > prev && prev < anchor) {
                        lastAnchor = prev
                    } else if (scroll < prev && prev > anchor) {
                        lastAnchor = prev
                    }
                    val currentAnchor = lastAnchor
                    val delta = scroll - currentAnchor
                    if (delta > 24f) {
                        if (!chromeHidden) chromeHidden = true
                        lastAnchor = scroll
                    } else if (delta < -24f) {
                        if (chromeHidden) chromeHidden = false
                        lastAnchor = scroll
                    }
                }
            }
        }
    }

    @Test
    fun keyboardAppearanceTriggersScrollbarForFittingNote() {
        val topInset = 60f
        val lineCount = 20
        val lineHeight = 40f
        val idleBottomInset = 64f
        val keyboardBottomInset = 400f
        val viewportH = 1000f

        // 1. Initially note fits on screen without keyboard
        val maxWithoutKeyboard = computeMaxY(topInset, lineCount, lineHeight, idleBottomInset, viewportH)
        assertEquals(0f, maxWithoutKeyboard, 0.001f)
        val layoutInitial = computeScrollbarLayout(
            height = viewportH,
            bottomInset = idleBottomInset,
            idleBottomInset = idleBottomInset,
            density = 2f,
            curScroll = 0f,
            curMax = maxWithoutKeyboard,
        )
        assertFalse("Scrollbar must be absent when note fits on screen", layoutInitial.visible)

        // 2. Keyboard appears -> bottomInset increases to 400f
        val maxWithKeyboard = computeMaxY(topInset, lineCount, lineHeight, keyboardBottomInset, viewportH)
        assertTrue("Max Y must be positive when keyboard reduces available height", maxWithKeyboard > 0.5f)
        assertEquals(260f, maxWithKeyboard, 0.001f)

        val layoutWithKeyboard = computeScrollbarLayout(
            height = viewportH,
            bottomInset = keyboardBottomInset,
            idleBottomInset = idleBottomInset,
            density = 2f,
            curScroll = 0f,
            curMax = maxWithKeyboard,
        )
        assertTrue("Scrollbar must be visible immediately with keyboard open", layoutWithKeyboard.visible)
        // Scrollbar track must end above the keyboard
        val trackBottom = layoutWithKeyboard.trackTop + layoutWithKeyboard.trackHeight
        assertTrue(
            "Scrollbar track must not extend into keyboard area",
            trackBottom <= viewportH - keyboardBottomInset,
        )

        // 3. Keyboard dismissed -> bottomInset returns to idleBottomInset
        val maxAfterDismiss = computeMaxY(topInset, lineCount, lineHeight, idleBottomInset, viewportH)
        assertEquals(0f, maxAfterDismiss, 0.001f)
        val layoutDismissed = computeScrollbarLayout(
            height = viewportH,
            bottomInset = idleBottomInset,
            idleBottomInset = idleBottomInset,
            density = 2f,
            curScroll = 0f,
            curMax = maxAfterDismiss,
        )
        assertFalse("Scrollbar must disappear when keyboard is dismissed", layoutDismissed.visible)
    }

    @Test
    fun toolbarIsShownWheneverScrollbarIsAbsent() {
        val sim = ChromeStateSimulator(autoHideChrome = true)

        // User was scrolled down in a long note, so chrome became hidden
        sim.onEditorScroll(scroll = 50f, maxY = 500f)
        sim.onEditorScroll(scroll = 150f, maxY = 500f)
        assertTrue("Chrome should be hidden after scrolling down", sim.chromeHidden)

        // Keyboard dismissed or note switched to a short note (maxY <= 0.5f)
        sim.onEditorScroll(scroll = 0f, maxY = 0f)
        assertFalse("Toolbar must be shown whenever scrollbar is absent (maxY <= 0.5f)", sim.chromeHidden)

        // Scrollbar appears again with keyboard (maxY = 200f)
        sim.onEditorScroll(scroll = 0f, maxY = 200f)
        assertFalse("Toolbar remains visible at scroll = 0", sim.chromeHidden)

        // User scrolls down again with keyboard
        sim.onEditorScroll(scroll = 40f, maxY = 200f)
        assertTrue("Toolbar hides after scrolling down", sim.chromeHidden)

        // Keyboard closes, note now fits on screen -> recalculate yields maxY = 0f
        sim.onEditorScroll(scroll = 0f, maxY = 0f)
        assertFalse("Toolbar must be shown immediately after keyboard dismissal if content fits", sim.chromeHidden)
    }

    @Test
    fun switchingNotesUpdatesScrollbarAndToolbarImmediately() {
        val sim = ChromeStateSimulator(autoHideChrome = true)

        // In long note, toolbar hidden after scroll
        sim.onEditorScroll(scroll = 100f, maxY = 800f)
        assertTrue(sim.chromeHidden)

        // Switch to short note (maxY = 0f)
        sim.onEditorScroll(scroll = 0f, maxY = 0f)
        assertFalse("Toolbar must be shown immediately upon switching to note with no scrollbar", sim.chromeHidden)

        val layout = computeScrollbarLayout(
            height = 1000f,
            bottomInset = 64f,
            idleBottomInset = 64f,
            density = 2f,
            curScroll = 0f,
            curMax = 0f,
        )
        assertFalse("Scrollbar must not be visible for short note", layout.visible)
    }
}
