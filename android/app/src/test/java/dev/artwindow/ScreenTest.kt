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
    fun `mightHold accepts a catalogue aspect slightly beyond the photo tolerance`() {
        val screen = Screen(1080, 2340)
        // trim ~= 0.18 here: past place()'s MAX_TRIM of 0.15, but within mightHold's slack.
        assertTrue(screen.mightHold(0.378))
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
    fun `a landscape-oriented display normalises to portrait`() {
        val screen = Screen(2340, 1080)
        assertEquals(1080, screen.width)
        assertEquals(2340, screen.height)
    }
}
