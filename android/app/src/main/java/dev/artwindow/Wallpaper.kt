package dev.artwindow

import android.app.WallpaperManager
import android.content.Context
import android.graphics.ImageDecoder
import android.graphics.Rect
import android.hardware.display.DisplayManager
import android.view.Display
import java.io.File
import java.io.IOException

/**
 * The display's physical pixel size, read the one way that works with no Activity —
 * [Wallpaper.pin] is called from [RotationJob], which has none.
 */
fun Context.screen(): Screen {
    val displayManager = getSystemService(Context.DISPLAY_SERVICE) as DisplayManager
    val mode = displayManager.getDisplay(Display.DEFAULT_DISPLAY).mode
    return Screen(mode.physicalWidth, mode.physicalHeight)
}

/**
 * Owns hanging a picture the way `desktop::pin` does on the Rust side: placement is
 * baked into the pixels here, once, rather than left for the system to interpret.
 *
 * A bitmap already at the screen's exact size and proportions leaves Android nothing
 * to zoom, crop, or parallax-scroll — every caller downstream of [pin] just has a
 * picture that already fits.
 */
object Wallpaper {

    /**
     * Decodes [file] straight to a bitmap of exactly [screen]'s size — scaled and
     * cropped per [Screen.place] — and sets it as both the home and lock wallpaper.
     *
     * @throws IOException if [file]'s proportions are too far from [screen]'s for
     *   [Screen.place] to accept.
     */
    fun pin(context: Context, file: File, screen: Screen) {
        val source = ImageDecoder.createSource(file)
        val bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            // Placed from the size this decoder reports rather than from a separate header
            // read: the decoder applies EXIF rotation, and the crop has to be in its terms
            // or it can land outside the picture.
            val (width, height) = info.size.width to info.size.height
            val placement = screen.place(width, height)
                ?: throw IOException("${file.name} (${width}x$height) does not fit ${screen.width}x${screen.height}")
            decoder.setTargetSize(placement.scaledWidth, placement.scaledHeight)
            decoder.crop = Rect(placement.crop.left, placement.crop.top, placement.crop.right, placement.crop.bottom)
            // The wallpaper manager needs to read pixels back out of the bitmap; a hardware
            // bitmap would not allow that.
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }

        val manager = WallpaperManager.getInstance(context)
        manager.setBitmap(bitmap, null, true, WallpaperManager.FLAG_SYSTEM or WallpaperManager.FLAG_LOCK)
        bitmap.recycle()
    }
}
