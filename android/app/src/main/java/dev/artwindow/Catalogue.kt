package dev.artwindow

import android.content.Context

/**
 * A prebuilt, pixel-verified list of paintings from four museums — the Met, the NGA,
 * Cleveland and SMK — plus, where a row names one, an artist credited from Wikimedia
 * Commons — built offline by `catalogue/build.py` (see CLAUDE.md) rather than searched
 * for on the device.
 *
 * `catalogue/dist/paintings.tsv` ships as the asset [ASSET_NAME]: one row per
 * painting, holding what a region/subject/shape filter needs plus the exact pixel
 * size of [Entry.imageUrl], verified at build time. Because that size is already
 * real, [candidates] and [availableSubjects] can decide shape and renderability
 * outright — there is no live preview step left to double-check them. Every rule that
 * decides what a phone will actually show — subject and artist matching, the portrait
 * and religious exclusions, shape and render fit — lives here, so there is exactly one
 * place that decides.
 */
class Catalogue(private val entries: List<Entry>) {

    /** One catalogued painting, ready to download without any further live lookup. */
    data class Entry(
        val source: MuseumSource,
        val id: String,
        val region: ArtworkRegion,
        val width: Int,
        val height: Int,
        val imageUrl: String,
        val detailsUrl: String,
        val title: String,
        val byline: String,
        val origin: String,
        val tags: List<String>,
        val artist: String,
    )

    /**
     * Every entry [preferences] offers on [screen], in random order so a fixed prefix
     * of the list doesn't always favour the same paintings — see [matching] for the
     * rule that decides "offers".
     */
    fun candidates(preferences: WallpaperPreferences, screen: Screen): List<Entry> =
        matching(preferences, screen).toList().shuffled()

    /**
     * Whether anything at all passes [matching] under [preferences] and [screen] —
     * Settings uses this to decide whether Apply should be enabled, checked with
     * [Sequence.any] rather than building and shuffling [candidates]' full list, since
     * only presence, not the entry, matters here.
     */
    fun anyMatch(preferences: WallpaperPreferences, screen: Screen): Boolean = matching(preferences, screen).any()

    /**
     * Which [ArtworkRegion]s have at least one entry when chosen alone within Origins,
     * the other sections (Subjects, Artists, Shape, the religious toggle) held at
     * [preferences]'s currently staged values — see [matching]. Settings hides a region
     * chip this doesn't return.
     */
    fun availableRegions(preferences: WallpaperPreferences, screen: Screen): Set<ArtworkRegion> =
        ArtworkRegion.entries.filterTo(mutableSetOf()) { region ->
            matching(preferences.copy(artworkRegions = setOf(region)), screen).any()
        }

    /**
     * Which [ArtworkSubject]s have at least one entry when chosen alone within
     * Subjects, the other sections held at [preferences]'s currently staged values —
     * [availableRegions]'s mirror for the subject axis. Settings hides a subject chip
     * this doesn't return.
     */
    fun availableSubjects(preferences: WallpaperPreferences, screen: Screen): Set<ArtworkSubject> =
        ArtworkSubject.entries.filterTo(mutableSetOf()) { subject ->
            matching(preferences.copy(artworkSubjects = setOf(subject)), screen).any()
        }

    /**
     * Every distinct, non-empty artist name in the catalogue, for Settings' chip list —
     * sorted so the list is stable across loads, independent of the other sections.
     */
    fun artists(): List<String> = entries.mapNotNull { it.artist.takeIf(String::isNotEmpty) }.distinct().sorted()

    /**
     * Which of [artists]' names have at least one entry when chosen alone within
     * Artists, the other sections held at [preferences]'s currently staged values —
     * [availableRegions]'s mirror for the artist axis. Settings hides an artist chip
     * this doesn't return.
     */
    fun availableArtists(preferences: WallpaperPreferences, screen: Screen): Set<String> =
        artists().filterTo(mutableSetOf()) { artist ->
            matching(preferences.copy(artworkArtists = setOf(artist)), screen).any()
        }

    /**
     * An entry qualifies when it passes every section — Shape, Origins, Subjects and
     * Artists — with an empty selection in a section meaning **Any**: that section
     * filters nothing (`preferences.artworkShape` always names one shape, so it never
     * needs this — [ArtworkShape.ANY] already means Any there). Sections are ANDed
     * together; the choices within a section are ORed. The portrait and religious
     * exclusions apply regardless of what qualified an entry.
     */
    private fun matching(preferences: WallpaperPreferences, screen: Screen): Sequence<Entry> = entries
        .asSequence()
        .filter { preferences.artworkRegions.isEmpty() || it.region in preferences.artworkRegions }
        .filter { entry ->
            preferences.artworkSubjects.isEmpty() ||
                preferences.artworkSubjects.any { subject -> matchesSubject(entry, subject) }
        }
        .filter { entry -> preferences.artworkArtists.isEmpty() || entry.artist in preferences.artworkArtists }
        .filterNot { isPortrait(it.title, it.tags) }
        .filterNot { preferences.hideReligious && isReligious(it.title, it.tags) }
        .filter { entry -> preferences.artworkShape.accepts(entry.width.toDouble() / entry.height, screen) }
        .filter { entry -> preferences.canRender(entry.width, entry.height, screen) }

