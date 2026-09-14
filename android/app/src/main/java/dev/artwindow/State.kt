package dev.artwindow

import android.content.Context
import java.io.File
import java.time.LocalDate

/**
 * What survives the process: the day the last picture settled on, the picture fetched
 * for that day, and the picture currently shown. [Rotation] is this class's only writer.
 *
 * `settledDay` is a calendar day (an epoch day), never a timestamp or a countdown —
 * the same rule `state.json`'s scheduler invariant states on the Rust side, and for
 * the same reason: a machine that slept past the appointed moment must still see
 * today as owed, and [isDue] is the whole of that decision.
 */
class State(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private val settledDay: Long?
        get() = if (prefs.contains(KEY_SETTLED_DAY)) prefs.getLong(KEY_SETTLED_DAY, 0) else null

    fun isDue(today: LocalDate): Boolean = settledDay != today.toEpochDay()

    /** The source picture that settled the day and must survive while a favourite is shown. */
    val fetchedArtwork: Artwork?
        get() = readArtwork("")

    /** The picture currently represented by the app, falling back to the legacy single record. */
    val shownArtwork: Artwork?
        get() = readArtwork(SHOWN_PREFIX) ?: fetchedArtwork

    /** Stamps [today] and records [artwork] in one commit, so a crash mid-write can never split the two apart. */
    fun recordFetched(artwork: Artwork, today: LocalDate) {
        prefs.edit().apply {
            putLong(KEY_SETTLED_DAY, today.toEpochDay())
            putArtwork("", artwork)
            putArtwork(SHOWN_PREFIX, artwork)
        }.commit()
    }

    /** Records a hand-picked picture without replacing the source picture for the day. */
    fun recordChosen(artwork: Artwork, today: LocalDate) {
        prefs.edit().apply {
            if (isDue(today)) putLong(KEY_SETTLED_DAY, today.toEpochDay())
            putArtwork(SHOWN_PREFIX, artwork)
        }.commit()
    }

    private fun readArtwork(prefix: String): Artwork? {
        val path = prefs.getString(prefix + KEY_PATH, null) ?: return null
        return Artwork(
            title = prefs.getString(prefix + KEY_TITLE, "") ?: "",
            byline = prefs.getString(prefix + KEY_BYLINE, "") ?: "",
            attribution = prefs.getString(prefix + KEY_ATTRIBUTION, "") ?: "",
            detailsUrl = prefs.getString(prefix + KEY_DETAILS_URL, null),
            origin = prefs.getString(prefix + KEY_ORIGIN, null),
            path = File(path),
        )
    }

    private fun android.content.SharedPreferences.Editor.putArtwork(prefix: String, artwork: Artwork) {
        putString(prefix + KEY_TITLE, artwork.title)
        putString(prefix + KEY_BYLINE, artwork.byline)
        putString(prefix + KEY_ATTRIBUTION, artwork.attribution)
        putString(prefix + KEY_DETAILS_URL, artwork.detailsUrl)
        putString(prefix + KEY_ORIGIN, artwork.origin)
        putString(prefix + KEY_PATH, artwork.path.path)
    }

    private companion object {
        const val PREFS_NAME = "art_window"
        const val KEY_SETTLED_DAY = "settled_day"
        const val KEY_TITLE = "title"
        const val KEY_BYLINE = "byline"
        const val KEY_ATTRIBUTION = "attribution"
        const val KEY_DETAILS_URL = "details_url"
        const val KEY_PATH = "path"
        const val KEY_ORIGIN = "origin"
        const val SHOWN_PREFIX = "shown_"
    }
}
