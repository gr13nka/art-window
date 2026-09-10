package dev.artwindow

import android.graphics.BitmapFactory
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** "Wheat Field with Cypresses" — one picture, ready to hang, with what a viewer would want to know about it. Mirrors Rust's `Artwork` in `art/mod.rs`. */
data class Artwork(
    val title: String,
    val byline: String,
    val attribution: String,
    val detailsUrl: String?,
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
 * The Metropolitan Museum of Art's open-access collection, filtered down to
 * paintings a phone screen can hang.
 *
 * Mirrors `src/art/met.rs` on the desktop side: the same User-Agent, the same
 * landscape-subject query and portrait skip, and the same `met-{id}.{ext}` filename
 * convention [idOf] reads back. A change to the query, the User-Agent, or the
 * filename convention belongs in both files.
 *
 * Two departments feed the pool rather than one. European Paintings (11) is almost
 * entirely canvases too wide for a phone; Asian Art (6) holds hanging scrolls that
 * are close to phone-shaped already. Both searches ask for the same subject, for the
 * same reason as the desktop app: a generic query returns mostly portraits.
 */
class Met(private val cacheDir: File) {

    /**
     * Finds a painting [screen] can hang without trimming or enlarging it too far,
     * downloads it, and returns it. [avoid]'s object id, if any, is skipped, so a
     * rotation does not repeat yesterday's picture.
     *
     * Candidates are tried strictly one at a time — parallel bursts against the
     * Met's API get blocked — up to [CANDIDATES] of them or [BUDGET_MS], whichever
     * comes first.
     */
    fun fetch(avoid: Artwork?, screen: Screen): Artwork {
        val avoidId = avoid?.let { idOf(it.path) }
        val ids = candidateIds()
        val deadline = System.currentTimeMillis() + BUDGET_MS
        var looked = 0
        var lastError: Exception? = null

        for (id in ids.take(CANDIDATES)) {
            if (System.currentTimeMillis() >= deadline) break
            if (id == avoidId) continue
            looked++

            try {
                val obj = fetchObject(id, screen) ?: continue // no image, a portrait, or a catalogued shape no phone suits
                if (!previewHolds(obj, screen)) continue // the catalogue passed it; the photograph's own shape did not
                val file = downloadAndPlace(obj, screen) ?: continue // the right shape, too few pixels to fill the screen

                Log.i(LOG_TAG, "object $id fits after $looked lookups")
                return Artwork(
                    title = obj.title,
                    byline = obj.byline,
                    attribution = "The Metropolitan Museum of Art",
                    detailsUrl = obj.objectUrl,
                    path = file,
                )
            } catch (e: Exception) {
                lastError = e
            }
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

    private fun candidateIds(): List<Long> {
        val ids = (searchIds(SEARCH_EUROPEAN) + searchIds(SEARCH_ASIAN)).distinct().shuffled()
        if (ids.isEmpty()) throw IOException("the Met returned no public-domain paintings")
        return ids
    }

    private fun searchIds(query: String): List<Long> {
        val results = getJson("$API/$query")
        val ids = results.optJSONArray("objectIDs") ?: JSONArray()
        return List(ids.length()) { ids.getLong(it) }
    }

    /** Null means "skip this candidate": no usable image, not classified as a painting, a portrait, or a shape no measurement supports. */
    private fun fetchObject(id: Long, screen: Screen): MetObject? {
        val json = getJson("$API/objects/$id")
        val primaryImage = json.optString("primaryImage", "")
        if (primaryImage.isEmpty()) return null
        if (json.optString("classification", "") != "Paintings") return null

        val title = json.optString("title", "").trim()
        val tags = json.optJSONArray("tags")
        val isPortrait = title.contains("portrait", ignoreCase = true) ||
            (tags != null && (0 until tags.length()).any {
                tags.getJSONObject(it).optString("term").equals("Portraits", ignoreCase = true)
            })
        if (isPortrait) return null

        val measurements = json.optJSONArray("measurements") ?: JSONArray()
        val plausibleShape = (0 until measurements.length()).any { i ->
            val element = measurements.getJSONObject(i).optJSONObject("elementMeasurements")
            val w = element?.optDouble("Width") ?: Double.NaN
            val h = element?.optDouble("Height") ?: Double.NaN
            w.isFinite() && h.isFinite() && h != 0.0 && screen.mightHold(w / h)
        }
        if (!plausibleShape) return null

        val artist = json.optString("artistDisplayName", "").trim()
        val date = json.optString("objectDate", "").trim()
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
    private fun previewHolds(obj: MetObject, screen: Screen): Boolean {
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
            screen.holds(bounds.outWidth.toDouble() / bounds.outHeight)
    }

    /** Downloads [obj]'s image and checks its real pixels against [screen]; deletes the file and returns `null` if they do not fit. */
    private fun downloadAndPlace(obj: MetObject, screen: Screen): File? {
        val file = download(obj.primaryImage, obj.objectId)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (screen.place(bounds.outWidth, bounds.outHeight) == null) {
            file.delete()
            return null
        }
        return file
    }

    private fun download(url: String, id: Long): File {
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
            file.outputStream().use { out ->
                connection.inputStream.use { input -> copyLimited(input, out, MAX_IMAGE_BYTES) }
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

    private fun copyLimited(input: java.io.InputStream, output: java.io.OutputStream, limit: Long) {
        val buffer = ByteArray(8192)
        var total = 0L
        while (total < limit) {
            val read = input.read(buffer, 0, minOf(buffer.size.toLong(), limit - total).toInt())
            if (read < 0) break
            output.write(buffer, 0, read)
            total += read
        }
    }

    private data class MetObject(
        val objectId: Long,
        val title: String,
        val byline: String,
        val objectUrl: String,
        val primaryImage: String,
        val primaryImageSmall: String,
    )

    companion object {
        private const val API = "https://collectionapi.metmuseum.org/public/collection/v1"

        // European Paintings (11) is almost entirely too wide for a phone; Asian Art (6) is
        // where the hanging scrolls are. Both ask for the same subject, for the same reason
        // as src/art/met.rs: a generic query there comes back mostly portraits. `medium`
        // is what keeps Asian Art's landscapes to paintings: without it nearly half of
        // what comes back is ceramics, prints and textiles, each a lookup spent for nothing.
        private const val SEARCH_EUROPEAN =
            "search?departmentId=11&medium=Paintings&hasImages=true&isPublicDomain=true&q=landscape"
        private const val SEARCH_ASIAN =
            "search?departmentId=6&medium=Paintings&hasImages=true&isPublicDomain=true&q=landscape"

        private const val USER_AGENT = "ArtWindow-Android/0.1.0 (+https://github.com/gr13nka/art-window)"

        /** The Met serves originals with no server-side resizing; this is the only size control there is. */
        private const val MAX_IMAGE_BYTES = 64L * 1024 * 1024

        /** Checking the web-sized copy's shape needs its header, not a painting's worth of bytes. */
        private const val MAX_PREVIEW_BYTES = 4L * 1024 * 1024

        /**
         * How many objects to try before giving up. Only about one in sixty fits a
         * phone, so this is sized to make running dry rare; [BUDGET_MS] is the limit
         * that usually binds.
         */
        private const val CANDIDATES = 400

        /** Generous enough for an original-resolution painting on a link that has just woken up, and no more. */
        private const val REQUEST_TIMEOUT_MS = 45_000

        /** How long a whole fetch may take before it gives up and leaves the day for the next attempt. */
        private const val BUDGET_MS = 3 * 60 * 1000L
    }
}
