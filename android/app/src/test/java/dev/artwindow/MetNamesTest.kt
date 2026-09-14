package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.File

class MetNamesTest {

    @Test
    fun `reads the object id back out of a name Met wrote`() {
        assertEquals(436535L, idOf(File("met-436535.jpg")))
    }

    @Test
    fun `rejects a name this source never wrote`() {
        assertNull(idOf(File("IMG_001.jpg")))
    }

    @Test
    fun `rejects a met- name whose remainder is not an id`() {
        assertNull(idOf(File("met-abc.jpg")))
    }

    @Test
    fun `search URL encodes a geography value and pagination`() {
        val url = Met.searchUrl(ArtworkRegion.NORTH_AMERICA, 500)

        assertEquals(true, url.contains("geoLocation=North+America"))
        assertEquals(true, url.endsWith("limit=500&offset=500"))
    }

    @Test
    fun `a durable copy is the same artwork as its cache origin`() {
        val cache = artwork(File("cache/met-436535.jpg"))
        val favourite = artwork(File("files/favourites/met-436535.jpg"))

        assertEquals(true, sameArtwork(cache, favourite))
        assertEquals(false, sameArtwork(cache, artwork(File("cache/met-436536.jpg"))))
    }

    private fun artwork(path: File) = Artwork(
        title = "Painting",
        byline = "Artist",
        attribution = "The Met",
        detailsUrl = null,
        origin = null,
        path = path,
    )
}
