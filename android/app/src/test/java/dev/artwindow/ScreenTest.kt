package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenTest {

    @Test
    fun `a phone-shaped painting is placed with a centred, screen-sized crop`() {
        val screen = Screen(1080, 2340)
        // width/height 1104x2400 has aspect 0.46, just off the screen's own 0.4615.
        val placement = screen.place(1104, 2400)

        assertTrue(placement != null)
        placement!!
        assertEquals(1080, placement.scaledWidth)
        assertEquals(2348, placement.scaledHeight)
        assertEquals(1080, placement.crop.right - placement.crop.left)
        assertEquals(2340, placement.crop.bottom - placement.crop.top)
        // No horizontal trim was needed, so the crop is flush left; the vertical
        // sliver removed by the crop is split evenly above and below.
        assertEquals(0, placement.crop.left)
        assertEquals(4, placement.crop.top)
    }

    @Test
    fun `a landscape painting is refused`() {
        val screen = Screen(1080, 2340)
        assertNull(screen.place(2000, 1000))
    }

    @Test
    fun `a painting needing more than 15 percent trim is refused`() {
        val screen = Screen(1080, 2340)
        // aspect 0.35 against the screen's 0.4615 trims about 24%, well past MAX_TRIM.
        // Large enough in pixels that this fails on trim alone, not enlargement.
        assertNull(screen.place(1400, 4000))
    }

    @Test
    fun `a painting needing more than 1_25x enlargement is refused`() {
        val screen = Screen(1080, 2340)
        // Same aspect as the screen (1080:2340 reduced by 5), so trim is zero — only
        // the 5x enlargement this tiny image would need can be why this is refused.
        assertNull(screen.place(216, 468))
    }

    @Test
    fun `holds judges proportions alone, so a small web copy can answer for its original`() {
        val screen = Screen(1080, 2392)
        // 0.47 is the shape of a real Met scroll that fits; 0.67 is a triptych photographed whole.
        assertTrue(screen.holds(0.47))
        assertTrue(!screen.holds(0.67))
        // place() refuses a 400-pixel copy on enlargement, which holds() never asks about.
        assertNull(screen.place(188, 400))
    }

    @Test
    fun `a screen keeps the orientation it is given`() {
        val landscape = Screen(3840, 2160)
        assertEquals(3840, landscape.width)
        assertEquals(2160, landscape.height)
        assertTrue(landscape.isLandscape)
        assertTrue(!Screen(1080, 2340).isLandscape)
        assertTrue(!Screen(1080, 1080).isLandscape)
    }

    @Test
    fun `a landscape screen holds wide paintings and refuses tall ones`() {
        val tv = Screen(3840, 2160)
        assertTrue(tv.holds(1.7))
        assertTrue(!tv.holds(0.6))
        assertTrue(tv.place(4000, 2250) != null)
        assertNull(tv.place(2000, 4000))
    }

    @Test
    fun `fit letterboxes a tall painting between the sides of a landscape screen`() {
        val placement = Screen(3840, 2160).fit(2000, 4000)

        assertTrue(placement != null)
        placement!!
        assertEquals(1080, placement.scaledWidth)
        assertEquals(2160, placement.scaledHeight)
        assertEquals(1380, placement.destination.left)
        assertEquals(2460, placement.destination.right)
    }

    @Test
    fun `cover accepts a horizontal painting when the user allows cropping`() {
        val placement = Screen(1080, 2340).cover(4000, 2000)

        assertTrue(placement != null)
        placement!!
        assertEquals(1080, placement.crop.right - placement.crop.left)
        assertEquals(2340, placement.crop.bottom - placement.crop.top)
        assertTrue(placement.crop.left > 0)
    }

    @Test
    fun `fit centers a complete horizontal painting with top and bottom space`() {
        val placement = Screen(1080, 2340).fit(4000, 2000)

        assertTrue(placement != null)
        placement!!
        assertEquals(1080, placement.scaledWidth)
        assertEquals(540, placement.scaledHeight)
        assertEquals(900, placement.destination.top)
        assertEquals(1440, placement.destination.bottom)
    }

    @Test
    fun `stretch still protects against excessive enlargement`() {
        assertTrue(Screen(1080, 2340).canStretch(1080, 2340))
        assertTrue(!Screen(1080, 2340).canStretch(200, 400))
    }

    private val phone = Screen(1080, 2340)

    @Test
    fun `at zoom 1 and centred the frame is today's cover and fit`() {
        val cover = phone.cover(4000, 2000)!!
        val coverFrame = phone.frame(4000, 2000, Base.COVER, Framing())
        assertEquals(cover.scaledWidth, coverFrame.width)
        assertEquals(cover.scaledHeight, coverFrame.height)
        assertEquals(-cover.crop.left, coverFrame.left)
        assertEquals(-cover.crop.top, coverFrame.top)

        val fit = phone.fit(4000, 2000)!!
        val fitFrame = phone.frame(4000, 2000, Base.FIT, Framing())
        assertEquals(fit.scaledWidth, fitFrame.width)
        assertEquals(fit.scaledHeight, fitFrame.height)
        assertEquals(fit.destination.left, fitFrame.left)
        assertEquals(fit.destination.top, fitFrame.top)
    }

    @Test
    fun `a zoomed fitted painting overflows along the axis it outgrows and pan puts it at each edge`() {
        // Fitted, 4000x2000 is 1080x540 on the phone; zoom 3 makes it 3240x1620.
        val left = phone.frame(4000, 2000, Base.FIT, Framing(3f, 0f, 0f))
        val middle = phone.frame(4000, 2000, Base.FIT, Framing(3f))
        val right = phone.frame(4000, 2000, Base.FIT, Framing(3f, 1f, 1f))

        assertEquals(3240, left.width)
        assertEquals(0, left.left)
        assertEquals(-(3240 - 1080) / 2, middle.left)
        assertEquals(1080 - 3240, right.left)
        // 1620 is shorter than the screen's 2340, so vertically it stays centred whatever the pan.
        assertEquals((2340 - 1620) / 2, left.top)
        assertEquals((2340 - 1620) / 2, right.top)
    }

    @Test
    fun `pan on an axis that does not overflow changes nothing`() {
        // Fitted at zoom 1 the painting is centred and the pan is ignored.
        assertEquals(
            phone.frame(4000, 2000, Base.FIT, Framing(1f, 0f, 0f)),
            phone.frame(4000, 2000, Base.FIT, Framing(1f, 1f, 1f)),
        )
        // Zoomed past the width but not the height: only the horizontal pan matters.
        val a = phone.frame(4000, 2000, Base.FIT, Framing(2f, 0f, 0f))
        val b = phone.frame(4000, 2000, Base.FIT, Framing(2f, 0f, 1f))
        assertEquals(a, b)
    }

    @Test
    fun `an overflowing axis always reaches both screen edges`() {
        for (zoom in listOf(1f, 1.5f, 2f, 3f)) {
            for (pan in listOf(0f, 0.3f, 0.5f, 1f)) {
                val rect = phone.frame(4000, 2000, Base.COVER, Framing(zoom, pan, pan))
                assertTrue(rect.left <= 0 && rect.left + rect.width >= phone.width)
                assertTrue(rect.top <= 0 && rect.top + rect.height >= phone.height)
            }
        }
    }

    @Test
    fun `a preview-scale frame is the screen-scale frame scaled down`() {
        val preview = Screen(270, 585) // a quarter of the phone
        val framing = Framing(2f, 0.25f, 0.75f)
        val full = phone.frame(4000, 2000, Base.COVER, framing)
        val small = preview.frame(4000, 2000, Base.COVER, framing)

        assertEquals(full.width / 4.0, small.width.toDouble(), 1.0)
        assertEquals(full.left / 4.0, small.left.toDouble(), 1.0)
        assertEquals(full.top / 4.0, small.top.toDouble(), 1.0)
    }

    @Test
    fun `dragging moves the painting by exactly the finger and stops at its edge`() {
        val start = Framing(2f, 0.5f, 0.5f)
        val before = phone.frame(4000, 2000, Base.COVER, start)
        val moved = phone.reframe(4000, 2000, Base.COVER, start, 1f, 500f, 500f, dx = 100f, dy = 0f)
        val after = phone.frame(4000, 2000, Base.COVER, moved)
        assertEquals(before.left + 100.0, after.left.toDouble(), 1.0)

        val past = phone.reframe(4000, 2000, Base.COVER, start, 1f, 500f, 500f, dx = 1e6f, dy = 1e6f)
        assertEquals(0f, past.panX, 0f)
        assertEquals(0f, past.panY, 0f)
    }

    @Test
    fun `a slow drag adds up, whichever way it goes`() {
        // A finger at 120 Hz sends well under a pixel per event. Both directions must
        // accumulate: this once moved right and up eagerly and left and down not at all.
        val preview = Screen(400, 880)
        for (step in listOf(-0.4f, 0.4f)) {
            var framing = Framing(2f, 0.5f, 0.5f)
            val before = preview.frame(4000, 2000, Base.COVER, framing)
            repeat(200) {
                framing = preview.reframe(4000, 2000, Base.COVER, framing, 1f, 200f, 440f, dx = step, dy = step)
            }
            val after = preview.frame(4000, 2000, Base.COVER, framing)
            assertEquals(before.left + step * 200.0, after.left.toDouble(), 1.5)
            assertEquals(before.top + step * 200.0, after.top.toDouble(), 1.5)
        }
    }

    @Test
    fun `a gesture with no fingers left changes nothing`() {
        // What lifting the last finger reports: an unspecified centroid, which is NaN.
        val start = Framing(2f, 0.3f, 0.7f)
        assertEquals(start, phone.reframe(4000, 2000, Base.COVER, start, 1f, Float.NaN, Float.NaN, 0f, 0f))
    }

    @Test
    fun `pinching keeps the point under the fingers where it was and stays within the zoom limits`() {
        val start = Framing()
        val before = phone.frame(4000, 2000, Base.COVER, start)
        val zoomed = phone.reframe(4000, 2000, Base.COVER, start, 2f, 800f, 1200f, 0f, 0f)
        val after = phone.frame(4000, 2000, Base.COVER, zoomed)
        // The painting point under (800, 1200) before is at (800 - left) / width of the painting.
        assertEquals((800.0 - before.left) / before.width, (800.0 - after.left) / after.width, 0.002)

        assertEquals(Screen.MAX_ZOOM, phone.reframe(4000, 2000, Base.COVER, start, 50f, 0f, 0f, 0f, 0f).zoom, 0f)
        assertEquals(1f, phone.reframe(4000, 2000, Base.COVER, Framing(2f), 0.01f, 0f, 0f, 0f, 0f).zoom, 0f)
    }

    @Test
    fun `a framing for a quarter-turned painting swaps the axes`() {
        assertEquals(Framing(2f, 0.25f, 0.9f), Framing(2f, 0.1f, 0.25f).forQuarterTurn())
    }
}
