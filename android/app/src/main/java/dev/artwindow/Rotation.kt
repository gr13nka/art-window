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

/** Where [Met.fetch] is within one attempt, detailed enough for [MainActivity] to narrate it. */
sealed interface FetchProgress {
    data class Searching(val subject: ArtworkSubject) : FetchProgress
    data class Checking(val looked: Int) : FetchProgress
    data class Downloading(val bytes: Long, val total: Long?) : FetchProgress
}

/** What a turn is doing, for [MainActivity] to show. */
sealed interface Status {
    data object Idle : Status
    data class Fetching(val progress: FetchProgress? = null) : Status
    data object Applying : Status
    data object SavingFavourite : Status
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

            _status.value = Status.Fetching()
            try {
                val screen = context.screen()
                val preferences = WallpaperPreferencesStore(context).load()
                val met = Met(context.cacheDir)
                // The shown work is the one tomorrow must avoid. Favourite copies keep
                // their met-{id} name precisely so the source can recognise them here.
                val artwork = met.fetch(
                    state.shownArtwork,
                    screen,
                    preferences,
                    onProgress = { _status.value = Status.Fetching(it) },
                )
                Wallpaper.pin(context, artwork.path, screen, preferences)
                state.recordFetched(artwork, today)
                met.discardAllBut(artwork.path)
                runCatching { Favourites(context).discardAllBut(artwork.path) }
                    .onFailure { Log.w(LOG_TAG, "cleaning favourites failed", it) }
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
            val artwork = State(context).shownArtwork
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

    /** Adds or removes the shown painting while sharing the wallpaper/cache action lock. */
    fun toggleFavourite(context: Context): Boolean = runLocked(
        status = Status.SavingFavourite,
        failure = "updating favourites",
    ) {
        val shown = State(context).shownArtwork ?: throw IllegalStateException("There is no painting to save")
        val favourites = Favourites(context)
        val existing = favourites.list().firstOrNull { sameArtwork(it.artwork, shown) }
        if (existing == null) favourites.keep(shown) else favourites.forget(existing.key)
        favourites.discardAllBut(shown.path)
    }

    /** Pins one saved painting without replacing the source picture recorded for the day. */
    fun showFavourite(context: Context, key: String): Boolean = runLocked(
        status = Status.Applying,
        failure = "showing favourite",
    ) {
        val favourites = Favourites(context)
        val artwork = favourites.get(key) ?: throw IllegalStateException("That favourite no longer exists")
        val state = State(context)
        val preferences = WallpaperPreferencesStore(context).load()
        Wallpaper.pin(context, artwork.path, context.screen(), preferences)
        state.recordChosen(artwork, LocalDate.now())
        state.fetchedArtwork?.path?.let { Met(context.cacheDir).discardAllBut(it) }
        favourites.discardAllBut(artwork.path)
    }

    fun forgetFavourite(context: Context, key: String): Boolean = runLocked(
        status = Status.SavingFavourite,
        failure = "removing favourite",
    ) {
        val state = State(context)
        val favourites = Favourites(context)
        favourites.forget(key)
        favourites.discardAllBut(state.shownArtwork?.path)
    }

    private fun runLocked(status: Status, failure: String, action: () -> Unit): Boolean {
        if (!running.compareAndSet(false, true)) return false
        return try {
            _status.value = status
            action()
            _status.value = Status.Idle
            true
        } catch (e: Exception) {
            Log.e(LOG_TAG, "$failure failed", e)
            _status.value = Status.Failed(e.message ?: e.toString())
            false
        } finally {
            running.set(false)
        }
    }
}
