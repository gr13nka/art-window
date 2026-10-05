package dev.artwindow

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory

/**
 * The painters the artist browser shows, each with the picture they are known by —
 * `catalogue/dist/artists/` (built by `catalogue/showcase.py`), bundled as the assets
 * `artists/index.tsv` and `artists/<slug>.jpg`. The catalogue stays the owner of who may
 * be chosen and how many paintings they have; this only knows what to *show* of them.
 * Android's own copy of the rule `Pending::artist_cards` keeps on the desktop and
 * `Artists.swift` on iOS: a change to the line or the ordering belongs in all three.
 */
class Artists(private val painters: List<Painter>) {

    /** One row of the index. [year] is null when the painting's byline carries none. */
    data class Painter(
        val name: String,
        val region: ArtworkRegion,
        val aboutUrl: String,
        val title: String,
        val year: String?,
        val file: String,
    )

    /**
     * The painters among [names] that the index knows, by region (the enum's order, as
     * the Origins section lists them) and then by name. A name with no row is left out:
     * the browser cannot show a painter it has no picture of.
     */
    fun among(names: Collection<String>): List<Painter> = painters
        .filter { it.name in names }
        .sortedWith(compareBy({ it.region.ordinal }, { it.name }))

    /**
     * Decodes [painter]'s picture no wider than needed for [targetWidth]. Thumbnails ask
     * for little and only the painter being looked at asks for much, so a shelf of
     * fourteen never holds fourteen 1400 px bitmaps.
     */
    fun picture(context: Context, painter: Painter, targetWidth: Int): Bitmap? {
        val path = "$DIRECTORY/${painter.file}"
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.assets.open(path).use { BitmapFactory.decodeStream(it, null, bounds) }
        if (bounds.outWidth <= 0) return null
        val options = BitmapFactory.Options().apply { inSampleSize = sampleSizeFor(bounds.outWidth, targetWidth) }
        return context.assets.open(path).use { BitmapFactory.decodeStream(it, null, options) }
    }

    companion object {
        private const val DIRECTORY = "artists"
        private const val INDEX = "$DIRECTORY/index.tsv"
        private const val FIELD_COUNT = 7

        @Volatile
        private var cached: Artists? = null

        /** Reads and parses the index once per process; later calls return the same instance. */
        fun load(context: Context): Artists {
            cached?.let { return it }
            return synchronized(this) {
                cached ?: context.assets.open(INDEX).bufferedReader().use { parse(it.readText()) }
                    .also { cached = it }
            }
        }

        /**
         * Parses `index.tsv`: a leading `#` comment, then `name region about showcase
         * title byline file` rows. The file is generated, so a short row or an unknown
         * region is skipped rather than failing the rest. `showcase` (the painting's
         * `source:id`) is for the build and unused here.
         */
        fun parse(text: String): Artists = Artists(
            text.lineSequence()
                .filter { it.isNotBlank() && !it.startsWith("#") }
                .mapNotNull(::parseLine)
                .toList(),
        )

        private fun parseLine(line: String): Painter? {
            val fields = line.split("\t")
            if (fields.size != FIELD_COUNT) return null
            val region = ArtworkRegion.entries.firstOrNull { it.name == fields[1] } ?: return null
            return Painter(
                name = fields[0],
                region = region,
                aboutUrl = fields[2],
                title = fields[4],
                year = yearIn(fields[5], fields[0]),
                file = fields[6],
            )
        }

        // The byline reads "painter, year"; the painter is the heading in the browser, so
        // only what follows the name is wanted — and nothing at all if nothing does.
        private fun yearIn(byline: String, name: String): String? =
            byline.takeIf { it.startsWith(name) }
                ?.removePrefix(name)?.trimStart(',', ' ')
                ?.takeIf { it.isNotEmpty() }

        /**
         * The one line under a painter's name, e.g. "Golden summer, Eaglemont, 1889 ·
         * Oceania, 40 paintings": the painting they are known by, where it is from, and
         * how many of their paintings the catalogue holds ([paintings], from
         * [Catalogue.paintingsBy]).
         */
        fun line(painter: Painter, paintings: Int): String {
            val painting = painter.year?.let { "${painter.title}, $it" } ?: painter.title
            return "$painting · ${regionLabel(painter.region)}, $paintings paintings"
        }
    }
}
