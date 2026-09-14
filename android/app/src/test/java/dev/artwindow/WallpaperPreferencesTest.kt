package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class WallpaperPreferencesTest {
    private val screen = Screen(1080, 2340)

    @Test
    fun `phone-shaped keeps the existing narrow tolerance`() {
        assertTrue(ArtworkShape.PHONE.accepts(0.47, screen))
        assertTrue(!ArtworkShape.PHONE.accepts(0.8, screen))
    }

    @Test
    fun `near-square is cumulative but stops beyond five by four`() {
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(0.47, screen))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(0.8, screen))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(1.25, screen))
        assertTrue(!ArtworkShape.NEAR_SQUARE.accepts(1.251, screen))
        assertTrue(!ArtworkShape.NEAR_SQUARE.accepts(0.35, screen))
    }

    @Test
    fun `any shape permits fully horizontal work`() {
        assertTrue(ArtworkShape.ANY.accepts(3.0, screen))
    }

    @Test
    fun `catalogue near-square check has slack but photograph check does not`() {
        assertTrue(ArtworkShape.NEAR_SQUARE.mightAccept(1.31, screen))
        assertTrue(!ArtworkShape.NEAR_SQUARE.accepts(1.31, screen))
    }

    @Test
    fun `unknown stored enum falls back safely`() {
        assertEquals(WallpaperStyle.ZOOM, enumValue("future-value", WallpaperStyle.ZOOM))
        assertEquals(WallpaperStyle.BLUR, enumValue("BLUR", WallpaperStyle.ZOOM))
    }

    @Test
    fun `missing and unknown region settings retain the established pool`() {
        assertEquals(ArtworkRegion.DEFAULT, regionValues(null))
        assertEquals(ArtworkRegion.DEFAULT, regionValues("FUTURE_REGION"))
    }

    @Test
    fun `stored regions support multiple choices`() {
        assertEquals(
            setOf(ArtworkRegion.AFRICA, ArtworkRegion.SOUTH_AMERICA),
            regionValues("AFRICA,SOUTH_AMERICA"),
        )
    }

    @Test
    fun `the final selected region cannot be removed`() {
        assertEquals(
            setOf(ArtworkRegion.OCEANIA),
            toggleRegion(setOf(ArtworkRegion.OCEANIA), ArtworkRegion.OCEANIA),
        )
        assertEquals(
            setOf(ArtworkRegion.EUROPE, ArtworkRegion.ASIA),
            toggleRegion(setOf(ArtworkRegion.EUROPE), ArtworkRegion.ASIA),
        )
    }
}
