package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import java.io.File
import org.junit.Test

class WallpaperPreferencesTest {
    private val screen = Screen(1080, 2340)

    @Test
    fun `screen-shaped keeps the existing narrow tolerance`() {
        assertTrue(ArtworkShape.SCREEN.accepts(0.47, screen))
        assertTrue(!ArtworkShape.SCREEN.accepts(0.8, screen))
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
    fun `on a landscape screen SCREEN wants wide paintings and NEAR_SQUARE adds the square ones`() {
        val tv = Screen(3840, 2160) // aspect ~1.778
        assertTrue(ArtworkShape.SCREEN.accepts(1.6, tv))
        assertTrue(!ArtworkShape.SCREEN.accepts(0.8, tv))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(1.6, tv))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(1.0, tv))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(0.8, tv))
        assertTrue(!ArtworkShape.NEAR_SQUARE.accepts(0.79, tv))
        assertTrue(ArtworkShape.NEAR_SQUARE.accepts(tv.aspectRatio * 1.15, tv))
        assertTrue(!ArtworkShape.NEAR_SQUARE.accepts(tv.aspectRatio * 1.16, tv))
    }

    @Test
    fun `the shape saved as PHONE loads as SCREEN`() {
        assertEquals(ArtworkShape.SCREEN, shapeValue("PHONE"))
        assertEquals(ArtworkShape.NEAR_SQUARE, shapeValue("NEAR_SQUARE"))
        assertEquals(ArtworkShape.SCREEN, shapeValue(null))
        assertEquals(ArtworkShape.SCREEN, shapeValue("future-value"))
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
    }

    @Test
    fun `a stored list of several painters reads as its first only`() {
        assertEquals(setOf("Mikhail Vrubel"), artistValues("Mikhail Vrubel,Ivan Shishkin"))
        assertEquals(setOf("Ivan Shishkin"), artistValues(",Ivan Shishkin"))
    }

    @Test
    fun `choosing a painter replaces whoever was chosen, and Any clears the choice`() {
        val vrubel = WallpaperPreferences(artworkArtists = chosenArtist("Mikhail Vrubel"))
        val shishkin = vrubel.copy(artworkArtists = chosenArtist("Ivan Shishkin"))
        assertEquals(setOf("Ivan Shishkin"), shishkin.artworkArtists)
        assertEquals(emptySet<String>(), shishkin.copy(artworkArtists = emptySet()).artworkArtists)
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

    @Test
    fun `framing defaults to centred and unzoomed`() {
        val prefs = WallpaperPreferences()
        assertEquals(1f, prefs.frameZoom, 0f)
        assertEquals(Framing(), prefs.framingFor(File("/cache/x.jpg"), screen))
    }

    @Test
    fun `the zoom follows every painting while the pan belongs to one`() {
        val prefs = WallpaperPreferences(frameZoom = 2f, panX = 0.9f, panY = 0.1f, panPainting = "museums-met-1.jpg")

        assertEquals(Framing(2f, 0.9f, 0.1f), prefs.framingFor(File("/cache/museums-met-1.jpg"), screen))
        assertEquals(Framing(2f), prefs.framingFor(File("/cache/museums-met-2.jpg"), screen))
    }

    @Test
    fun `a landscape screen ignores the framing altogether`() {
        val prefs = WallpaperPreferences(frameZoom = 2f, panX = 0.9f, panPainting = "a.jpg")
        assertEquals(Framing(), prefs.framingFor(File("/cache/a.jpg"), Screen(3840, 2160)))
    }

    @Test
    fun `only the styles with a sharp picture are framed`() {
        val prefs = WallpaperPreferences()
        assertTrue(prefs.copy(style = WallpaperStyle.ZOOM).framesSharpPicture())
        assertTrue(prefs.copy(style = WallpaperStyle.BORDERS).framesSharpPicture())
        assertTrue(prefs.copy(style = WallpaperStyle.BLUR, blurVariant = BlurVariant.BACKDROP).framesSharpPicture())
        assertTrue(!prefs.copy(style = WallpaperStyle.BLUR, blurVariant = BlurVariant.WHOLE_IMAGE).framesSharpPicture())
        assertTrue(!prefs.copy(style = WallpaperStyle.STRETCH).framesSharpPicture())
        assertEquals(Base.COVER, prefs.copy(style = WallpaperStyle.ZOOM).frameBase())
        assertEquals(Base.FIT, prefs.copy(style = WallpaperStyle.BORDERS).frameBase())
    }

    @Test
    fun `framing is not part of the filters`() {
        assertTrue(WallpaperPreferences().sameFiltersAs(WallpaperPreferences(frameZoom = 3f, panX = 0.1f, panPainting = "x.jpg")))
    }
}
