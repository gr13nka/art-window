package dev.artwindow

import android.graphics.Color
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import android.graphics.drawable.TransitionDrawable
import android.service.dreams.DreamService
import android.view.Gravity
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import java.time.LocalDate
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The television's screensaver: whatever [Frame] holds, full screen, for as long as the
 * system leaves it running.
 *
 * It only observes. The picture is already composed at the screen's size under the
 * saved preferences, so the view just fits it; and a dream never fetches — when a
 * picture is owed it asks [RotationJob] like every other surface and the new one
 * arrives through [Frame.shown], crossfading in.
 */
class ArtDream : DreamService() {
    private lateinit var picture: ImageView
    private lateinit var caption: TextView
    private var scope: CoroutineScope? = null
    private var captionFade: Job? = null

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        isFullscreen = true
        isInteractive = false
        isScreenBright = true

        val root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        picture = ImageView(this).apply { scaleType = ImageView.ScaleType.FIT_CENTER }
        root.addView(picture, ViewGroup.LayoutParams(MATCH, MATCH))
        val margin = (CAPTION_MARGIN_DP * resources.displayMetrics.density).toInt()
        caption = TextView(this).apply {
            setTextColor(Color.WHITE)
            setShadowLayer(6f, 0f, 2f, Color.BLACK)
            textSize = 16f
            alpha = 0f
        }
        root.addView(
            caption,
            FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                gravity = Gravity.BOTTOM or Gravity.START
                setMargins(margin, margin, margin, margin)
            },
        )
        setContentView(root)

        RotationJob.scheduleDaily(applicationContext)
        if (State(applicationContext).isDue(LocalDate.now())) {
            RotationJob.scheduleNow(applicationContext, force = false)
        }

        val watching = CoroutineScope(SupervisorJob() + Dispatchers.Main)
        scope = watching
        watching.launch {
            val first = withContext(Dispatchers.Default) { Frame.current(applicationContext) }
            first?.let(::show)
            Frame.shown.filterNotNull().collect(::show)
        }
    }

    override fun onDetachedFromWindow() {
        scope?.cancel()
        scope = null
        super.onDetachedFromWindow()
    }

    /** Crossfades to [rendered]; the first picture, with nothing to fade from, simply appears. */
    private fun show(rendered: Rendered) {
        val next = BitmapDrawable(resources, rendered.bitmap)
        val previous: Drawable? = picture.drawable?.let { (it as? TransitionDrawable)?.getDrawable(1) ?: it }
        if (previous == null) {
            picture.setImageDrawable(next)
        } else if (previous !== next && (previous as? BitmapDrawable)?.bitmap !== rendered.bitmap) {
            picture.setImageDrawable(TransitionDrawable(arrayOf(previous, next)).apply {
                isCrossFadeEnabled = true
                startTransition(CROSSFADE_MS)
            })
        }
        announce(rendered.artwork)
    }

    private fun announce(artwork: Artwork) {
        caption.text = listOf(artwork.title, artwork.byline).filter { it.isNotEmpty() }.joinToString(" · ")
        captionFade?.cancel()
        captionFade = scope?.launch {
            caption.animate().alpha(1f).setDuration(CAPTION_FADE_MS).start()
            delay(CAPTION_VISIBLE_MS)
            caption.animate().alpha(0f).setDuration(CAPTION_FADE_MS).start()
        }
    }

    private companion object {
        const val MATCH = ViewGroup.LayoutParams.MATCH_PARENT
        const val CROSSFADE_MS = 1500
        const val CAPTION_FADE_MS = 600L
        const val CAPTION_VISIBLE_MS = 8_000L
        const val CAPTION_MARGIN_DP = 32
    }
}
