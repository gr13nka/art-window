package dev.artwindow

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ImageProcessingTest {
    @Test
    fun `automatic border color samples the edge rather than the center`() {
        val blue = 0xff2040c0.toInt()
        val red = 0xffe02020.toInt()
        val pixels = IntArray(25) { blue }
        pixels[12] = red

        assertEquals(blue, edgeAverageColor(pixels, 5, 5))
    }

    @Test
    fun `common colors are ordered by frequency and deduplicated`() {
        val red = 0xffe02020.toInt()
        val almostRed = 0xffe92325.toInt()
        val blue = 0xff2040d0.toInt()
        val colors = commonImageColors(
            IntArray(18) { index ->
                when {
                    index < 9 -> red
                    index < 14 -> almostRed
                    else -> blue
                }
            },
        )

        assertTrue(colors.size >= 2)
        assertTrue((colors.first() ushr 16 and 0xff) > 200)
        assertTrue(colors.any { (it and 0xff) > 150 && (it ushr 16 and 0xff) < 80 })
    }

    @Test
    fun `zero-radius blur is an exact copy`() {
        val pixels = intArrayOf(0xff000000.toInt(), 0xffffffff.toInt())
        assertArrayEquals(pixels, blurPixels(pixels, 2, 1, 0))
    }

    @Test
    fun `blur preserves a flat color`() {
        val color = 0xff527aa1.toInt()
        val blurred = blurPixels(IntArray(25) { color }, 5, 5, 2)
        assertTrue(blurred.all { it == color })
    }

    @Test
    fun `blur spreads a bright pixel to its neighbors`() {
        val pixels = IntArray(25) { 0xff000000.toInt() }
        pixels[12] = 0xffffffff.toInt()
        val blurred = blurPixels(pixels, 5, 5, 1)

        assertTrue((blurred[11] and 0xff) > 0)
        assertTrue((blurred[12] and 0xff) < 255)
    }
}
