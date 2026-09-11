package dev.artwindow

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * Screen-relative placement and the default phone-shaped artwork tolerance.
 *
 * [mightHold] and [holds] retain the original crop-friendly policy used by the
 * Phone-shaped preference. [cover], [fit] and [canStretch] are the lower-level
 * geometry used by every rendering style.
 *
 * Always portrait: the constructor sorts whatever a display reports into
 * [width] <= [height], so a landscape-shaped [android.view.Display.Mode] and a
 * portrait one describe the same [Screen].
 */
@ConsistentCopyVisibility
data class Screen private constructor(val width: Int, val height: Int) {

    val aspectRatio: Double = width.toDouble() / height

    /**
     * A cheap pass against a catalogue's stated size. Museum measurements describe
     * the object (a stretcher, a scroll's mount), not the photograph, so this allows
     * more slack than [place]'s pixel-exact verdict — enough that a painting only
     * [place] would end up refusing rarely costs a download, without being so loose
     * that it lets through shapes [place] would never accept.
     */
    fun mightHold(aspect: Double): Boolean = trimFor(aspect, aspectRatio) <= MAX_TRIM + SLACK

    /**
     * Whether a photograph of these proportions fills the screen within [MAX_TRIM].
     * Proportions only, so a web-sized copy can answer for the original before the
     * original is downloaded; [place] adds the question of whether there are enough
     * pixels.
     */
    fun holds(aspect: Double): Boolean = trimFor(aspect, aspectRatio) <= MAX_TRIM

    /**
     * How a photograph of size [width] x [height] would sit on this screen if scaled
     * to cover it exactly, or `null` if that costs too much: either edge trimmed past
     * [MAX_TRIM], or the image enlarged past [MAX_ENLARGEMENT] to reach screen size
     * at all.
     */
    fun place(width: Int, height: Int): Placement? {
        if (!holds(width.toDouble() / height)) return null

        return cover(width, height)
    }

    /** Centred aspect-fill placement with no shape rejection. */
    fun cover(width: Int, height: Int): Placement? {
        if (width <= 0 || height <= 0) return null

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

    /** Centred aspect-fit destination, leaving the remainder available for a backdrop or border. */
    fun fit(width: Int, height: Int): FitPlacement? {
        if (width <= 0 || height <= 0) return null
        val scale = min(this.width.toDouble() / width, this.height.toDouble() / height)
        if (scale > MAX_ENLARGEMENT) return null
        val scaledWidth = ceil(width * scale).toInt().coerceAtMost(this.width)
        val scaledHeight = ceil(height * scale).toInt().coerceAtMost(this.height)
        val left = (this.width - scaledWidth) / 2
        val top = (this.height - scaledHeight) / 2
        return FitPlacement(scaledWidth, scaledHeight, Box(left, top, left + scaledWidth, top + scaledHeight))
    }

    fun canStretch(width: Int, height: Int): Boolean =
        width > 0 && height > 0 &&
            max(this.width.toDouble() / width, this.height.toDouble() / height) <= MAX_ENLARGEMENT

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
 * Painting dimensions after aspect-fill scaling, plus the centred screen-sized
 * [crop]. Kept platform-free so selection and geometry remain JVM-testable.
 */
data class Placement(val scaledWidth: Int, val scaledHeight: Int, val crop: Box)

data class FitPlacement(val scaledWidth: Int, val scaledHeight: Int, val destination: Box)
