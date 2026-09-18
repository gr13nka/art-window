package dev.artwindow

import android.graphics.BitmapFactory
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/** "Wheat Field with Cypresses" — one picture, ready to hang, with what a viewer would want to know about it. Mirrors Rust's `Artwork` in `art/mod.rs`. */
data class Artwork(
    val title: String,
    val byline: String,
    val attribution: String,
    val detailsUrl: String?,
    val origin: String?,
    val path: File,
)

/**
 * Recovers the object id a [Met] download spelled into its own filename, or `null`
 * if [file] came from somewhere else.
 *
 * The id has to outlive the process so tomorrow's painting is not today's, and the
 * download already spells it into the filename; remembering it a second time would
 * only create something that could disagree with the file on disk. Internal only for
 * [dev.artwindow.MetNamesTest] — recognising this source's own work is exactly the
 * knowledge that has no business leaving this file.
 */
internal fun idOf(file: File): Long? {
    val stripped = file.nameWithoutExtension.removePrefix("met-")
    if (stripped == file.nameWithoutExtension) return null
    return stripped.toLongOrNull()
}

/**
 * The Metropolitan Museum of Art's open-access collection, filtered to the
 * artwork-shape breadth chosen in Settings.
 *
 * Mirrors `src/art/met.rs` on the desktop side: the same User-Agent, the same
 * portrait skip, and the same `met-{id}.{ext}` filename convention [idOf] reads
 * back. The subject queried and the religious-scene filter are Android-only
 * preferences for now — the desktop keeps a fixed `q=landscape` and no religious
 * filter; see the exception CLAUDE.md's Android section names. A change to the
 * User-Agent or the filename convention still belongs in both files.
 *
 * The saved origin set feeds the pool. Europe and Asia are the default because
 * European canvases provide breadth while Asian hanging scrolls are often close to
 * phone-shaped already. Every geography asks for the same chosen subject: a generic
 * query returns mostly portraits.
 */
class Met(private val cacheDir: File) {

    /**
     * Finds a painting matching the saved shape and rendering preferences,
     * downloads it, and returns it. [avoid]'s object id, if any, is skipped, so a
     * rotation does not repeat yesterday's picture.
     *
     * Candidates are tried strictly one at a time — parallel bursts against the
     * Met's API get blocked — up to [CANDIDATES] of them or [BUDGET_MS], whichever
     * comes first.
     */
    fun fetch(
        avoid: Artwork?,
        screen: Screen,
        preferences: WallpaperPreferences,
        onProgress: (FetchProgress) -> Unit = {},
    ): Artwork {
        val avoidId = avoid?.let { idOf(it.path) }
        val started = System.currentTimeMillis()
        val deadline = started + BUDGET_MS
        // One subject for this attempt; the fallback below may widen to the rest of
        // preferences.artworkSubjects, but this is the one the primary pass — and the
        // "Searching" progress line, which can only narrate one subject at a time — uses.
        val subject = preferences.artworkSubjects.random()
        val needsFallback = !preferences.artworkRegions.containsAll(ArtworkRegion.DEFAULT) ||
            preferences.artworkSubjects.size > 1
        val primaryDeadline = if (needsFallback) started + PRIMARY_BUDGET_MS else deadline
        val primaryLimit = if (needsFallback) PRIMARY_CANDIDATES else CANDIDATES
        var looked = 0
        var lastError: Exception? = null
        val tried = mutableSetOf<Long>()

        fun tryCandidates(ids: List<Long>, limit: Int, phaseDeadline: Long): Artwork? {
            for (id in ids) {
                if (looked >= limit || System.currentTimeMillis() >= phaseDeadline) break
                if (id == avoidId || !tried.add(id)) continue
                looked++
                onProgress(FetchProgress.Checking(looked))

                try {
                    val obj = fetchObject(id, screen, preferences) ?: continue
                    if (!previewHolds(obj, screen, preferences.artworkShape)) continue
                    val file = downloadAndPlace(obj, screen, preferences, onProgress) ?: continue

                    Log.i(LOG_TAG, "object $id fits after $looked lookups")
                    return Artwork(
                        title = obj.title,
                        byline = obj.byline,
                        attribution = "The Metropolitan Museum of Art",
                        detailsUrl = obj.objectUrl,
                        origin = obj.origin,
                        path = file,
                    )
                } catch (e: Exception) {
                    lastError = e
                }
            }
            return null
        }

        onProgress(FetchProgress.Searching(subject))
        val primaryPairs = subject.queries.flatMap { query ->
            preferences.artworkRegions.sortedBy { it.ordinal }.map { query to it }
        }
        val primary = candidateIds(primaryPairs)
        tryCandidates(primary, primaryLimit, primaryDeadline)?.let { return it }
        if (needsFallback && System.currentTimeMillis() < deadline) {
            onProgress(FetchProgress.Searching(subject))
            // Generalised fallback: every subject the user chose, not just this attempt's
            // one, paired with the chosen regions widened to include the established
            // default pool — the same widening the region-only fallback always did.
            val fallbackRegions = preferences.artworkRegions + ArtworkRegion.DEFAULT
            val fallbackPairs = preferences.artworkSubjects.flatMap { s ->
                s.queries.flatMap { query -> fallbackRegions.sortedBy { it.ordinal }.map { query to it } }
            }
            val fallback = candidateIds(fallbackPairs)
            tryCandidates(fallback, CANDIDATES, deadline)?.let { return it }
        }

        // Nearly every candidate is turned away rather than failing, so the last network
        // error alone would misname the outcome — the first run on a phone reported a 404
        // for what was really 120 paintings of the wrong shape. It rides along as the cause.
        throw IOException("none of $looked paintings fit a ${screen.width}x${screen.height} screen", lastError)
    }

