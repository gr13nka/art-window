package dev.artwindow

import android.graphics.BitmapFactory
import android.util.Log
import java.io.File
import java.io.IOException
import java.net.CookieHandler
import java.net.CookieManager
import java.net.CookiePolicy
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.abs

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
 * Recovers `{source}-{id}` from a filename [Museums] wrote, or `null` if [file] came
 * from somewhere else.
 *
 * The key has to outlive the process so tomorrow's painting is not today's, and a
 * download already spells it into the filename; remembering it a second time would
 * only create something that could disagree with the file on disk. A legacy
 * `met-{id}.jpg` from before the four-museum catalogue parses the same way, since
 * "met" is still a recognised [MuseumSource.code] — that is what keeps an old
 * favourite or the state file's shown picture usable after this change. Internal only
 * for tests — recognising this module's own work has no business leaving this file.
 */
internal fun keyOf(file: File): String? {
    val stem = file.nameWithoutExtension
    val source = MuseumSource.entries.firstOrNull { stem.startsWith("${it.code}-") } ?: return null
    val id = stem.removePrefix("${source.code}-")
    return id.takeIf { it.isNotEmpty() }?.let { "${source.code}-$it" }
}

/** Recovers which [MuseumSource] wrote a [keyOf] key, or `null` if [key] is `null` or unrecognised. */
internal fun museumSourceOfKey(key: String?): MuseumSource? =
    key?.let { k -> MuseumSource.entries.firstOrNull { k.startsWith("${it.code}-") } }

/**
 * Downloads one painting from the local catalogue [catalogue] built by
 * `catalogue/build.py` — the collection is picked and shape/render-filtered entirely
 * on the device, by [Catalogue.candidates], so a turn makes exactly one HTTP request
 * to fetch an image rather than a live search plus per-candidate lookups.
 *
 * [MAX_ATTEMPTS] exists only because the catalogue's own numbers can drift from the
 * file actually served — a museum re-encodes an image between the catalogue's build
 * and this download. The decoded size is checked against what the catalogue promised,
 * and a mismatch discards the file and moves on to the next candidate rather than
 * showing a picture that doesn't fit.
 */
class Museums(private val cacheDir: File, private val catalogue: Catalogue) {

    init {
        // A museum's CDN can treat a client that never returns its session cookie as a
        // fresh bot on every request; HttpURLConnection only keeps cookies when a
        // CookieHandler is installed, and there can only be one for the whole process —
        // guarded so the next Museums (a fresh one is made every turn) doesn't stomp on
        // the handler already set.
        if (CookieHandler.getDefault() == null) {
            CookieHandler.setDefault(CookieManager(null, CookiePolicy.ACCEPT_ORIGINAL_SERVER))
        }
    }

    /**
     * Finds a painting matching the saved shape and rendering preferences, downloads
     * it, and returns it. [avoid]'s file, if any, is skipped by its [keyOf] key, so a
     * rotation does not repeat yesterday's picture.
     */
    fun fetch(
        avoid: Artwork?,
        screen: Screen,
        preferences: WallpaperPreferences,
        onProgress: (FetchProgress) -> Unit = {},
    ): Artwork {
        val avoidKey = avoid?.let { keyOf(it.path) }
        // Shape, Origins, Subjects and Artists are sections a painting must pass all of
        // (empty means Any within a section, so a section a person left untouched never
        // narrows anything) — Catalogue.candidates already shuffles every qualifying
        // entry, so there is no per-choice draw or fallback pass left to do here.
        val ordered = catalogue.candidates(preferences, screen)
            .filterNot { avoidKey != null && "${it.source.code}-${it.id}" == avoidKey }
        if (ordered.isEmpty()) {
            throw IOException("No paintings match these filters")
        }

        onProgress(FetchProgress.Searching)
        var lastError: Exception? = null
        var attempted = 0
        for (entry in ordered.take(MAX_ATTEMPTS)) {
            attempted++
            try {
                val file = download(entry, onProgress)
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(file.path, bounds)
                val width = bounds.outWidth
                val height = bounds.outHeight
                val matches = width > 0 && height > 0 &&
                    closeEnough(width, entry.width) && closeEnough(height, entry.height) &&
                    preferences.canRender(width, height, screen)
                if (matches) {
                    Log.i(LOG_TAG, "${entry.source.code}-${entry.id} downloaded after $attempted attempt(s)")
                    return Artwork(
                        title = entry.title.ifEmpty { "Untitled" },
                        byline = entry.byline,
                        attribution = entry.source.displayName,
                        detailsUrl = entry.detailsUrl.takeIf { it.isNotEmpty() },
                        origin = entry.origin.takeIf { it.isNotEmpty() },
                        path = file,
                    )
                }
                file.delete()
            } catch (e: Refused) {
                // A refusal means a museum's CDN is throttling this client rather than
                // objecting to this one candidate, so it is rethrown instead of moving on
                // to the next entry, which would just keep hammering a client already
                // being throttled and extend the block.
                throw e
            } catch (e: Exception) {
                lastError = e
            }
        }
        throw IOException("none of $attempted downloaded paintings matched the catalogue", lastError)
    }

