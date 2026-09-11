package dev.artwindow

import android.content.Context
import android.util.Log
import java.time.LocalDate
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** The tag every part of this app logs under; `adb logcat -s ArtWindow` follows a rotation end to end. */
internal const val LOG_TAG = "ArtWindow"

/** What a turn is doing, for [MainActivity] to show. */
sealed interface Status {
    data object Idle : Status
    data object Fetching : Status
    data object Applying : Status
    data class Failed(val message: String) : Status
}

/**
 * One turn of the rotation — the only place [Met.fetch] is ever called.
 *
 * Mirrors the desktop's `rotation::show`: the day only advances once the wallpaper is
 * actually up ([State.recordFetched] runs after [Wallpaper.pin] succeeds), so a
 * failed attempt leaves the day unspent for the next scheduled run to retry.
 */
object Rotation {
    private val running = AtomicBoolean(false)
    private val _status = MutableStateFlow<Status>(Status.Idle)
    val status: StateFlow<Status> = _status.asStateFlow()

    /**
     * Runs one turn if [force] or a picture is owed; does nothing if a turn is
     * already in flight, rather than queueing — the scheduler and *Next picture*
     * racing for the wallpaper would otherwise leave a loser writing into a cache
     * the winner's [Met.discardAllBut] sweep already ran for.
     */
    fun turn(context: Context, force: Boolean) {
        if (!running.compareAndSet(false, true)) return
        try {
            val state = State(context)
            val today = LocalDate.now()
            if (!force && !state.isDue(today)) return

            _status.value = Status.Fetching
            try {
                val screen = context.screen()
                val preferences = WallpaperPreferencesStore(context).load()
                val met = Met(context.cacheDir)
                val artwork = met.fetch(state.artwork, screen, preferences)
                Wallpaper.pin(context, artwork.path, screen, preferences)
                state.recordFetched(artwork, today)
                met.discardAllBut(artwork.path)
                _status.value = Status.Idle
            } catch (e: Exception) {
                Log.e(LOG_TAG, "rotation failed", e)
                _status.value = Status.Failed(e.message ?: e.toString())
            }
        } finally {
            running.set(false)
        }
    }

    /** Re-renders the cached painting and commits [preferences] as one serialized user action. */
    fun applyPreferences(context: Context, preferences: WallpaperPreferences): Boolean {
        if (!running.compareAndSet(false, true)) return false
        return try {
            _status.value = Status.Applying
            val artwork = State(context).artwork
            if (artwork != null && artwork.path.isFile) {
                Wallpaper.pin(context, artwork.path, context.screen(), preferences)
            }
            if (!WallpaperPreferencesStore(context).save(preferences)) {
                throw IllegalStateException("Could not save wallpaper settings")
            }
            _status.value = Status.Idle
            true
        } catch (e: Exception) {
            Log.e(LOG_TAG, "applying wallpaper settings failed", e)
            _status.value = Status.Failed(e.message ?: e.toString())
            false
        } finally {
            running.set(false)
        }
    }
}