    /** Deletes every download this source made except [keep]; a file belongs to it exactly when [idOf] recognises its name. */
    fun discardAllBut(keep: File) {
        val files = cacheDir.listFiles() ?: return
        for (file in files) {
            if (file.isFile && file != keep && idOf(file) != null) {
                file.delete()
            }
        }
    }

    private fun candidateIds(pairs: List<Pair<String, ArtworkRegion>>): List<Long> {
        return searchIds(pairs).distinct().shuffled()
    }

    private fun searchIds(pairs: List<Pair<String, ArtworkRegion>>): List<Long> {
        val ids = mutableListOf<Long>()
        pairs.forEach { (query, region) -> ids += searchIds(query, region) }
        return ids
    }

    private fun searchIds(query: String, region: ArtworkRegion): List<Long> {
        val ids = mutableListOf<Long>()
        var offset = 0
        var total: Int
        do {
            val results = getJson(searchUrl(query, region, offset))
            total = results.optInt("total", 0).coerceAtMost(MAX_SEARCH_RESULTS)
            val page = results.optJSONArray("objectIDs") ?: JSONArray()
            if (page.length() == 0) break
            repeat(page.length()) { ids += page.getLong(it) }
            offset += SEARCH_PAGE_SIZE
        } while (offset < total)
        return ids
    }

    /**
     * Null means "skip this candidate": no usable image, not classified as a
     * painting, a portrait, a shape no measurement supports, or — when
     * [WallpaperPreferences.hideReligious] is on — a religious scene.
     */
    private fun fetchObject(id: Long, screen: Screen, preferences: WallpaperPreferences): MetObject? {
        val json = getJson("$API/objects/$id")
        val primaryImage = json.optString("primaryImage", "")
        if (primaryImage.isEmpty()) return null
        if (json.optString("classification", "") != "Paintings") return null

        val title = json.optString("title", "").trim()
        val tags = json.optJSONArray("tags")
        val tagTerms = tags?.let { array -> (0 until array.length()).map { array.getJSONObject(it).optString("term") } }.orEmpty()

        val isPortrait = title.contains("portrait", ignoreCase = true) ||
            tagTerms.any { it.equals("Portraits", ignoreCase = true) }
        if (isPortrait) return null
        if (preferences.hideReligious && isReligious(title, tagTerms)) return null

        val measurements = json.optJSONArray("measurements") ?: JSONArray()
        val measuredAspects = (0 until measurements.length()).mapNotNull { i ->
            val element = measurements.getJSONObject(i).optJSONObject("elementMeasurements")
            val w = element?.optDouble("Width") ?: Double.NaN
            val h = element?.optDouble("Height") ?: Double.NaN
            if (w.isFinite() && h.isFinite() && h > 0.0) w / h else null
        }
        if (measuredAspects.isNotEmpty() && measuredAspects.none { preferences.artworkShape.mightAccept(it, screen) }) return null

        val artist = json.optString("artistDisplayName", "").trim()
        val date = json.optString("objectDate", "").trim()
        val origin = json.optString("country", "").trim()
            .ifEmpty { json.optString("culture", "").trim() }
            .takeIf { it.isNotEmpty() }
        val byline = when {
            artist.isEmpty() && date.isEmpty() -> ""
            artist.isEmpty() -> date
            date.isEmpty() -> artist
            else -> "$artist, $date"
        }

        return MetObject(
            objectId = id,
            title = title.ifEmpty { "Untitled" },
            byline = byline,
            objectUrl = json.optString("objectURL", ""),
            primaryImage = primaryImage,
            primaryImageSmall = json.optString("primaryImageSmall", ""),
            origin = origin,
        )
    }

