package dev.artwindow

import android.app.UiModeManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

fun Context.isTelevision(): Boolean =
    (getSystemService(Context.UI_MODE_SERVICE) as UiModeManager).currentModeType ==
        Configuration.UI_MODE_TYPE_TELEVISION

/** A picture already composed for the screen, with the work it shows so a caption can be drawn. */
data class Rendered(val bitmap: Bitmap, val artwork: Artwork)

/**
 * Where a TV's painting lives: it has no system wallpaper to hand a bitmap to, so
 * the screensaver and the launchable activity both draw from here.
 *
 * Superseded bitmaps are never recycled. An observer may still be drawing the old
 * one when a new one is published, and recycling under it would crash the draw; the
 * garbage collector frees it once the last observer lets go.
 */
object Frame {
    private val _shown = MutableStateFlow<Rendered?>(null)
    val shown: StateFlow<Rendered?> = _shown.asStateFlow()

    private val lock = Any()

    internal fun publish(rendered: Rendered) {
        synchronized(lock) { _shown.value = rendered }
    }

    /**
     * The painting to show now. After a process restart nothing has been published
     * yet, so it is rendered again from what [State] remembers; `null` if there is
     * no painting or it cannot be rendered. Blocks while rendering, so call it off
     * the main thread.
     */
    fun current(context: Context): Rendered? = synchronized(lock) {
        _shown.value ?: try {
            val artwork = State(context).shownArtwork ?: return null
            val bitmap = WallpaperRenderer.render(
                artwork.path,
                context.screen(),
                WallpaperPreferencesStore(context).load(),
            )
            Rendered(bitmap, artwork).also { _shown.value = it }
        } catch (e: Exception) {
            Log.w(LOG_TAG, "rendering the frame failed", e)
            null
        }
    }
}
