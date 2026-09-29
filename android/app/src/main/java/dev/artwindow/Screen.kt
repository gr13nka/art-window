package dev.artwindow

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * Screen-relative placement and the default screen-shaped artwork tolerance.
 *
 * [holds] is the crop-friendly policy used by the Screen-shaped preference. [cover],
 * [fit] and [canStretch] are the lower-level geometry used by every rendering style.
 *
 * Keeps the orientation it is given: a phone's display mode reports portrait and a
 * TV's landscape, and the two want different paintings, so the shape rules ask
 * [isLandscape] rather than assuming one. Every method here compares ratios
 * symmetrically, so none of them needs to know which way round it is.
 */
data class Screen(val width: Int, val height: Int) {

    val aspectRatio: Double = width.toDouble() / height

    val isLandscape: Boolean get() = width > height

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

        /** A photograph smaller than the screen by more than this is refused rather than upscaled soft. */
        const val MAX_ENLARGEMENT = 1.25

        private fun trimFor(a: Double, b: Double): Double = 1 - min(a, b) / max(a, b)
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