    /**
     * Asks the Met's web-sized copy whether the photograph really has the shape the
     * catalogue promised, before the original is fetched.
     *
     * Most candidates that pass [Screen.mightHold] fail here: a folding screen or a
     * triptych lists one tall panel among its measurements and is photographed whole.
     * The copy costs a few hundred kilobytes where the original costs up to tens of
     * megabytes, and a phone pays for every one of them.
     */
    private fun previewHolds(obj: MetObject, screen: Screen, shape: ArtworkShape): Boolean {
        if (obj.primaryImageSmall.isEmpty()) return true // nothing cheaper to ask; the original decides
        val connection = openConnection(obj.primaryImageSmall)
        val bytes = try {
            val buffer = java.io.ByteArrayOutputStream()
            connection.inputStream.use { copyLimited(it, buffer, MAX_PREVIEW_BYTES) }
            buffer.toByteArray()
        } finally {
            connection.disconnect()
        }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        return bounds.outWidth > 0 && bounds.outHeight > 0 &&
            shape.accepts(bounds.outWidth.toDouble() / bounds.outHeight, screen)
    }

    /** Downloads [obj]'s image and checks its real shape and usable resolution. */
    private fun downloadAndPlace(
        obj: MetObject,
        screen: Screen,
        preferences: WallpaperPreferences,
        onProgress: (FetchProgress) -> Unit,
    ): File? {
        val file = download(obj.primaryImage, obj.objectId, onProgress)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        val width = bounds.outWidth
        val height = bounds.outHeight
        val shapeFits = width > 0 && height > 0 &&
            preferences.artworkShape.accepts(width.toDouble() / height, screen)
        val renderable = when (preferences.style) {
            WallpaperStyle.ZOOM -> screen.cover(width, height) != null
            WallpaperStyle.STRETCH -> screen.canStretch(width, height)
            WallpaperStyle.BLUR -> if (preferences.blurVariant == BlurVariant.BACKDROP) {
                screen.fit(width, height) != null
            } else {
                screen.cover(width, height) != null
            }
            WallpaperStyle.BORDERS -> screen.fit(width, height) != null
        }
        if (!shapeFits || !renderable) {
            file.delete()
            return null
        }
        return file
    }

    private fun download(url: String, id: Long, onProgress: (FetchProgress) -> Unit): File {
        val connection = openConnection(url)
        try {
            val length = connection.getHeaderField("Content-Length")?.toLongOrNull()
            if (length != null && length > MAX_IMAGE_BYTES) {
                throw IOException("image is $length bytes, over the $MAX_IMAGE_BYTES-byte limit")
            }

            val extension = url.substringAfterLast('.', "jpg").take(4)
            cacheDir.mkdirs()
            // Load-bearing: idOf reads the object id back out of this name, which is how
            // tomorrow's painting avoids being today's.
            val file = File(cacheDir, "met-$id.$extension")
            // Throttled so the StateFlow isn't flooded: a whole-percent change when the
            // server sent Content-Length, or every ~256 KB when it didn't.
            var lastPercent = -1
            var lastReportedBytes = 0L
            file.outputStream().use { out ->
                connection.inputStream.use { input ->
                    copyLimited(input, out, MAX_IMAGE_BYTES) { bytes ->
                        if (length != null) {
                            val percent = ((bytes * 100) / length).toInt()
                            if (percent != lastPercent) {
                                lastPercent = percent
                                onProgress(FetchProgress.Downloading(bytes, length))
                            }
                        } else if (bytes - lastReportedBytes >= DOWNLOAD_REPORT_BYTES) {
                            lastReportedBytes = bytes
                            onProgress(FetchProgress.Downloading(bytes, null))
                        }
                    }
                }
            }
            return file
        } finally {
            connection.disconnect()
        }
    }

    private fun getJson(url: String): JSONObject {
        val connection = openConnection(url)
        try {
            return JSONObject(connection.inputStream.bufferedReader().use { it.readText() })
        } finally {
            connection.disconnect()
        }
    }

