package dev.artwindow

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MuseumsTest {

    @Test
    fun `reads source and id back out of a name this module wrote`() {
        assertEquals("nga-1234", keyOf(File("nga-1234.jpg")))
        assertEquals("cma-98765", keyOf(File("cma-98765.png")))
        assertEquals("smk-KMS1", keyOf(File("smk-KMS1.jpg")))
    }

    @Test
    fun `a legacy met- name from before the four-museum catalogue still parses`() {
        assertEquals("met-436535", keyOf(File("met-436535.jpg")))
    }

    @Test
    fun `rejects a name with no recognised museum prefix`() {
        assertNull(keyOf(File("IMG_001.jpg")))
    }

    @Test
    fun `rejects a recognised prefix with nothing after it`() {
        assertNull(keyOf(File("met-.jpg")))
    }

    @Test
    fun `museumSourceOfKey recovers the museum a keyOf key names`() {
        assertEquals(MuseumSource.MET, museumSourceOfKey("met-436535"))
        assertEquals(MuseumSource.NGA, museumSourceOfKey("nga-1234"))
        assertNull(museumSourceOfKey(null))
        assertNull(museumSourceOfKey("unknown-1"))
    }

    @Test
    fun `a durable copy is the same artwork as its cache origin`() {
        val cache = artwork(File("cache/nga-1234.jpg"))
        val favourite = artwork(File("files/favourites/nga-1234.jpg"))

        assertEquals(true, sameArtwork(cache, favourite))
        assertEquals(false, sameArtwork(cache, artwork(File("cache/nga-1235.jpg"))))
    }

    private fun artwork(path: File) = Artwork(
        title = "Painting",
        byline = "Artist",
        attribution = "National Gallery of Art, Washington",
        detailsUrl = null,
        origin = null,
        path = path,
    )
}
