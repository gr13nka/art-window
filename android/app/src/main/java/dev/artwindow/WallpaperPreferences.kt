package dev.artwindow

import android.content.Context

enum class WallpaperStyle { ZOOM, STRETCH, BLUR, BORDERS }

enum class BlurVariant { BACKDROP, WHOLE_IMAGE }

enum class ArtworkShape {
    PHONE,
    NEAR_SQUARE,
    ANY;

    fun accepts(aspect: Double, screen: Screen): Boolean {
        if (!aspect.isFinite() || aspect <= 0.0) return false
        return when (this) {
            PHONE -> screen.holds(aspect)
            NEAR_SQUARE -> aspect >= screen.aspectRatio * (1.0 - Screen.MAX_TRIM) && aspect <= NEAR_SQUARE_MAX
            ANY -> true
        }
    }

    /** Looser catalogue-only check; the downloaded preview remains the final shape verdict. */
    fun mightAccept(aspect: Double, screen: Screen): Boolean {
        if (!aspect.isFinite() || aspect <= 0.0) return false
        return when (this) {
            PHONE -> screen.mightHold(aspect)
            NEAR_SQUARE ->
                aspect >= screen.aspectRatio * (1.0 - Screen.MAX_TRIM - CATALOGUE_SLACK) &&
                    aspect <= NEAR_SQUARE_MAX / (1.0 - CATALOGUE_SLACK)
            ANY -> true
        }
    }

    private companion object {
        const val NEAR_SQUARE_MAX = 1.25
        const val CATALOGUE_SLACK = 0.05
    }
}

enum class BorderColorMode { BLACK, AUTOMATIC, CUSTOM }

data class WallpaperPreferences(
    val style: WallpaperStyle = WallpaperStyle.ZOOM,
    val artworkShape: ArtworkShape = ArtworkShape.PHONE,
    val blurVariant: BlurVariant = BlurVariant.BACKDROP,
    val blurStrength: Int = DEFAULT_BLUR_STRENGTH,
    val borderColorMode: BorderColorMode = BorderColorMode.BLACK,
    val customBorderColor: Int = DEFAULT_CUSTOM_COLOR,
) {
    companion object {
        const val DEFAULT_BLUR_STRENGTH = 50
        const val DEFAULT_CUSTOM_COLOR = 0xff3434c8.toInt()
    }
}

class WallpaperPreferencesStore(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun load(): WallpaperPreferences = WallpaperPreferences(
        style = enumValue(prefs.getString(KEY_STYLE, null), WallpaperStyle.ZOOM),
        artworkShape = enumValue(prefs.getString(KEY_SHAPE, null), ArtworkShape.PHONE),
        blurVariant = enumValue(prefs.getString(KEY_BLUR_VARIANT, null), BlurVariant.BACKDROP),
        blurStrength = prefs.getInt(KEY_BLUR_STRENGTH, WallpaperPreferences.DEFAULT_BLUR_STRENGTH).coerceIn(0, 100),
        borderColorMode = enumValue(prefs.getString(KEY_BORDER_MODE, null), BorderColorMode.BLACK),
        customBorderColor = prefs.getInt(KEY_CUSTOM_COLOR, WallpaperPreferences.DEFAULT_CUSTOM_COLOR) or (0xff shl 24),
    )

    fun save(value: WallpaperPreferences): Boolean = prefs.edit()
        .putString(KEY_STYLE, value.style.name)
        .putString(KEY_SHAPE, value.artworkShape.name)
        .putString(KEY_BLUR_VARIANT, value.blurVariant.name)
        .putInt(KEY_BLUR_STRENGTH, value.blurStrength.coerceIn(0, 100))
        .putString(KEY_BORDER_MODE, value.borderColorMode.name)
        .putInt(KEY_CUSTOM_COLOR, value.customBorderColor or (0xff shl 24))
        .commit()

    private companion object {
        const val PREFS_NAME = "art_window"
        const val KEY_STYLE = "wallpaper_style"
        const val KEY_SHAPE = "artwork_shape"
        const val KEY_BLUR_VARIANT = "blur_variant"
        const val KEY_BLUR_STRENGTH = "blur_strength"
        const val KEY_BORDER_MODE = "border_color_mode"
        const val KEY_CUSTOM_COLOR = "custom_border_color"
    }
}

internal inline fun <reified T : Enum<T>> enumValue(raw: String?, fallback: T): T =
    enumValues<T>().firstOrNull { it.name == raw } ?: fallback
