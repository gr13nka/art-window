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
                val met = Met(context.cacheDir)
                val artwork = met.fetch(state.artwork, screen)
                Wallpaper.pin(context, artwork.path, screen)
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
}