    /** Deletes every download this source made except [keep]; a file belongs to it exactly when [keyOf] recognises its name. */
    fun discardAllBut(keep: File) {
        val files = cacheDir.listFiles() ?: return
        for (file in files) {
            if (file.isFile && file != keep && keyOf(file) != null) {
                file.delete()
            }
        }
    }

    /** Downloads [entry]'s image to `{source}-{id}.{ext}` in [cacheDir]. */
    private fun download(entry: Catalogue.Entry, onProgress: (FetchProgress) -> Unit): File {
        val connection = openConnection(entry.imageUrl, entry.source.displayName)
        try {
            val length = connection.getHeaderField("Content-Length")?.toLongOrNull()
            if (length != null && length > MAX_IMAGE_BYTES) {
                throw IOException("image is $length bytes, over the $MAX_IMAGE_BYTES-byte limit")
            }

            val extension = entry.imageUrl.substringAfterLast('.', "jpg").substringBefore('?').take(4)
            cacheDir.mkdirs()
            // Load-bearing: keyOf reads the source and id back out of this name, which is
            // how tomorrow's painting avoids being today's and how discardAllBut
            // recognises this module's own downloads.
            val file = File(cacheDir, "${entry.source.code}-${entry.id}.$extension")
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

    /**
     * Opens [url] after [pace] enforces the minimum gap since the last request to this
     * URL's host. A 403 or 429 throws [Refused] rather than the generic error below,
     * since those two mean the CDN is throttling this client, not complaining about
     * this one URL. [sourceName] names nothing about the request itself — it only
     * travels through to [Refused]'s message, so that message can name the museum
     * being asked rather than speaking of "a museum" in general.
     */
    private fun openConnection(url: String, sourceName: String): HttpURLConnection {
        val target = URL(url)
        pace(target.host)
        val connection = target.openConnection() as HttpURLConnection
        connection.connectTimeout = REQUEST_TIMEOUT_MS
        connection.readTimeout = REQUEST_TIMEOUT_MS
        connection.setRequestProperty("User-Agent", USER_AGENT)
        connection.connect()
        val status = connection.responseCode
        if (status == 403 || status == 429) {
            connection.disconnect()
            throw Refused(sourceName, url, status)
        }
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

    /** Whether [actual] pixels is within [TOLERANCE] of the [catalogued] figure the TSV promised. */
    private fun closeEnough(actual: Int, catalogued: Int): Boolean =
        abs(actual - catalogued) <= catalogued * TOLERANCE

    /**
     * Thrown by [openConnection] when a museum's CDN answers 403 or 429 — a refusal
     * aimed at this client, not a complaint about one candidate. [fetch] rethrows it
     * instead of recording it as `lastError` and moving on to the next entry. The
     * message is the sentence a person reads on screen if this reaches them uncaught,
     * naming [sourceName] so it is clear which museum is throttling; [url] and
     * [status] travel in the cause instead, for the log rather than the phone.
     */
    private class Refused(sourceName: String, url: String, status: Int) : IOException(
        "$sourceName is refusing requests from this phone for now — try again later",
        IOException("$url returned HTTP $status"),
    )

    companion object {
        private const val USER_AGENT = "ArtWindow-Android/0.1.0 (+https://github.com/gr13nka/art-window)"

        /** No museum here resizes on request; this is the only size control there is. */
        private const val MAX_IMAGE_BYTES = 64L * 1024 * 1024

        /** How often a download with no Content-Length reports progress. */
        private const val DOWNLOAD_REPORT_BYTES = 256L * 1024

        /**
         * How much a downloaded image's decoded pixel size may differ from the
         * catalogue's own figure and still count as the painting the catalogue promised.
         */
        private const val TOLERANCE = 0.02

        /** Catalogue entries tried, at most, before a fetch gives up on the day. */
        private const val MAX_ATTEMPTS = 3

        /** Generous enough for an original-resolution painting on a link that has just woken up, and no more. */
        private const val REQUEST_TIMEOUT_MS = 45_000

        /** Minimum gap between requests to the same host, so a run of downloads from one museum reads as a person turning pages, not a burst its CDN blocks. */
        private const val REQUEST_GAP_MS = 750L

        private val lastRequestAt = ConcurrentHashMap<String, Long>()

        /**
         * Blocks the calling thread until at least [REQUEST_GAP_MS] has passed since the
         * last request to [host]. `fetch` already runs on a worker thread, so a sleep
         * here is cheap. Synchronized so two requests to the same host can't both read a
         * stale timestamp, decide they're clear, and fire together; different hosts pace
         * independently, since each museum's CDN enforces its own limit.
         */
        @Synchronized
        private fun pace(host: String) {
            val wait = REQUEST_GAP_MS - (System.currentTimeMillis() - (lastRequestAt[host] ?: 0L))
            if (wait > 0) Thread.sleep(wait)
            lastRequestAt[host] = System.currentTimeMillis()
        }
    }
}
