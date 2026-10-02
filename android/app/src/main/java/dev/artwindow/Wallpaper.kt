package dev.artwindow

import android.app.WallpaperManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Rect
import android.hardware.display.DisplayManager
import android.view.Display
import java.io.File
import java.io.IOException
import kotlin.math.roundToInt

/** The display size available to a JobService, which has no Activity window to query. */
fun Context.screen(): Screen {
    val displayManager = getSystemService(Context.DISPLAY_SERVICE) as DisplayManager
    val mode = displayManager.getDisplay(Display.DEFAULT_DISPLAY).mode
    return Screen(mode.physicalWidth, mode.physicalHeight)
}

/** Renders both the real wallpaper and the smaller Settings preview. */
object WallpaperRenderer {
    fun render(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean = true,
    ): Bitmap = hung(file, screen, preferences) { target, turned ->
        val framing = preferences.framingFor(file, screen).let { if (turned) it.forQuarterTurn() else it }
        renderUpright(file, target, preferences, enforceEnlargementLimit, framing)
    }

    /**
     * The part of the wallpaper that does not depend on how the sharp painting is framed:
     * Blur's blurred backdrop or the border colour, which the Settings preview draws once
     * and puts the painting over. `null` for a style with nothing under the painting (Zoom)
     * and for one whose picture is not framed at all (Stretch, Blur's whole-image variant),
     * which the preview shows as [render] does.
     */
    fun renderBase(file: File, screen: Screen, preferences: WallpaperPreferences): Bitmap? {
        if (preferences.style == WallpaperStyle.ZOOM || !preferences.framesSharpPicture()) return null
        return hung(file, screen, preferences) { target, _ ->
            when (preferences.style) {
                WallpaperStyle.BORDERS -> Bitmap.createBitmap(target.width, target.height, Bitmap.Config.ARGB_8888)
                    .also { Canvas(it).drawColor(borderColor(file, preferences)) }
                else -> blurredBackdrop(file, target, preferences, enforceEnlargementLimit = false)
            }
        }
    }

    /**
     * The painting itself, no longer than [maxEdge], as it will hang: turned a quarter turn
     * when [WallpaperPreferences.turns] says so. Decoded once per painting for the preview,
     * which then moves and scales it without touching the pixels again.
     */
    fun hungPainting(file: File, screen: Screen, preferences: WallpaperPreferences, maxEdge: Int): Bitmap {
        val sample = decodeSample(file, maxEdge)
        if (!preferences.turns(sample.width, sample.height, screen)) return sample
        try {
            return Bitmap.createBitmap(sample, 0, 0, sample.width, sample.height, QUARTER_TURN, true)
        } finally {
            sample.recycle()
        }
    }

    /**
     * Runs [block] on the screen the painting is laid out on and returns the finished
     * bitmap at [screen]'s size. A turned painting is hung by doing everything the ordinary
     * way on a screen lying on its side, then turning the finished picture back: every
     * style then works on the turned painting without any of them knowing. The quarter turn
     * is clockwise, so the painting's top ends at the screen's right edge. Gated on a
     * portrait screen by [turns], so a stale preference never turns a picture on a TV.
     */
    private fun hung(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        block: (Screen, Boolean) -> Bitmap,
    ): Bitmap {
        val (width, height) = sizeOf(file)
        if (!preferences.turns(width, height, screen)) return block(screen, false)
        val flat = block(Screen(screen.height, screen.width), true)
        try {
            return Bitmap.createBitmap(flat, 0, 0, flat.width, flat.height, QUARTER_TURN, true)
        } finally {
            flat.recycle()
        }
    }

    private fun sizeOf(file: File): Pair<Int, Int> {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        return bounds.outWidth to bounds.outHeight
    }

    private fun renderUpright(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean,
        framing: Framing,
    ): Bitmap = when (preferences.style) {
        WallpaperStyle.ZOOM -> Bitmap.createBitmap(screen.width, screen.height, Bitmap.Config.ARGB_8888).also {
            drawFramed(Canvas(it), file, screen, Base.COVER, framing, enforceEnlargementLimit)
        }
        WallpaperStyle.STRETCH -> decodeStretch(file, screen.width, screen.height, enforceEnlargementLimit)
        WallpaperStyle.BLUR -> renderBlur(file, screen, preferences, enforceEnlargementLimit, framing)
        WallpaperStyle.BORDERS -> renderBorders(file, screen, preferences, enforceEnlargementLimit, framing)
    }

