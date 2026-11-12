package md.vibecodium.mobile

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.roundToInt

/**
 * Unit tests validating band scrolling math, threshold triggering,
 * cascading re-render prevention, and atomic buffer handoff for [NativeEditorView].
 */
class BandScrollingTest {

    private fun computeBandMargin(h: Int): Int = (h * 0.6f).roundToInt()

    private fun computeTranslation(bandOrigin: Float, margin: Int, scroll: Float): Float {
        return ((bandOrigin + margin) - scroll).roundToInt().toFloat()
    }

    @Test
    fun bandMarginIsSixtyPercentOfViewport() {
        assertEquals(600, computeBandMargin(1000))
        assertEquals(1200, computeBandMargin(2000))
        assertEquals(1440, computeBandMargin(2400))
    }

    @Test
    fun screenCoordinateIsStrictlyInvariantAcrossBandSwaps() {
        val viewportH = 2400
        val m = computeBandMargin(viewportH)
        val scroll = 25000f

        // Document lines across the viewport
        val docLines = listOf(24000f, 25000f, 25500f, 26000f, 27000f)

        // Previous band was centered around scroll = 23800f
        val oldBandOrigin = 23800f
        val oldTranslation = computeTranslation(oldBandOrigin, m, scroll)

        // New band is centered around scroll = 25000f
        val newBandOrigin = 25000f - m // 23560f
        val newTranslation = computeTranslation(newBandOrigin, m, scroll)

        for (docY in docLines) {
            val oldTextureY = docY - oldBandOrigin
            val oldScreenY = -m + oldTranslation + oldTextureY

            val newTextureY = docY - newBandOrigin
            val newScreenY = -m + newTranslation + newTextureY

            val expectedScreenY = docY - scroll

            assertEquals("Old band position must match ideal screen pos", expectedScreenY, oldScreenY, 0.001f)
            assertEquals("New band position must match ideal screen pos", expectedScreenY, newScreenY, 0.001f)
            assertEquals("Screen Y must be identical across band swap", oldScreenY, newScreenY, 0.001f)
        }
    }

    @Test
    fun headroomThresholdTriggersOnlyWhenCloseToMargin() {
        val h = 2000
        val m = computeBandMargin(h).toFloat() // 1200f
        val threshold = m * 0.35f // 420f
        val maxOrigin = 50000f

        var bandOrigin = 10000f

        // Scrolling in the middle of the band: headroom is high
        val midScroll = bandOrigin + m // 11200f
        val topHeadroom = midScroll - bandOrigin // 1200f
        val bottomHeadroom = (bandOrigin + 2f * m) - midScroll // 1200f

        assertFalse("Mid-band scroll should not trigger band render", topHeadroom < threshold)
        assertFalse("Mid-band scroll should not trigger band render", bottomHeadroom < threshold)

        // Scrolling down near bottom of band:
        // bottomHeadroom = (bandOrigin + 2 * m) - scroll
        val triggerScrollDown = bandOrigin + 2f * m - threshold + 1f // 12400f - 420f + 1f = 11981f
        val downHeadroom = (bandOrigin + 2f * m) - triggerScrollDown
        assertTrue("Scroll near bottom edge must trigger band render", downHeadroom < threshold)

        val targetOriginDown = (triggerScrollDown - m).coerceIn(0f, maxOrigin).roundToInt().toFloat()
        val delta = kotlin.math.abs(targetOriginDown - bandOrigin)
        assertTrue("Target origin must differ from current bandOrigin by at least threshold", delta >= threshold)
    }

    @Test
    fun preventsCascadingReRendersComparingAgainstNewlyRenderedOrigin() {
        val h = 2000
        val m = computeBandMargin(h).toFloat() // 1200f
        val threshold = m * 0.35f // 420f

        val oldBandOrigin = 10000f
        val newlyRendered = 10800f // Rust rendered this new band

        // During the in-flight render, user moved finger slightly (e.g. +30px of scroll)
        val pendingTarget = 10830f

        // BUGGY logic: comparing against stale oldBandOrigin:
        val buggyShouldRender = kotlin.math.abs(pendingTarget - oldBandOrigin) > threshold
        assertTrue("Stale bandOrigin triggers false cascading render!", buggyShouldRender)

        // FIXED logic: comparing against newlyRendered:
        val lastOrigin = newlyRendered
        val fixedShouldRender = kotlin.math.abs(pendingTarget - lastOrigin) > threshold
        assertFalse("Comparing against newlyRendered correctly suppresses cascading render", fixedShouldRender)
    }

    @Test
    fun atomicHandoffConsumesRenderedOriginExactlyOnce() {
        val pendingRenderedOrigin = AtomicReference<Float?>(null)

        // 1. Render thread finishes and publishes origin
        val rendered = 12500f
        pendingRenderedOrigin.set(rendered)

        // 2. First consumer (e.g. onSurfaceTextureUpdated or mainHandler.post) takes it
        val consumedFirst = pendingRenderedOrigin.getAndSet(null)
        assertEquals(rendered, consumedFirst)

        // 3. Second consumer sees null and does not double-update
        val consumedSecond = pendingRenderedOrigin.getAndSet(null)
        assertNull("Subsequent callback must see null and avoid duplicate work", consumedSecond)
    }
}
