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
    fun `canRender rejects an image too small to reach screen size under its style`() {
        val tooSmall = WallpaperPreferences(style = WallpaperStyle.ZOOM)
        assertTrue(!tooSmall.canRender(200, 433, screen)) // needs over 5x enlargement to cover
        assertTrue(tooSmall.canRender(1080, 2340, screen))
    }

    @Test
    fun `canRender for BORDERS and STRETCH uses fit and stretch geometry respectively`() {
        val borders = WallpaperPreferences(style = WallpaperStyle.BORDERS)
        assertTrue(borders.canRender(4000, 2000, screen)) // fits within the screen, letterboxed
        assertTrue(!borders.canRender(200, 100, screen)) // would need too much enlargement to fit

        val stretch = WallpaperPreferences(style = WallpaperStyle.STRETCH)
        assertTrue(stretch.canRender(1080, 2340, screen))
        assertTrue(!stretch.canRender(200, 433, screen))
    }

    @Test
    fun `unknown stored enum falls back safely`() {
        assertEquals(WallpaperStyle.ZOOM, enumValue("future-value", WallpaperStyle.ZOOM))
        assertEquals(WallpaperStyle.BLUR, enumValue("BLUR", WallpaperStyle.ZOOM))
    }

    @Test
    fun `a never-saved preference starts from the curated DEFAULT, for regions and subjects alike`() {
        assertEquals(ArtworkRegion.DEFAULT, regionValues(null))
        assertEquals(ArtworkSubject.DEFAULT, subjectValues(null))
    }

    @Test
    fun `a saved value that decodes to nothing means Any, not DEFAULT`() {
        // Blank, or naming only options this build no longer recognises — once a
        // preference has actually been saved, empty is Any, never a fallback. This is
        // also the acceptable migration path: an empty subject set a previous build
        // saved (its own DEFAULT fallback) now reads as Any instead of Landscape.
        assertEquals(emptySet<ArtworkRegion>(), regionValues(""))
        assertEquals(emptySet<ArtworkRegion>(), regionValues("FUTURE_REGION"))
        assertEquals(emptySet<ArtworkSubject>(), subjectValues(""))
        assertEquals(emptySet<ArtworkSubject>(), subjectValues("FUTURE_SUBJECT"))
        assertEquals(emptySet<ArtworkSubject>(), subjectValues("CITY")) // a retired subject name
    }

    @Test
    fun `stored regions and subjects support multiple choices, dropping only what they don't recognise`() {
        assertEquals(
            setOf(ArtworkRegion.AFRICA, ArtworkRegion.SOUTH_AMERICA),
            regionValues("AFRICA,SOUTH_AMERICA"),
        )
        assertEquals(
            setOf(ArtworkSubject.SEASCAPE, ArtworkSubject.STILL_LIFE),
            subjectValues("SEASCAPE,STILL_LIFE"),
        )
        assertEquals(setOf(ArtworkSubject.SEASCAPE), subjectValues("CITY,SEASCAPE")) // "CITY" is retired
    }

    @Test
    fun `missing artist settings mean Any even on a fresh install, blank entries are dropped`() {
        assertEquals(emptySet<String>(), artistValues(null))
        assertEquals(emptySet<String>(), artistValues(""))
        assertEquals(setOf("Mikhail Vrubel"), artistValues("Mikhail Vrubel"))
        assertEquals(setOf("Mikhail Vrubel", "Ivan Shishkin"), artistValues("Mikhail Vrubel,Ivan Shishkin"))
    }

    @Test
    fun `toggled can empty a selection back to Any — there is no last item to protect`() {
        assertEquals(
            emptySet<ArtworkRegion>(),
            toggled(setOf(ArtworkRegion.OCEANIA), ArtworkRegion.OCEANIA),
        )
        assertEquals(
            setOf(ArtworkRegion.EUROPE, ArtworkRegion.ASIA),
            toggled(setOf(ArtworkRegion.EUROPE), ArtworkRegion.ASIA),
        )
        assertEquals(
            emptySet<ArtworkSubject>(),
            toggled(setOf(ArtworkSubject.LANDSCAPE), ArtworkSubject.LANDSCAPE),
        )
        assertEquals(
            setOf(ArtworkSubject.LANDSCAPE, ArtworkSubject.STILL_LIFE),
            toggled(setOf(ArtworkSubject.LANDSCAPE), ArtworkSubject.STILL_LIFE),
        )
        assertEquals(
            setOf("Mikhail Vrubel"),
            toggled(emptySet(), "Mikhail Vrubel"),
        )
    }
}