    companion object {
        private const val ASSET_NAME = "paintings.tsv"
        private const val FIELD_COUNT = 11
        private const val FIELD_COUNT_WITH_ARTIST = 12

        @Volatile
        private var cached: Catalogue? = null

        /** Reads and parses [ASSET_NAME] once per process; later calls return the same instance. */
        fun load(context: Context): Catalogue {
            cached?.let { return it }
            return synchronized(this) {
                cached ?: context.assets.open(ASSET_NAME).bufferedReader().use { parse(it.readText()) }
                    .also { cached = it }
            }
        }

        /**
         * Parses the TSV format `catalogue/build.py` writes: a leading `#` comment line,
         * then one `source id region width height image_url details_url title byline
         * origin tags [artist]` row per painting — the trailing `artist` column is a
         * Wikimedia Commons addition, present only on `wmc` rows for now, so both
         * [FIELD_COUNT] and [FIELD_COUNT_WITH_ARTIST] are accepted and a row missing it
         * gets an empty [Entry.artist]. Pure and dependency-free so it is unit-testable
         * without an asset manager. A row naming an unknown source or region, or whose
         * width or height isn't a positive integer, is skipped rather than failing the
         * whole catalogue — the build applies only objective gates, and a stray bad row
         * must not take the rest of the list down with it.
         */
        fun parse(text: String): Catalogue {
            val entries = text.lineSequence()
                .filter { it.isNotBlank() && !it.startsWith("#") }
                .mapNotNull(::parseLine)
                .toList()
            return Catalogue(entries)
        }

        private fun parseLine(line: String): Entry? {
            val fields = line.split("\t")
            if (fields.size != FIELD_COUNT && fields.size != FIELD_COUNT_WITH_ARTIST) return null
            val source = MuseumSource.byCode(fields[0]) ?: return null
            val region = ArtworkRegion.entries.firstOrNull { it.name == fields[2] } ?: return null
            val width = fields[3].toIntOrNull() ?: return null
            val height = fields[4].toIntOrNull() ?: return null
            if (width <= 0 || height <= 0) return null
            val tags = fields[10].takeIf { it.isNotEmpty() }?.split("|").orEmpty()
            return Entry(
                source = source,
                id = fields[1],
                region = region,
                width = width,
                height = height,
                imageUrl = fields[5],
                detailsUrl = fields[6],
                title = fields[7],
                byline = fields[8],
                origin = fields[9],
                tags = tags,
                artist = fields.getOrNull(11).orEmpty(),
            )
        }
    }
}

/**
 * Whether any of [subject]'s queries names [entry] — case-insensitive, whole word,
 * an optional trailing "s" so a search term and its plural both match, checked
 * against the title and every tag.
 */
private fun matchesSubject(entry: Catalogue.Entry, subject: ArtworkSubject): Boolean =
    subject.queries.any { query -> matchesQuery(query, entry.title) || entry.tags.any { tag -> matchesQuery(query, tag) } }

private fun matchesQuery(query: String, text: String): Boolean =
    Regex("\\b" + Regex.escape(query) + "s?\\b", RegexOption.IGNORE_CASE).containsMatchIn(text)

/**
 * Whether [title] or a tag in [tagTerms] names a portrait, for [Catalogue.candidates]'s
 * exclusion. Danish "portræt" (SMK's titles are Danish — see `docs/android.md`) is
 * checked the same substring way as "portrait" rather than through [ArtworkSubject],
 * since a portrait is excluded outright and never itself a subject someone selects.
 */
internal fun isPortrait(title: String, tagTerms: List<String>): Boolean =
    title.contains("portrait", ignoreCase = true) ||
        title.contains("portræt", ignoreCase = true) ||
        tagTerms.any { it.equals("Portraits", ignoreCase = true) }

/**
 * Whether [title] or one of [tagTerms] names a religious scene or figure, for
 * [WallpaperPreferences.hideReligious].
 *
 * One whole-word, case-insensitive regex rather than per-word substring checks, so
 * that "Christmas" is never mistaken for "Christ" and short fragments like "holy"
 * or "magi" don't fire inside an unrelated longer word. Multi-word phrases such as
 * "last supper" match as a phrase for the same reason. "St." is deliberately left
 * out — it would also hide views of St. Petersburg and similar townscapes — and the
 * museums' own tags ("Saints", "Virgin Mary", "Christ", "Angels") catch most of what
 * excluding it gives up. Top-level and not a method on [Catalogue] because it has
 * nothing to do with fetching or filtering: it only judges text already in hand.
 *
 * SMK's titles are Danish, so the list also carries a short run of Danish terms for
 * the same handful of common subjects ("kristus", "jomfru maria", "helgen",
 * "apostel", "engel", "korsfæstelse") rather than a full Danish translation of every
 * English entry above — "madonna" is already the same word in both languages. Kept
 * deliberately short: this list only needs to catch what actually turns up.
 */
internal fun isReligious(title: String, tagTerms: List<String>): Boolean {
    val haystacks = listOf(title) + tagTerms
    return haystacks.any { RELIGIOUS_TERMS.containsMatchIn(it) }
}

private val RELIGIOUS_TERMS = Regex(
    "\\b(?:" + listOf(
        "christ", "jesus", "madonna", "virgin", "saints?", "holy", "annunciation",
        "crucifixion", "crucified", "nativity", "adoration", "magi", "piet[aà]",
        "lamentation", "resurrection", "ascension", "assumption", "transfiguration",
        "apostles?", "evangelists?", "baptism", "angels?", "deposition", "entombment",
        "magdalene", "pope", "bible", "biblical", "gospel", "prophets?",
        "martyrs?", "martyrdom", "last supper", "pentecost", "flight into egypt",
        "moses", "abraham", "noah", "jonah", "tobias", "judith", "susanna", "samson",
        "buddha", "bodhisattva", "arhat", "deit(?:y|ies)",
        "kristus", "jomfru maria", "helgen", "apostel", "engel", "korsfæstelse",
    ).joinToString("|") + ")\\b",
    RegexOption.IGNORE_CASE,
)
