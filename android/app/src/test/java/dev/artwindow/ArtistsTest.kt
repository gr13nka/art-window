package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ArtistsTest {

    private fun row(
        name: String = "Tom Roberts",
        region: String = "OCEANIA",
        title: String = "Shearing the Rams",
        byline: String = "Tom Roberts, 1890",
    ) = listOf(name, region, "https://en.wikipedia.org/wiki/X", "123", title, byline, "x.jpg").joinToString("\t")

    @Test
    fun `reads a good row after the comment line`() {
        val painter = Artists.parse("# generated\n${row()}\n").among(listOf("Tom Roberts")).single()

        assertEquals(ArtworkRegion.OCEANIA, painter.region)
        assertEquals("Shearing the Rams", painter.title)
        assertEquals("1890", painter.year)
        assertEquals("x.jpg", painter.file)
        assertEquals("https://en.wikipedia.org/wiki/X", painter.aboutUrl)
    }

    @Test
    fun `skips a short row and a row with an unknown region`() {
        val text = listOf("# c", "Short\tOCEANIA\turl", row(name = "Lost", region = "ATLANTIS"), row()).joinToString("\n")

        assertEquals(listOf("Tom Roberts"), Artists.parse(text).among(listOf("Short", "Lost", "Tom Roberts")).map { it.name })
    }

    @Test
    fun `a byline with no year gives no year`() {
        val bare = Artists.parse(row(byline = "Tom Roberts")).among(listOf("Tom Roberts")).single()
        val other = Artists.parse(row(byline = "Someone Else, 1890")).among(listOf("Tom Roberts")).single()

        assertNull(bare.year)
        assertNull(other.year)
    }

    @Test
    fun `the line joins painting, year, region and count`() {
        val dated = Artists.parse(row(title = "Golden summer, Eaglemont", byline = "Tom Roberts, 1889"))
            .among(listOf("Tom Roberts")).single()
        val undated = Artists.parse(row(byline = "Tom Roberts")).among(listOf("Tom Roberts")).single()

        assertEquals("Golden summer, Eaglemont, 1889 · Oceania, 40 paintings", Artists.line(dated, 40))
        assertEquals("Shearing the Rams · Oceania, 25 paintings", Artists.line(undated, 25))
    }

    @Test
    fun `orders by region then name and leaves out a painter the index lacks`() {
        val text = listOf(
            row(name = "Zed", region = "OCEANIA"),
            row(name = "Beta", region = "SOUTH_AMERICA"),
            row(name = "Alpha", region = "SOUTH_AMERICA"),
            row(name = "Mid", region = "EUROPE"),
        ).joinToString("\n")

        val ordered = Artists.parse(text).among(listOf("Zed", "Beta", "Alpha", "Mid", "Missing")).map { it.name }

        assertEquals(listOf("Mid", "Alpha", "Beta", "Zed"), ordered)
    }

    @Test
    fun `a painter is blocked only when not chosen and the catalogue says why`() {
        val chosen = setOf("A")
        val blocks = mapOf(
            "A" to ArtistBlock.SUBJECT,
            "C" to ArtistBlock.SUBJECT,
            "D" to ArtistBlock.RELIGIOUS,
        )

        assertNull(blockedReason("A", chosen, blocks))
        assertNull(blockedReason("B", chosen, blocks))
        assertEquals("None of C's paintings match the chosen subject.", blockedReason("C", chosen, blocks))
        assertEquals("None of D's paintings are left with religious scenes hidden.", blockedReason("D", chosen, blocks))
    }
}
