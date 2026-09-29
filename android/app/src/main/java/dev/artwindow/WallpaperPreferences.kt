package dev.artwindow

import android.content.Context

enum class WallpaperStyle { ZOOM, STRETCH, BLUR, BORDERS }

enum class BlurVariant { BACKDROP, WHOLE_IMAGE }

enum class ArtworkShape {
    /** Paintings shaped like the screen itself: tall on a phone, wide on a TV. */
    SCREEN,

    /** [SCREEN] plus paintings near square, on the side away from the screen's own shape. */
    NEAR_SQUARE,
    ANY;

    fun accepts(aspect: Double, screen: Screen): Boolean {
        if (!aspect.isFinite() || aspect <= 0.0) return false
        return when (this) {
            SCREEN -> screen.holds(aspect)
            NEAR_SQUARE ->
                if (screen.isLandscape) {
                    aspect >= NEAR_SQUARE_MIN && aspect <= screen.aspectRatio * (1.0 + Screen.MAX_TRIM)
                } else {
                    aspect >= screen.aspectRatio * (1.0 - Screen.MAX_TRIM) && aspect <= NEAR_SQUARE_MAX
                }
            ANY -> true
        }
    }

    private companion object {
        /** Five by four: the widest a painting may be and still count as near square on a portrait screen. */
        const val NEAR_SQUARE_MAX = 1.25

        /** Four by five: the same limit turned on its side, for a landscape screen. */
        const val NEAR_SQUARE_MIN = 0.8
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
 * What a painting is of. [queries] are the catalogue search terms that stand in for
 * it — matched against a [Catalogue.Entry]'s title and tags — several for a subject
 * the collection holds thinly, so a subject a person actually chose still has enough
 * terms behind it to turn up paintings.
 *
 * SMK's records come back in Danish (see `docs/android.md`), so each list also
 * carries the Danish words for the same subject rather than relying on an English
 * query to happen to appear — a plain SMK title never does. Each Danish irregular
 * plural is spelled out as its own entry ("landskab", "landskaber") instead of
 * leaning on [matchesQuery]'s English "+s" rule, which does not form it. Danish "by"
 * (town/city) is deliberately absent: it collides with the English preposition
 * "by", flooding the catalogue's English titles with false landscape matches.
 */
enum class ArtworkSubject(val queries: List<String>) {
    LANDSCAPE(
        listOf(
            "landscape", "cityscape", "city", "street",
            "landskab", "landskaber", "parti fra", "udsigt", "gade", "gader",
        ),
    ),
    SEASCAPE(
        listOf(
            "seascape", "marine", "boats",
            "havn", "skibe", "kyst", "strand", "hav", "både",
        ),
    ),
    STILL_LIFE(
        listOf(
            "still life", "flowers",
            "opstilling", "blomster", "stilleben",
        ),
    );

    companion object {
        val DEFAULT = setOf(LANDSCAPE)
    }
}

enum class BorderColorMode { BLACK, AUTOMATIC, CUSTOM }

data class WallpaperPreferences(
    val style: WallpaperStyle = WallpaperStyle.ZOOM,
    val artworkShape: ArtworkShape = ArtworkShape.SCREEN,
    val blurVariant: BlurVariant = BlurVariant.BACKDROP,
    val blurStrength: Int = DEFAULT_BLUR_STRENGTH,
    val borderColorMode: BorderColorMode = BorderColorMode.BLACK,
    val customBorderColor: Int = DEFAULT_CUSTOM_COLOR,
    val artworkRegions: Set<ArtworkRegion> = ArtworkRegion.DEFAULT,
    val artworkSubjects: Set<ArtworkSubject> = ArtworkSubject.DEFAULT,
    val artworkArtists: Set<String> = emptySet(),
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
        artworkShape = shapeValue(prefs.getString(KEY_SHAPE, null)),
        blurVariant = enumValue(prefs.getString(KEY_BLUR_VARIANT, null), BlurVariant.BACKDROP),
        blurStrength = prefs.getInt(KEY_BLUR_STRENGTH, WallpaperPreferences.DEFAULT_BLUR_STRENGTH).coerceIn(0, 100),
        borderColorMode = enumValue(prefs.getString(KEY_BORDER_MODE, null), BorderColorMode.BLACK),
        customBorderColor = prefs.getInt(KEY_CUSTOM_COLOR, WallpaperPreferences.DEFAULT_CUSTOM_COLOR) or (0xff shl 24),
        artworkRegions = regionValues(prefs.getString(KEY_REGIONS, null)),
        artworkSubjects = subjectValues(prefs.getString(KEY_SUBJECTS, null)),
        artworkArtists = artistValues(prefs.getString(KEY_ARTISTS, null)),
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
        .putString(KEY_ARTISTS, value.artworkArtists.sorted().joinToString(","))
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
        const val KEY_ARTISTS = "artwork_artists"
        const val KEY_HIDE_RELIGIOUS = "hide_religious"
    }
}

internal inline fun <reified T : Enum<T>> enumValue(raw: String?, fallback: T): T =
    enumValues<T>().firstOrNull { it.name == raw } ?: fallback

/** [ArtworkShape.SCREEN] was saved as `PHONE` before the app knew about TVs; that name still has to load. */
internal fun shapeValue(raw: String?): ArtworkShape =
    enumValue(if (raw == "PHONE") ArtworkShape.SCREEN.name else raw, ArtworkShape.SCREEN)

/**
 * [raw] `null` means these preferences have never been saved — a fresh install starts
 * from [ArtworkRegion.DEFAULT] rather than an unfiltered Any. Any other stored value,
 * including one that decodes to nothing (blank, or naming only regions this build no
 * longer recognises), is that section's honest state: Any, per the "empty selection
 * means Any" rule in `Catalogue.matching` — once a preference has actually been saved,
 * there is no other fallback.
 */
internal fun regionValues(raw: String?): Set<ArtworkRegion> {
    if (raw == null) return ArtworkRegion.DEFAULT
    return raw.split(',').mapNotNullTo(mutableSetOf()) { stored -> ArtworkRegion.entries.firstOrNull { it.name == stored } }
}

/** [regionValues]'s rule for the subject axis: `null` starts from [ArtworkSubject.DEFAULT], anything else is Any if it decodes to nothing. */
internal fun subjectValues(raw: String?): Set<ArtworkSubject> {
    if (raw == null) return ArtworkSubject.DEFAULT
    return raw.split(',').mapNotNullTo(mutableSetOf()) { stored -> ArtworkSubject.entries.firstOrNull { it.name == stored } }
}

/**
 * An artist name is a catalogue value, not a fixed enum, so there is nothing to
 * validate it against here the way [regionValues] and [subjectValues] validate
 * against their enums — a name that no longer names an artist in the catalogue is
 * simply tolerated and matches nothing (see [Catalogue.matching]), rather than being
 * dropped at load time. Blank entries are filtered so an empty stored string reads
 * as no artists chosen. Artists default to Any even on a fresh install (`raw == null`),
 * unlike [regionValues] and [subjectValues] — there is no curated artist default.
 */
internal fun artistValues(raw: String?): Set<String> =
    raw?.split(',')?.filter { it.isNotBlank() }?.toSet().orEmpty()

/**
 * Toggles [item] in [current]. Reaching the empty set is fine and expected — Origins,
 * Subjects and Artists all treat an empty selection as Any, so there is no "last item"
 * to protect the way there once was.
 */
internal fun <T> toggled(current: Set<T>, item: T): Set<T> =
    if (item in current) current - item else current + item

/**
 * Whether a photograph of [width] x [height] can be rendered under this preference's
 * [WallpaperPreferences.style] without exceeding [Screen.MAX_ENLARGEMENT] — the same
 * geometry [WallpaperRenderer.render] applies, asked ahead of time so a candidate that
 * would fail to render is never offered. Shared by [Catalogue] (screening the
 * catalogue's own pixel counts) and [Museums] (verifying what actually downloaded).
 */
fun WallpaperPreferences.canRender(width: Int, height: Int, screen: Screen): Boolean = when (style) {
    WallpaperStyle.ZOOM -> screen.cover(width, height) != null
    WallpaperStyle.STRETCH -> screen.canStretch(width, height)
    WallpaperStyle.BLUR -> if (blurVariant == BlurVariant.BACKDROP) {
        screen.fit(width, height) != null
    } else {
        screen.cover(width, height) != null
    }
    WallpaperStyle.BORDERS -> screen.fit(width, height) != null
}