    private fun openConnection(url: String): HttpURLConnection {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.connectTimeout = REQUEST_TIMEOUT_MS
        connection.readTimeout = REQUEST_TIMEOUT_MS
        connection.setRequestProperty("User-Agent", USER_AGENT)
        connection.connect()
        val status = connection.responseCode
        if (status !in 200..299) {
            connection.disconnect()
            throw IOException("$url returned HTTP $status")
        }
        return connection
    }

    private fun copyLimited(
        input: java.io.InputStream,
        output: java.io.OutputStream,
        limit: Long,
        onBytesCopied: ((Long) -> Unit)? = null,
    ) {
        val buffer = ByteArray(8192)
        var total = 0L
        while (total < limit) {
            val read = input.read(buffer, 0, minOf(buffer.size.toLong(), limit - total).toInt())
            if (read < 0) break
            output.write(buffer, 0, read)
            total += read
            onBytesCopied?.invoke(total)
        }
    }

    private data class MetObject(
        val objectId: Long,
        val title: String,
        val byline: String,
        val objectUrl: String,
        val primaryImage: String,
        val primaryImageSmall: String,
        val origin: String?,
    )

    companion object {
        private const val API = "https://collectionapi.metmuseum.org/public/collection/v1"
        private const val SEARCH_API = "https://collectionapi.metmuseum.org/public/collection/v1.1/search"

        // Every chosen geography asks for the same subject query: a generic query comes
        // back mostly portraits. `medium` keeps regions with broad collections to
        // paintings rather than spending object lookups on ceramics and textiles. The
        // subject itself is now a Settings preference — see the class doc — unlike the
        // desktop's fixed `q=landscape`.
        private const val USER_AGENT = "ArtWindow-Android/0.1.0 (+https://github.com/gr13nka/art-window)"

        /** The Met serves originals with no server-side resizing; this is the only size control there is. */
        private const val MAX_IMAGE_BYTES = 64L * 1024 * 1024

        /** Checking the web-sized copy's shape needs its header, not a painting's worth of bytes. */
        private const val MAX_PREVIEW_BYTES = 4L * 1024 * 1024

        /** How often a download with no Content-Length reports progress. */
        private const val DOWNLOAD_REPORT_BYTES = 256L * 1024

        /**
         * How many objects to try before giving up. Only about one in sixty fits a
         * phone, so this is sized to make running dry rare; [BUDGET_MS] is the limit
         * that usually binds.
         */
        private const val CANDIDATES = 400
        private const val PRIMARY_CANDIDATES = 300
        private const val SEARCH_PAGE_SIZE = 500
        private const val MAX_SEARCH_RESULTS = 10_000

        /** Generous enough for an original-resolution painting on a link that has just woken up, and no more. */
        private const val REQUEST_TIMEOUT_MS = 45_000

        /** How long a whole fetch may take before it gives up and leaves the day for the next attempt. */
        private const val BUDGET_MS = 3 * 60 * 1000L
        private const val PRIMARY_BUDGET_MS = 2 * 60 * 1000L

        internal fun searchUrl(query: String, region: ArtworkRegion, offset: Int): String {
            val encodedQuery = URLEncoder.encode(query, Charsets.UTF_8.name())
            val encodedRegion = URLEncoder.encode(region.apiName, Charsets.UTF_8.name())
            return "$SEARCH_API?medium=Paintings&hasImages=true&isPublicDomain=true" +
                "&q=$encodedQuery&geoLocation=$encodedRegion&limit=$SEARCH_PAGE_SIZE&offset=$offset"
        }
    }
}

/**
 * Whether [title] or one of [tagTerms] names a religious scene or figure, for
 * [WallpaperPreferences.hideReligious].
 *
 * One whole-word, case-insensitive regex rather than per-word substring checks, so
 * that "Christmas" is never mistaken for "Christ" and short fragments like "holy"
 * or "magi" don't fire inside an unrelated longer word. Multi-word phrases such as
 * "last supper" match as a phrase for the same reason. "St." is deliberately left
 * out — it would also hide views of St. Petersburg and similar cityscapes — and the
 * Met's own tags ("Saints", "Virgin Mary", "Christ", "Angels") catch most of what
 * excluding it gives up. Top-level and not a method on [Met] because it has nothing
 * to do with fetching: it only judges text already in hand.
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
    ).joinToString("|") + ")\\b",
    RegexOption.IGNORE_CASE,
)
