package dev.artwindow

import android.content.Context
import java.io.File
import java.time.LocalDate

/**
 * What survives the process: the day the last picture settled on, and the picture
 * itself. [Rotation] is this class's only writer.
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

    /** The picture currently on the wallpaper, or `null` before the first rotation ever completes. */
    val artwork: Artwork?
        get() {
            val path = prefs.getString(KEY_PATH, null) ?: return null
            return Artwork(
                title = prefs.getString(KEY_TITLE, "") ?: "",
                byline = prefs.getString(KEY_BYLINE, "") ?: "",
                attribution = prefs.getString(KEY_ATTRIBUTION, "") ?: "",
                detailsUrl = prefs.getString(KEY_DETAILS_URL, null),
                path = File(path),
            )
        }

    /** Stamps [today] and records [artwork] in one commit, so a crash mid-write can never split the two apart. */
    fun recordFetched(artwork: Artwork, today: LocalDate) {
        prefs.edit()
            .putLong(KEY_SETTLED_DAY, today.toEpochDay())
            .putString(KEY_TITLE, artwork.title)
            .putString(KEY_BYLINE, artwork.byline)
            .putString(KEY_ATTRIBUTION, artwork.attribution)
            .putString(KEY_DETAILS_URL, artwork.detailsUrl)
            .putString(KEY_PATH, artwork.path.path)
            .commit()
    }

    private companion object {
        const val PREFS_NAME = "art_window"
        const val KEY_SETTLED_DAY = "settled_day"
        const val KEY_TITLE = "title"
        const val KEY_BYLINE = "byline"
        const val KEY_ATTRIBUTION = "attribution"
        const val KEY_DETAILS_URL = "details_url"
        const val KEY_PATH = "path"
    }
}
