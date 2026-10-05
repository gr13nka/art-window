package dev.artwindow

import android.content.Context
import java.io.File

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

enum class ArtworkRegion(val label: String) {
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
    /** Phones only: turn a wide painting a quarter turn before hanging it; see [hungSize]. */
    val rotateWide: Boolean = false,
    /** How far the sharp painting is zoomed, 1 to [Screen.MAX_ZOOM]: a style option, kept across paintings and styles. */
    val frameZoom: Float = 1f,
    /** Where the screen's window sits on the painting along each axis, 0 to 1; belongs to [panPainting] alone, see [framingFor]. */
    val panX: Float = Screen.CENTRED,
    val panY: Float = Screen.CENTRED,
    /** The file name of the one painting [panX] and [panY] were set for; any other painting is centred. */
    val panPainting: String? = null,
) {
    /** Whether [other] chooses the same paintings, whatever it does with them: every filter equal, style and colours ignored. */
    fun sameFiltersAs(other: WallpaperPreferences): Boolean =
        artworkShape == other.artworkShape &&
            artworkRegions == other.artworkRegions &&
            artworkSubjects == other.artworkSubjects &&
            artworkArtists == other.artworkArtists &&
            hideReligious == other.hideReligious

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
        rotateWide = prefs.getBoolean(KEY_ROTATE_WIDE, false),
        frameZoom = prefs.getFloat(KEY_FRAME_ZOOM, 1f).finiteOr(1f).coerceIn(1f, Screen.MAX_ZOOM),
        // `pan` was one number for whichever axis overflowed; it now seeds both.
        panX = prefs.getFloat(KEY_PAN_X, prefs.getFloat(KEY_PAN, Screen.CENTRED)).finiteOr(Screen.CENTRED).coerceIn(0f, 1f),
        panY = prefs.getFloat(KEY_PAN_Y, prefs.getFloat(KEY_PAN, Screen.CENTRED)).finiteOr(Screen.CENTRED).coerceIn(0f, 1f),
        panPainting = prefs.getString(KEY_PAN_PAINTING, null),
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
        .putBoolean(KEY_ROTATE_WIDE, value.rotateWide)
        .remove(KEY_PAN)
        .putFloat(KEY_FRAME_ZOOM, value.frameZoom.finiteOr(1f).coerceIn(1f, Screen.MAX_ZOOM))
        .putFloat(KEY_PAN_X, value.panX.finiteOr(Screen.CENTRED).coerceIn(0f, 1f))
        .putFloat(KEY_PAN_Y, value.panY.finiteOr(Screen.CENTRED).coerceIn(0f, 1f))
        .putString(KEY_PAN_PAINTING, value.panPainting)
        .commit()

    private companion object {
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
        const val KEY_ROTATE_WIDE = "rotate_wide"
        const val KEY_PAN = "pan"
        const val KEY_PAN_X = "pan_x"
        const val KEY_PAN_Y = "pan_y"
        const val KEY_FRAME_ZOOM = "frame_zoom"
        const val KEY_PAN_PAINTING = "pan_painting"
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
 *
 * One painter at a time. The stored value stays a comma-separated list so nothing
 * migrates, but an earlier build could store several and a mixture nobody asked for by
 * name is worse than a guess, so only the first is read — here and nowhere else.
 */
internal fun artistValues(raw: String?): Set<String> =
    raw?.split(',')?.firstOrNull { it.isNotBlank() }?.let { setOf(it) }.orEmpty()

/** [artist] as the one painter chosen, in place of whoever was. */
internal fun chosenArtist(artist: String): Set<String> = setOf(artist)

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
fun WallpaperPreferences.canRender(width: Int, height: Int, screen: Screen): Boolean {
    val (w, h) = hungSize(width, height, screen)
    return when (style) {
        WallpaperStyle.ZOOM -> screen.cover(w, h) != null
        WallpaperStyle.STRETCH -> screen.canStretch(w, h)
        WallpaperStyle.BLUR -> if (blurVariant == BlurVariant.BACKDROP) {
            screen.fit(w, h) != null
        } else {
            screen.cover(w, h) != null
        }
        WallpaperStyle.BORDERS -> screen.fit(w, h) != null
    }
}

/** Whether a painting [width] x [height] is turned a quarter turn on [screen]: asked for, wide, and the screen tall. */
fun WallpaperPreferences.turns(width: Int, height: Int, screen: Screen): Boolean =
    rotateWide && !screen.isLandscape && width > height

/**
 * The size a painting of [width] x [height] has once hung on [screen]: swapped when it
 * is turned (see [turns]). The one place that decides it, so the shape filter, the render
 * check and the renderer cannot disagree. Not for comparing a download with the
 * catalogue's declared size, which is about the file and not the hanging.
 */
fun WallpaperPreferences.hungSize(width: Int, height: Int, screen: Screen): Pair<Int, Int> =
    if (turns(width, height, screen)) height to width else width to height

/**
 * How the painting in [file] is framed on [screen]. The zoom is a style option and
 * applies to any painting; the pan belongs to the one painting it was set for and is
 * the centre for every other, so the next painting starts centred with nothing resetting
 * anything. A painting is recognised by its file name, which a favourite's copy keeps.
 * A landscape screen — a TV — ignores both: nobody can adjust them there.
 */
fun WallpaperPreferences.framingFor(file: File, screen: Screen): Framing = when {
    screen.isLandscape -> Framing()
    panPainting == file.name -> Framing(frameZoom, panX, panY)
    else -> Framing(frameZoom)
}

/**
 * Whether the style hangs a sharp painting that can be framed: Zoom, Blur with its
 * backdrop, Borders. Stretch fills the screen with the painting whatever its shape, and
 * Blur's whole-image variant has no sharp picture.
 */
fun WallpaperPreferences.framesSharpPicture(): Boolean =
    style == WallpaperStyle.ZOOM || style == WallpaperStyle.BORDERS ||
        (style == WallpaperStyle.BLUR && blurVariant == BlurVariant.BACKDROP)

/** Where [framesSharpPicture]'s picture starts out: filling the screen for Zoom, fitted inside it otherwise. */
fun WallpaperPreferences.frameBase(): Base = if (style == WallpaperStyle.ZOOM) Base.COVER else Base.FIT

internal fun WallpaperPreferences.hungAspect(width: Int, height: Int, screen: Screen): Double {
    val (w, h) = hungSize(width, height, screen)
    return w.toDouble() / h
}

/**
 * `coerceIn` lets a NaN straight through, and a NaN pan — which a build of 2026-10-02
 * could store — pins the painting to its left edge for good. Read or written, a framing
 * number that is not a number is the default.
 */
private fun Float.finiteOr(fallback: Float): Float = if (isFinite()) this else fallback
