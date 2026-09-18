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

enum class ArtworkRegion(val apiName: String) {
    EUROPE("Europe"),
    ASIA("Asia"),
    AFRICA("Africa"),
    NORTH_AMERICA("North America"),
    SOUTH_AMERICA("South America"),
    OCEANIA("Oceania");

    companion object {
        val DEFAULT = setOf(EUROPE, ASIA)
    }
}

/**
 * What a painting is of. [queries] are the Met search terms that stand in for it —
 * several for a subject the collection holds thinly, so [Met] has more than one
 * query to draw candidates from before it has to fall back to another subject.
 */
enum class ArtworkSubject(val queries: List<String>) {
    LANDSCAPE(listOf("landscape")),
    SEASCAPE(listOf("seascape", "marine", "boats")),
    STILL_LIFE(listOf("still life", "flowers")),
    CITY(listOf("cityscape", "city", "street"));

    companion object {
        val DEFAULT = setOf(LANDSCAPE)
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
    val artworkRegions: Set<ArtworkRegion> = ArtworkRegion.DEFAULT,
    val artworkSubjects: Set<ArtworkSubject> = ArtworkSubject.DEFAULT,
    val hideReligious: Boolean = false,
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
        artworkRegions = regionValues(prefs.getString(KEY_REGIONS, null)),
        artworkSubjects = subjectValues(prefs.getString(KEY_SUBJECTS, null)),
        hideReligious = prefs.getBoolean(KEY_HIDE_RELIGIOUS, false),
    )

    fun save(value: WallpaperPreferences): Boolean = prefs.edit()
        .putString(KEY_STYLE, value.style.name)
        .putString(KEY_SHAPE, value.artworkShape.name)
        .putString(KEY_BLUR_VARIANT, value.blurVariant.name)
        .putInt(KEY_BLUR_STRENGTH, value.blurStrength.coerceIn(0, 100))
        .putString(KEY_BORDER_MODE, value.borderColorMode.name)
        .putInt(KEY_CUSTOM_COLOR, value.customBorderColor or (0xff shl 24))
        .putString(KEY_REGIONS, value.artworkRegions.sortedBy { it.ordinal }.joinToString(",") { it.name })
        .putString(KEY_SUBJECTS, value.artworkSubjects.sortedBy { it.ordinal }.joinToString(",") { it.name })
        .putBoolean(KEY_HIDE_RELIGIOUS, value.hideReligious)
        .commit()

    private companion object {
        const val PREFS_NAME = "art_window"
        const val KEY_STYLE = "wallpaper_style"
        const val KEY_SHAPE = "artwork_shape"
        const val KEY_BLUR_VARIANT = "blur_variant"
        const val KEY_BLUR_STRENGTH = "blur_strength"
        const val KEY_BORDER_MODE = "border_color_mode"
        const val KEY_CUSTOM_COLOR = "custom_border_color"
        const val KEY_REGIONS = "artwork_regions"
        const val KEY_SUBJECTS = "artwork_subjects"
        const val KEY_HIDE_RELIGIOUS = "hide_religious"
    }
}

internal inline fun <reified T : Enum<T>> enumValue(raw: String?, fallback: T): T =
    enumValues<T>().firstOrNull { it.name == raw } ?: fallback

internal fun regionValues(raw: String?): Set<ArtworkRegion> {
    val regions = raw
        ?.split(',')
        ?.mapNotNull { stored -> ArtworkRegion.entries.firstOrNull { it.name == stored } }
        ?.toSet()
        .orEmpty()
    return regions.ifEmpty { ArtworkRegion.DEFAULT }
}

internal fun subjectValues(raw: String?): Set<ArtworkSubject> {
    val subjects = raw
        ?.split(',')
        ?.mapNotNull { stored -> ArtworkSubject.entries.firstOrNull { it.name == stored } }
        ?.toSet()
        .orEmpty()
    return subjects.ifEmpty { ArtworkSubject.DEFAULT }
}

/** Shared by regions and subjects: toggles [item], but never empties the set. */
internal fun <T> toggled(current: Set<T>, item: T): Set<T> =
    if (item in current) {
        if (current.size == 1) current else current - item
    } else {
        current + item
    }