    /**
     * Draws the sharp painting where [Screen.frame] puts it: only the part the screen
     * shows is decoded, at the size it is shown, so a painting zoomed to three times the
     * screen costs one screen of pixels and not nine. The enlargement limit judges the
     * painting at its own size ([Base]), whatever the zoom: which paintings are admitted
     * must not depend on it, and a zoomed one may be upscaled past the limit by choice.
     */
    private fun drawFramed(
        canvas: Canvas,
        file: File,
        screen: Screen,
        base: Base,
        framing: Framing,
        enforceEnlargementLimit: Boolean,
    ) {
        val (width, height) = sizeOf(file)
        if (enforceEnlargementLimit && screen.baseScale(width, height, base) > Screen.MAX_ENLARGEMENT) {
            tooSmall(file, width, height, screen.width, screen.height)
        }
        val rect = screen.frame(width, height, base, framing)
        val left = maxOf(0, rect.left)
        val top = maxOf(0, rect.top)
        val right = minOf(screen.width, rect.left + rect.width)
        val bottom = minOf(screen.height, rect.top + rect.height)
        if (right <= left || bottom <= top) return
        val visible = Rect(left - rect.left, top - rect.top, right - rect.left, bottom - rect.top)
        val painting = decode(file) { decoder, _, _ ->
            decoder.setTargetSize(rect.width, rect.height)
            decoder.crop = visible
        }
        try {
            canvas.drawBitmap(painting, left.toFloat(), top.toFloat(), BITMAP_PAINT)
        } finally {
            painting.recycle()
        }
    }

    fun commonColors(file: File, limit: Int = 5): List<Int> {
        val sample = decodeSample(file, COLOR_SAMPLE_SIZE)
        return try {
            val pixels = IntArray(sample.width * sample.height)
            sample.getPixels(pixels, 0, sample.width, 0, 0, sample.width, sample.height)
            commonImageColors(pixels, limit)
        } finally {
            sample.recycle()
        }
    }

    private fun renderBlur(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean,
        framing: Framing,
    ): Bitmap {
        val result = blurredBackdrop(file, screen, preferences, enforceEnlargementLimit)
        if (preferences.blurVariant == BlurVariant.BACKDROP) {
            drawFramed(Canvas(result), file, screen, Base.FIT, framing, enforceEnlargementLimit)
        }
        return result
    }

    /** The whole screen filled with the blurred painting: Blur's backdrop, or all of Blur's whole-image variant. */
    private fun blurredBackdrop(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean,
    ): Bitmap {
        val smallWidth = minOf(screen.width, BLUR_SHORT_EDGE)
        val smallHeight = (smallWidth.toDouble() * screen.height / screen.width).roundToInt().coerceAtLeast(1)
        val smallCover = decodeCover(
            file,
            smallWidth,
            smallHeight,
            enforceEnlargementLimit && preferences.blurVariant == BlurVariant.WHOLE_IMAGE,
            requiredWidth = screen.width,
            requiredHeight = screen.height,
        )
        val pixels = IntArray(smallCover.width * smallCover.height)
        smallCover.getPixels(pixels, 0, smallCover.width, 0, 0, smallCover.width, smallCover.height)
        smallCover.recycle()

        val radius = (MAX_BLUR_RADIUS * preferences.blurStrength.coerceIn(0, 100) / 100f).roundToInt()
        val blurredPixels = blurPixels(pixels, smallWidth, smallHeight, radius)
        val blurred = Bitmap.createBitmap(blurredPixels, smallWidth, smallHeight, Bitmap.Config.ARGB_8888)
        val result = Bitmap.createBitmap(screen.width, screen.height, Bitmap.Config.ARGB_8888)
        Canvas(result).drawBitmap(blurred, null, Rect(0, 0, screen.width, screen.height), BITMAP_PAINT)
        blurred.recycle()
        return result
    }

