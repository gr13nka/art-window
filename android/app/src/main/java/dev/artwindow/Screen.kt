package dev.artwindow

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * The one place "close enough" is decided for hanging a painting on a phone screen.
 *
 * A phone is tall and narrow, so unlike the desktop app's fit-plus-letterbox, a
 * painting here fills the screen and loses a little of itself at the edges instead.
 * Two checks follow from that, and this is the only type that makes either of them:
 * [mightHold] is a cheap pre-filter against a museum catalogue's stated size, before
 * anything is downloaded; [place] is the real verdict, against a decoded photograph's
 * actual pixels.
 *
 * Always portrait: the constructor sorts whatever a display reports into
 * [width] <= [height], so a landscape-shaped [android.view.Display.Mode] and a
 * portrait one describe the same [Screen].
 */
@ConsistentCopyVisibility
data class Screen private constructor(val width: Int, val height: Int) {

    private val aspect: Double = width.toDouble() / height

    /**
     * A cheap pass against a catalogue's stated size. Museum measurements describe
     * the object (a stretcher, a scroll's mount), not the photograph, so this allows
     * more slack than [place]'s pixel-exact verdict — enough that a painting only
     * [place] would end up refusing rarely costs a download, without being so loose
     * that it lets through shapes [place] would never accept.
     */
    fun mightHold(aspect: Double): Boolean = trimFor(aspect, this.aspect) <= MAX_TRIM + SLACK

    /**
     * Whether a photograph of these proportions fills the screen within [MAX_TRIM].
     * Proportions only, so a web-sized copy can answer for the original before the
     * original is downloaded; [place] adds the question of whether there are enough
     * pixels.
     */
    fun holds(aspect: Double): Boolean = trimFor(aspect, this.aspect) <= MAX_TRIM

    /**
     * How a photograph of size [width] x [height] would sit on this screen if scaled
     * to cover it exactly, or `null` if that costs too much: either edge trimmed past
     * [MAX_TRIM], or the image enlarged past [MAX_ENLARGEMENT] to reach screen size
     * at all.
     */
    fun place(width: Int, height: Int): Placement? {
        if (!holds(width.toDouble() / height)) return null

        val scale = max(this.width.toDouble() / width, this.height.toDouble() / height)
        if (scale > MAX_ENLARGEMENT) return null

        val scaledWidth = ceil(width * scale).toInt()
        val scaledHeight = ceil(height * scale).toInt()
        val cropLeft = (scaledWidth - this.width) / 2
        val cropTop = (scaledHeight - this.height) / 2
        return Placement(
            scaledWidth = scaledWidth,
            scaledHeight = scaledHeight,
            crop = Box(cropLeft, cropTop, cropLeft + this.width, cropTop + this.height),
        )
    }

    companion object {
        /** Neither axis of a placed image may lose more than this fraction of itself to the crop. */
        const val MAX_TRIM = 0.15

        /** How much looser [mightHold] is than [place], since a catalogue's own numbers are unreliable. */
        private const val SLACK = 0.05

        /** A photograph smaller than the screen by more than this is refused rather than upscaled soft. */
        const val MAX_ENLARGEMENT = 1.25

        private fun trimFor(a: Double, b: Double): Double = 1 - min(a, b) / max(a, b)

        operator fun invoke(width: Int, height: Int): Screen = Screen(min(width, height), max(width, height))
    }
}

/** An axis-aligned box in scaled-image pixels. Its own tiny type, not [android.graphics.Rect], so [Screen] stays JVM-testable. */
data class Box(val left: Int, val top: Int, val right: Int, val bottom: Int)

/**
 * A painting scaled to [scaledWidth] x [scaledHeight] to cover the screen, plus the
 * centred, screen-sized [crop] of it that removes the trimmed edges. [Wallpaper.pin]
 * hands both straight to `ImageDecoder`.
 */
data class Placement(val scaledWidth: Int, val scaledHeight: Int, val crop: Box)
