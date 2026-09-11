package dev.artwindow

import android.app.WallpaperManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
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
    ): Bitmap = when (preferences.style) {
        WallpaperStyle.ZOOM -> decodeCover(file, screen.width, screen.height, enforceEnlargementLimit)
        WallpaperStyle.STRETCH -> decodeStretch(file, screen.width, screen.height, enforceEnlargementLimit)
        WallpaperStyle.BLUR -> renderBlur(file, screen, preferences, enforceEnlargementLimit)
        WallpaperStyle.BORDERS -> renderBorders(file, screen, preferences, enforceEnlargementLimit)
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
        val canvas = Canvas(result)
        canvas.drawBitmap(blurred, null, Rect(0, 0, screen.width, screen.height), BITMAP_PAINT)
        blurred.recycle()

        if (preferences.blurVariant == BlurVariant.BACKDROP) {
            val foreground = decodeFit(file, screen.width, screen.height, enforceEnlargementLimit)
            try {
                val left = (screen.width - foreground.width) / 2
                val top = (screen.height - foreground.height) / 2
                canvas.drawBitmap(foreground, left.toFloat(), top.toFloat(), BITMAP_PAINT)
            } finally {
                foreground.recycle()
            }
        }
        return result
    }

    private fun renderBorders(
        file: File,
        screen: Screen,
        preferences: WallpaperPreferences,
        enforceEnlargementLimit: Boolean,
    ): Bitmap {
        val result = Bitmap.createBitmap(screen.width, screen.height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(result)
        val background = when (preferences.borderColorMode) {
            BorderColorMode.BLACK -> Color.BLACK
            BorderColorMode.CUSTOM -> preferences.customBorderColor or (0xff shl 24)
            BorderColorMode.AUTOMATIC -> automaticBorderColor(file)
        }
        canvas.drawColor(background)
        val foreground = decodeFit(file, screen.width, screen.height, enforceEnlargementLimit)
        try {
            val left = (screen.width - foreground.width) / 2
            val top = (screen.height - foreground.height) / 2
            canvas.drawBitmap(foreground, left.toFloat(), top.toFloat(), BITMAP_PAINT)
        } finally {
            foreground.recycle()
        }
        return result
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

    private fun decodeFit(file: File, targetWidth: Int, targetHeight: Int, enforceLimit: Boolean): Bitmap =
        decode(file) { decoder, width, height ->
            val scale = minOf(targetWidth.toDouble() / width, targetHeight.toDouble() / height)
            if (enforceLimit && scale > Screen.MAX_ENLARGEMENT) {
                tooSmall(file, width, height, targetWidth, targetHeight)
            }
            decoder.setTargetSize(
                (width * scale).roundToInt().coerceAtLeast(1),
                (height * scale).roundToInt().coerceAtLeast(1),
            )
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

    private val BITMAP_PAINT = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private const val BLUR_SHORT_EDGE = 256
    private const val MAX_BLUR_RADIUS = 24
    private const val COLOR_SAMPLE_SIZE = 160
}

object Wallpaper {
    fun pin(context: Context, file: File, screen: Screen, preferences: WallpaperPreferences) {
        val bitmap = WallpaperRenderer.render(file, screen, preferences)
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
