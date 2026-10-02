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

    /** The scale a painting of this size has before any zoom: [Base.COVER] fills the screen, [Base.FIT] fits inside it. */
    fun baseScale(width: Int, height: Int, base: Base): Double {
        val across = this.width.toDouble() / width
        val down = this.height.toDouble() / height
        return if (base == Base.COVER) max(across, down) else min(across, down)
    }

    /**
     * Where a painting of [width] x [height] (as hung) sits on this screen at [framing]:
     * the one geometry the renderer uses at screen size and the Settings preview at its own,
     * so they cannot disagree. Zoom 1 centred is [cover] for [Base.COVER] and [fit] for
     * [Base.FIT]. Along an axis the painting overflows, the pan chooses which part shows
     * (0 puts the painting's left or top edge at the screen's, 1 its right or bottom) and
     * the painting always reaches both screen edges — no blank strip; along one it does
     * not, it stays centred and the pan does nothing.
     */
    fun frame(width: Int, height: Int, base: Base, framing: Framing): FrameRect {
        val scale = baseScale(width, height, base) * framing.zoom
        // The epsilon keeps an exact fit from becoming one pixel too wide.
        val scaledWidth = ceil(width * scale - 1e-6).toInt().coerceAtLeast(1)
        val scaledHeight = ceil(height * scale - 1e-6).toInt().coerceAtLeast(1)
        return FrameRect(
            left = offset(this.width, scaledWidth, framing.panX),
            top = offset(this.height, scaledHeight, framing.panY),
            width = scaledWidth,
            height = scaledHeight,
        )
    }

    /**
     * [framing] after a gesture: the painting scaled by [scaleChange] about the point
     * ([x], [y]) on the screen — so the part under the fingers stays under them — and then
     * moved by ([dx], [dy]) pixels, exactly as far as the fingers moved. Clamped at the
     * painting's edges and at the zoom limits.
     */
    fun reframe(
        width: Int, height: Int, base: Base, framing: Framing,
        scaleChange: Float, x: Float, y: Float, dx: Float, dy: Float,
    ): Framing {
        // A gesture that reports nonsense changes nothing, rather than leaving a NaN
        // where every later calculation would inherit it.
        if (listOf(scaleChange, x, y, dx, dy).any { !it.isFinite() }) return framing
        val zoom = (framing.zoom * scaleChange).coerceIn(1f, MAX_ZOOM)
        val ratio = zoom / framing.zoom
        val before = frame(width, height, base, framing)
        val after = frame(width, height, base, framing.copy(zoom = zoom))
        // Where the pan puts the painting exactly, not the whole pixel `frame` draws it
        // at. A finger sends a fraction of a pixel per event, and starting each event
        // from the rounded position threw that fraction away in one direction and
        // rounded it up in the other: a slow drag left or up never moved at all.
        val left = x - (x - exactOffset(this.width, before.width, framing.panX)) * ratio + dx
        val top = y - (y - exactOffset(this.height, before.height, framing.panY)) * ratio + dy
        return Framing(
            zoom = zoom,
            panX = panFor(this.width, after.width, left),
            panY = panFor(this.height, after.height, top),
        )
    }

    /** Whether a drag along each axis (across, down) could move a painting framed so. */
    fun canMove(width: Int, height: Int, base: Base, framing: Framing): Pair<Boolean, Boolean> {
        val rect = frame(width, height, base, framing)
        return (rect.width > this.width) to (rect.height > this.height)
    }

    private fun offset(screen: Int, painting: Int, pan: Float): Int =
        if (painting > screen) -((painting - screen) * pan.coerceIn(0f, 1f).toDouble()).toInt() else (screen - painting) / 2

    /** [offset] before it is rounded to a pixel. */
    private fun exactOffset(screen: Int, painting: Int, pan: Float): Float =
        if (painting > screen) -(painting - screen) * pan.coerceIn(0f, 1f) else (screen - painting) / 2f

    private fun panFor(screen: Int, painting: Int, left: Float): Float =
        if (painting > screen) (-left / (painting - screen)).coerceIn(0f, 1f) else CENTRED

    fun canStretch(width: Int, height: Int): Boolean =
        width > 0 && height > 0 &&
            max(this.width.toDouble() / width, this.height.toDouble() / height) <= MAX_ENLARGEMENT

    companion object {
        /** The pan that centres a painting. */
        const val CENTRED = 0.5f

        /** The furthest a painting may be zoomed past its own size. */
        const val MAX_ZOOM = 3f

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

/** The size a painting has before any zoom: filling the screen, or fitted inside it. */
enum class Base { COVER, FIT }

/**
 * How a painting is framed: [zoom] from 1 (its own size) to [Screen.MAX_ZOOM], and where
 * the screen's window sits on it along each axis it overflows, [panX] and [panY] from 0
 * (the painting's left or top edge at the screen's) to 1 (its right or bottom).
 */
data class Framing(
    val zoom: Float = 1f,
    val panX: Float = Screen.CENTRED,
    val panY: Float = Screen.CENTRED,
) {
    /**
     * This framing in the terms of a painting turned a quarter turn clockwise: the
     * screen's left becomes the turned painting's bottom, and its top its left.
     */
    fun forQuarterTurn(): Framing = Framing(zoom, panX = panY, panY = 1f - panX)
}

/** Where a painting's rectangle sits relative to the screen: its corner may be off-screen, hence negative. */
data class FrameRect(val left: Int, val top: Int, val width: Int, val height: Int)