    private fun renderBorders(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean,
        framing: Framing,
    ): Bitmap {
        val result = Bitmap.createBitmap(screen.width, screen.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(result)
        canvas.drawColor(borderColor(file, preferences))
        drawFramed(canvas, file, screen, Base.FIT, framing, enforceEnlargementLimit)
        return result
    }

    private fun borderColor(file: File, preferences: WallpaperPreferences): Int = when (preferences.borderColorMode) {
        BorderColorMode.BLACK -> Color.BLACK
        BorderColorMode.CUSTOM -> preferences.customBorderColor or (0xff shl 24)
        BorderColorMode.AUTOMATIC -> automaticBorderColor(file)
    }

    private fun automaticBorderColor(file: File): Int {
        val sample = decodeSample(file, COLOR_SAMPLE_SIZE)
        return try {
            val pixels = IntArray(sample.width * sample.height)
            sample.getPixels(pixels, 0, sample.width, 0, 0, sample.width, sample.height)
            edgeAverageColor(pixels, sample.width, sample.height)
        } finally {
            sample.recycle()
        }
    }

    private fun decodeCover(
        file: File,
        targetWidth: Int,
        targetHeight: Int,
        enforceLimit: Boolean,
        requiredWidth: Int = targetWidth,
        requiredHeight: Int = targetHeight,
    ): Bitmap =
        decode(file) { decoder, width, height ->
            val scale = maxOf(targetWidth.toDouble() / width, targetHeight.toDouble() / height)
            val scaledWidth = (width * scale).roundToInt().coerceAtLeast(targetWidth)
            val scaledHeight = (height * scale).roundToInt().coerceAtLeast(targetHeight)
            if (enforceLimit) {
                val requiredScale = maxOf(requiredWidth.toDouble() / width, requiredHeight.toDouble() / height)
                if (requiredScale > Screen.MAX_ENLARGEMENT) {
                    tooSmall(file, width, height, requiredWidth, requiredHeight)
                }
            }
            decoder.setTargetSize(scaledWidth, scaledHeight)
            val left = (scaledWidth - targetWidth) / 2
            val top = (scaledHeight - targetHeight) / 2
            decoder.crop = Rect(left, top, left + targetWidth, top + targetHeight)
        }

    private fun decodeStretch(file: File, targetWidth: Int, targetHeight: Int, enforceLimit: Boolean): Bitmap =
        decode(file) { decoder, width, height ->
            if (
                enforceLimit &&
                maxOf(targetWidth.toDouble() / width, targetHeight.toDouble() / height) > Screen.MAX_ENLARGEMENT
            ) {
                tooSmall(file, width, height, targetWidth, targetHeight)
            }
            decoder.setTargetSize(targetWidth, targetHeight)
        }

    private fun decodeSample(file: File, maximumEdge: Int): Bitmap = decode(file) { decoder, width, height ->
        val scale = minOf(1.0, maximumEdge.toDouble() / maxOf(width, height))
        decoder.setTargetSize(
            (width * scale).roundToInt().coerceAtLeast(1),
            (height * scale).roundToInt().coerceAtLeast(1),
        )
    }

    private inline fun decode(
        file: File,
        crossinline configure: (ImageDecoder, Int, Int) -> Unit,
    ): Bitmap {
        if (!file.isFile) throw IOException("${file.name} is no longer available")
        return ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, info, _ ->
            configure(decoder, info.size.width, info.size.height)
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.isMutableRequired = false
        }
    }

    private fun tooSmall(file: File, width: Int, height: Int, targetWidth: Int, targetHeight: Int): Nothing =
        throw IOException("${file.name} (${width}x$height) is too small for ${targetWidth}x$targetHeight")

    private val QUARTER_TURN = Matrix().apply { postRotate(90f) }
    private val BITMAP_PAINT = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private const val BLUR_SHORT_EDGE = 256
    private const val MAX_BLUR_RADIUS = 24
    private const val COLOR_SAMPLE_SIZE = 160
}

object Wallpaper {
    /**
     * Puts [artwork] on the device's wallpaper surface: the system wallpaper on a
     * phone, the in-app [Frame] on a TV, which has no wallpaper to set. [artwork] is
     * here for the TV's caption; the phone needs only its file.
     */
    fun pin(context: Context, artwork: Artwork, screen: Screen, preferences: WallpaperPreferences) {
        val bitmap = WallpaperRenderer.render(artwork.path, screen, preferences)
        if (context.isTelevision()) {
            Frame.publish(Rendered(bitmap, artwork))
            return
        }
        try {
            WallpaperManager.getInstance(context).setBitmap(
                bitmap,
                null,
                true,
                WallpaperManager.FLAG_SYSTEM or WallpaperManager.FLAG_LOCK,
            )
        } finally {
            bitmap.recycle()
        }
    }
}
