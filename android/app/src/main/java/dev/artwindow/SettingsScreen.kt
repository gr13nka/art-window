package dev.artwindow

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import java.io.File
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
fun SettingsScreen(
    context: Context,
    artwork: Artwork?,
    screen: Screen,
    preferences: WallpaperPreferences,
    savedPreferences: WallpaperPreferences,
    onPreferencesChange: (WallpaperPreferences) -> Unit,
    onChoicesAvailable: (Boolean) -> Unit = {},
) {
    var preview by remember { mutableStateOf<Bitmap?>(null) }
    var commonColors by remember { mutableStateOf(emptyList<Int>()) }
    var previewError by remember { mutableStateOf(false) }
    val path = artwork?.path?.takeIf(File::isFile)
    val previewScreen = remember(screen) {
        Screen(PREVIEW_WIDTH, (PREVIEW_WIDTH / screen.aspectRatio).roundToInt())
    }

    // On a phone the sharp painting is framed — pinched and dragged — in a preview made of two
    // layers: what lies under it (Blur's backdrop, the border colour), rendered through the
    // renderer and redone only when a style option changes, and the painting itself, decoded
    // once and placed by Compose. A gesture then changes numbers and nothing is re-rendered.
    val framed = preferences.framesSharpPicture() && !screen.isLandscape && !context.isTelevision()
    var sharp by remember { mutableStateOf<Bitmap?>(null) }
    val currentPreferences by rememberUpdatedState(preferences)
    val configuration = LocalConfiguration.current
    // Tall enough to have something to pinch, within the width there is; a TV keeps a small one.
    val previewWidth = if (screen.isLandscape) {
        150.dp
    } else {
        minOf((configuration.screenHeightDp * PREVIEW_HEIGHT_SHARE * screen.aspectRatio.toFloat()).dp, (configuration.screenWidthDp - 40).dp)
    }

    // Which options each section can actually offer, computed off the main thread
    // whenever a filter changes, since deciding this means walking the whole catalogue.
    // Per-option availability holds the *other* sections at their currently staged
    // values (see Catalogue.availableRegions and its siblings) — an option not returned
    // here is hidden from its chip row, never removed from the staged preference itself.
    // Optimistic region/subject defaults (everything available) avoid a flash of an
    // empty chip row before the first computation lands; artists start empty since
    // [allArtists] itself is empty until the catalogue loads, which already hides the
    // Artists section.
    var catalogue by remember { mutableStateOf<Catalogue?>(null) }
    var allArtists by remember { mutableStateOf<List<String>>(emptyList()) }
    var availableRegions by remember { mutableStateOf(ArtworkRegion.entries.toSet()) }
    var availableSubjects by remember { mutableStateOf(ArtworkSubject.entries.toSet()) }
    var artistBlocks by remember { mutableStateOf<Map<String, ArtistBlock>>(emptyMap()) }
    // What the artist browser shows: who it can show, and how many paintings each has.
    var painters by remember { mutableStateOf<List<Artists.Painter>>(emptyList()) }
    var paintingCounts by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var browsing by rememberSaveable { mutableStateOf(false) }
    // Null while the staged filters leave a pool worth rotating through; otherwise how
    // many paintings they do leave.
    var tooFew by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(Unit) {
        val loaded = withContext(Dispatchers.Default) { Catalogue.load(context) }
        catalogue = loaded
        allArtists = withContext(Dispatchers.Default) { loaded.artists() }
        withContext(Dispatchers.Default) {
            // A missing or unreadable index leaves the browser empty, never Settings broken.
            painters = runCatching { Artists.load(context).among(allArtists) }.getOrDefault(emptyList())
            paintingCounts = allArtists.associateWith { loaded.paintingsBy(it) }
        }
    }
    LaunchedEffect(
        catalogue,
        preferences.artworkRegions,
        preferences.artworkSubjects,
        preferences.artworkArtists,
        preferences.artworkShape,
        preferences.hideReligious,
        preferences.rotateWide,
        preferences.style,
        preferences.blurVariant,
        savedPreferences,
        screen,
    ) {
        val loaded = catalogue ?: return@LaunchedEffect
        availableRegions = withContext(Dispatchers.Default) { loaded.availableRegions(preferences, screen) }
        availableSubjects = withContext(Dispatchers.Default) { loaded.availableSubjects(preferences, screen) }
        artistBlocks = withContext(Dispatchers.Default) { loaded.artistBlocks(preferences, screen) }
        tooFew = withContext(Dispatchers.Default) {
            if (loaded.hasEnough(preferences, screen)) null else loaded.matchCount(preferences, screen)
        }
        onChoicesAvailable(withContext(Dispatchers.Default) { loaded.canApply(preferences, savedPreferences, screen) })
    }

    // Framing is deliberately not a key: it moves with the fingers and must not re-render.
    val renderedFor = preferences.copy(frameZoom = 1f, panX = Screen.CENTRED, panY = Screen.CENTRED, panPainting = null)
    LaunchedEffect(path, renderedFor, framed) {
        if (path == null) {
            preview?.recycle()
            preview = null
            previewError = false
            return@LaunchedEffect
        }
        val next = withContext(Dispatchers.Default) {
            runCatching {
                if (framed) {
                    WallpaperRenderer.renderBase(path, previewScreen, preferences)
                } else {
                    WallpaperRenderer.render(path, previewScreen, preferences, enforceEnlargementLimit = false)
                }
            }.getOrNull()
        }
        preview?.recycle()
        preview = next
        previewError = next == null && !framed
    }
    LaunchedEffect(path, preferences.rotateWide, framed) {
        val next = if (path == null || !framed) {
            null
        } else {
            withContext(Dispatchers.Default) {
                runCatching { WallpaperRenderer.hungPainting(path, screen, preferences, SHARP_EDGE) }.getOrNull()
            }
        }
        sharp?.recycle()
        sharp = next
    }
    LaunchedEffect(path) {
        commonColors = if (path == null) {
            emptyList()
        } else {
            withContext(Dispatchers.Default) {
                runCatching { WallpaperRenderer.commonColors(path) }.getOrDefault(emptyList())
            }
        }
    }
    DisposableEffect(Unit) {
        onDispose {
            preview?.recycle()
            sharp?.recycle()
        }
    }

    // Fold state per section, kept across rotation and process death. All four start
    // unfolded except Artists, which starts folded when it's at Any — there's nothing
    // to review in a section nobody has narrowed.
    var shapeExpanded by rememberSaveable { mutableStateOf(true) }
    var originsExpanded by rememberSaveable { mutableStateOf(true) }
    var subjectsExpanded by rememberSaveable { mutableStateOf(true) }
    var artistsExpanded by rememberSaveable { mutableStateOf(preferences.artworkArtists.isNotEmpty()) }

    val originsSummary = preferences.artworkRegions.takeIf { it.isNotEmpty() }
        ?.sortedBy { it.ordinal }?.joinToString(", ") { regionLabel(it) }
        ?: "Any region"
    val subjectsSummary = preferences.artworkSubjects.takeIf { it.isNotEmpty() }
        ?.sortedBy { it.ordinal }?.joinToString(", ") { subjectLabel(it) }
        ?: "Any subject"
    // A chosen painter wins over Shape and Origins (see Catalogue.matching); they stay on
    // show, dimmed and inert, rather than vanishing from under the user.
    val byArtist = preferences.artworkArtists.isNotEmpty()
    val artistsSummary = preferences.artworkArtists.takeIf { it.isNotEmpty() }
        ?.first()
        ?: "Any artist"

    // Choosing a painter happens in the browser, which takes over the whole screen and
    // hands back to Settings with the staged choice intact.
    if (browsing) {
        ArtistBrowserScreen(
            painters = painters,
            paintings = paintingCounts,
            chosen = preferences.artworkArtists,
            blocks = artistBlocks,
            onChoose = { artist -> onPreferencesChange(preferences.copy(artworkArtists = chosenArtist(artist))) },
            onClose = { browsing = false },
        )
        return
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Surface(
            modifier = Modifier
                .align(Alignment.CenterHorizontally)
                .width(previewWidth)
                .aspectRatio(screen.aspectRatio.toFloat())
                .then(
                    sharp?.let { picture ->
                        Modifier.frameGestures(
                            key = path,
                            paintingWidth = picture.width,
                            paintingHeight = picture.height,
                            base = preferences.frameBase(),
                            framing = { path?.let { currentPreferences.framingFor(it, screen) } ?: Framing() },
                            onFraming = { framing ->
                                val now = currentPreferences
                                if (path != null) {
                                    onPreferencesChange(
                                        now.copy(
                                            frameZoom = framing.zoom,
                                            panX = framing.panX,
                                            panY = framing.panY,
                                            panPainting = path.name,
                                        ),
                                    )
                                }
                            },
                        )
                    } ?: Modifier,
                ),
            shape = RoundedCornerShape(14.dp),
            color = MaterialTheme.colorScheme.surface,
            shadowElevation = 3.dp,
        ) {
            when {
                framed && sharp != null -> Box(Modifier.fillMaxSize()) {
                    preview?.let {
                        Image(
                            bitmap = it.asImageBitmap(),
                            contentDescription = null,
                            contentScale = ContentScale.FillBounds,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    FramedPicture(
                        picture = sharp!!,
                        base = preferences.frameBase(),
                        framing = path?.let { preferences.framingFor(it, screen) } ?: Framing(),
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                preview != null && !framed -> Image(
                    bitmap = preview!!.asImageBitmap(),
                    contentDescription = "Wallpaper preview",
                    contentScale = ContentScale.FillBounds,
                    modifier = Modifier.fillMaxSize(),
                )
                previewError -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text("Preview unavailable", style = MaterialTheme.typography.bodySmall)
                }
                else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text("Your painting", style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        if (framed && sharp != null) {
            Text(
                "Pinch and drag to frame it",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.muted,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
            )
        }

        SectionTitle("Wallpaper style")
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            WallpaperStyle.entries.forEach { style ->
                StyleCard(
                    style = style,
                    selected = style == preferences.style,
                    onClick = { onPreferencesChange(preferences.copy(style = style)) },
                    modifier = Modifier.width(78.dp),
                )
            }
        }

        if (preferences.style == WallpaperStyle.BLUR) {
            ConditionalPanel {
                Text("Blur layout", style = MaterialTheme.typography.titleSmall)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    SmallChoice(
                        label = "Sharp artwork",
                        selected = preferences.blurVariant == BlurVariant.BACKDROP,
                        onClick = {
                            onPreferencesChange(preferences.copy(blurVariant = BlurVariant.BACKDROP))
                        },
                        modifier = Modifier.weight(1f),
                    )
                    SmallChoice(
                        label = "Blur everything",
                        selected = preferences.blurVariant == BlurVariant.WHOLE_IMAGE,
                        onClick = {
                            onPreferencesChange(preferences.copy(blurVariant = BlurVariant.WHOLE_IMAGE))
                        },
                        modifier = Modifier.weight(1f),
                    )
                }
                Text(
                    "Blur strength  ${preferences.blurStrength}%",
                    style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.padding(top = 14.dp),
                )
                Slider(
                    value = preferences.blurStrength.toFloat(),
                    onValueChange = {
                        onPreferencesChange(preferences.copy(blurStrength = it.roundToInt()))
                    },
                    valueRange = 0f..100f,
                )
            }
        }

        if (preferences.style == WallpaperStyle.BORDERS) {
            ConditionalPanel {
                Text("Border color", style = MaterialTheme.typography.titleSmall)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    BorderColorMode.entries.forEach { mode ->
                        SmallChoice(
                            label = when (mode) {
                                BorderColorMode.BLACK -> "Black"
                                BorderColorMode.AUTOMATIC -> "Automatic"
                                BorderColorMode.CUSTOM -> "Custom"
                            },
                            selected = preferences.borderColorMode == mode,
                            onClick = {
                                onPreferencesChange(preferences.copy(borderColorMode = mode))
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
                if (preferences.borderColorMode == BorderColorMode.AUTOMATIC) {
                    Text(
                        "Matches the colors around the edge of each painting.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.muted,
                        modifier = Modifier.padding(top = 10.dp),
                    )
                }
                if (preferences.borderColorMode == BorderColorMode.CUSTOM) {
                    CustomColorControls(
                        color = preferences.customBorderColor,
                        currentPictureColors = commonColors,
                        onColor = {
                            onPreferencesChange(
                                preferences.copy(
                                    borderColorMode = BorderColorMode.CUSTOM,
                                    customBorderColor = it,
                                ),
                            )
                        },
                    )
                }
            }
        }

        // Only a tall screen has a wide painting to turn; a TV's wide ones already fit.
        if (!screen.isLandscape && !context.isTelevision()) {
            Spacer(Modifier.height(12.dp))
            ToggleRow(
                title = "Turn wide paintings",
                detail = "Wide paintings fill the screen, viewed with the phone on its side",
                checked = preferences.rotateWide,
                onCheckedChange = { onPreferencesChange(preferences.copy(rotateWide = it)) },
            )
        }

        // A painting must pass every section below — Shape, Origins, Subjects and
        // Artists — to be offered; within a section, checking more than one option
        // widens it. An empty section means Any: it filters nothing. Any and every checked
        // option are always shown — a checked option hidden for matching nothing could
        // never be unchecked. Other options [availableRegions] and its siblings don't
        // return are hidden, computed against what the other sections are staged to.
        FoldableSection(
            title = "Shape",
            summary = shapeLabel(preferences.artworkShape, screen),
            expanded = shapeExpanded,
            onToggle = { shapeExpanded = !shapeExpanded },
            inert = byArtist,
        ) {
            Text(
                if (byArtist) NOT_USED_FOR_ARTIST else "This changes future downloads. It does not fetch a new painting when you apply.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.muted,
                modifier = Modifier.padding(bottom = 8.dp),
            )
            ArtworkShape.entries.forEach { shape ->
                SelectionRow(
                    title = shapeLabel(shape, screen),
                    detail = shapeDetail(shape, screen),
                    selected = preferences.artworkShape == shape,
                    onClick = { if (!byArtist) onPreferencesChange(preferences.copy(artworkShape = shape)) },
                )
            }
        }

        FoldableSection(
            title = "Origins",
            summary = originsSummary,
            expanded = originsExpanded,
            onToggle = { originsExpanded = !originsExpanded },
            inert = byArtist,
        ) {
            Text(
                if (byArtist) NOT_USED_FOR_ARTIST else "Matches any region you check. Any allows every region.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.muted,
                modifier = Modifier.padding(bottom = 8.dp),
            )
            SelectionRow(
                title = "Any",
                selected = preferences.artworkRegions.isEmpty(),
                onClick = { if (!byArtist) onPreferencesChange(preferences.copy(artworkRegions = emptySet())) },
            )
            ArtworkRegion.entries.filter { it in availableRegions || it in preferences.artworkRegions }.forEach { region ->
                SelectionRow(
                    title = regionLabel(region),
                    selected = region in preferences.artworkRegions,
                    onClick = {
                        if (!byArtist) {
                            onPreferencesChange(
                                preferences.copy(artworkRegions = toggled(preferences.artworkRegions, region)),
                            )
                        }
                    },
                )
            }
        }

        FoldableSection(
            title = "Subjects",
            summary = subjectsSummary,
            expanded = subjectsExpanded,
            onToggle = { subjectsExpanded = !subjectsExpanded },
        ) {
            Text(
                "Matches any subject you check. Any allows every subject.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.muted,
                modifier = Modifier.padding(bottom = 8.dp),
            )
            SelectionRow(
                title = "Any",
                selected = preferences.artworkSubjects.isEmpty(),
                onClick = { onPreferencesChange(preferences.copy(artworkSubjects = emptySet())) },
            )
            ArtworkSubject.entries.filter { it in availableSubjects || it in preferences.artworkSubjects }.forEach { subject ->
                SelectionRow(
                    title = subjectLabel(subject),
                    detail = subjectDetail(subject),
                    selected = subject in preferences.artworkSubjects,
                    onClick = {
                        onPreferencesChange(
                            preferences.copy(artworkSubjects = toggled(preferences.artworkSubjects, subject)),
                        )
                    },
                )
            }
        }

        if (allArtists.isNotEmpty()) {
            FoldableSection(
                title = "Artists",
                summary = artistsSummary,
                expanded = artistsExpanded,
                onToggle = { artistsExpanded = !artistsExpanded },
            ) {
                // Two choices, exactly one on. The chosen painter is named even when a newer
                // catalogue no longer has them, so nothing staged is hidden. Mirrors
                // `Pending::artist_row`.
                SelectionRow(
                    title = "Any artist",
                    selected = !byArtist,
                    onClick = { onPreferencesChange(preferences.copy(artworkArtists = emptySet())) },
                )
                if (painters.isNotEmpty()) {
                    SelectionRow(
                        title = preferences.artworkArtists.firstOrNull() ?: "Choose…",
                        selected = byArtist,
                        onClick = { browsing = true },
                    )
                }
            }
        }

        tooFew?.let { matching ->
            Text(
                when (matching) {
                    0 -> "No painting matches these filters — set one section to Any"
                    else -> {
                        val few = if (matching == 1) "Only 1 painting matches" else "Only $matching paintings match"
                        // Thin filters that are already applied block nothing, so the
                        // sentence must not say they do; fetch widens them instead.
                        if (preferences.sameFiltersAs(savedPreferences)) {
                            "$few these filters, so pictures come from a wider selection"
                        } else {
                            "$few these filters — at least ${Catalogue.MIN_POOL} are needed"
                        }
                    }
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(top = 14.dp),
            )
        }

        ToggleRow(
            title = "Hide religious scenes",
            detail = "Skips saints, Madonnas and Bible scenes",
            checked = preferences.hideReligious,
            onCheckedChange = { onPreferencesChange(preferences.copy(hideReligious = it)) },
        )
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 19.dp, bottom = 8.dp),
    )
}

/**
 * One of Shape, Origins, Subjects or Artists: a header naming the section and
 * summarising its current selection, tappable to fold or unfold [content] under it.
 * The header itself (not just the chevron) is the tap target.
 */
@Composable
private fun FoldableSection(
    title: String,
    summary: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    inert: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = Modifier.padding(top = 19.dp)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .focusRing(RoundedCornerShape(8.dp))
                .clickable(onClick = onToggle)
                .padding(vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(
                    summary,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.muted,
                    modifier = Modifier.padding(top = 1.dp),
                )
            }
            Chevron(expanded)
        }
        if (expanded) {
            Column(modifier = Modifier.padding(top = 6.dp).alpha(if (inert) 0.4f else 1f), content = content)
        }
    }
}

/** A small hand-drawn chevron — ∨ folded, ∧ unfolded — rather than pulling in an icon library for one glyph. */
@Composable
private fun Chevron(expanded: Boolean) {
    val color = MaterialTheme.colorScheme.muted
    Canvas(modifier = Modifier.size(18.dp)) {
        val halfWidth = size.width * 0.3f
        val apexY = if (expanded) size.height * 0.35f else size.height * 0.65f
        val baseY = if (expanded) size.height * 0.65f else size.height * 0.35f
        val apex = Offset(size.width / 2f, apexY)
        val stroke = 1.6.dp.toPx()
        drawLine(color, Offset(size.width / 2f - halfWidth, baseY), apex, stroke, cap = StrokeCap.Round)
        drawLine(color, apex, Offset(size.width / 2f + halfWidth, baseY), stroke, cap = StrokeCap.Round)
    }
}

private fun shapeLabel(shape: ArtworkShape, screen: Screen): String = when (shape) {
    ArtworkShape.SCREEN -> if (screen.isLandscape) "TV-shaped" else "Phone-shaped"
    ArtworkShape.NEAR_SQUARE -> "Include near-square"
    ArtworkShape.ANY -> "Any shape"
}

private fun shapeDetail(shape: ArtworkShape, screen: Screen): String = when (shape) {
    ArtworkShape.SCREEN ->
        if (screen.isLandscape) "Wide paintings that need little cropping" else "Tall paintings that need little cropping"
    ArtworkShape.NEAR_SQUARE ->
        if (screen.isLandscape) "Wide, square, and slightly tall paintings" else "Tall, square, and slightly wide paintings"
    ArtworkShape.ANY ->
        if (screen.isLandscape) "Also allow fully vertical paintings" else "Also allow fully horizontal paintings"
}

internal fun regionLabel(region: ArtworkRegion): String = when (region) {
    ArtworkRegion.EUROPE -> "Europe"
    ArtworkRegion.ASIA -> "Asia"
    ArtworkRegion.AFRICA -> "Africa"
    ArtworkRegion.NORTH_AMERICA -> "North America"
    ArtworkRegion.SOUTH_AMERICA -> "South America"
    ArtworkRegion.OCEANIA -> "Oceania"
}

private fun subjectLabel(subject: ArtworkSubject): String = when (subject) {
    ArtworkSubject.LANDSCAPE -> "Landscape"
    ArtworkSubject.SEASCAPE -> "Seascape"
    ArtworkSubject.STILL_LIFE -> "Still life"
}

private fun subjectDetail(subject: ArtworkSubject): String = when (subject) {
    ArtworkSubject.LANDSCAPE -> "Countryside, rivers, skies and views of towns"
    ArtworkSubject.SEASCAPE -> "Sea, ships and harbours; a thinner pool of phone-shaped finds"
    ArtworkSubject.STILL_LIFE -> "Flowers, fruit and tabletops"
}

@Composable
private fun StyleCard(
    style: WallpaperStyle,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(10.dp)
    Surface(
        modifier = modifier
            .height(76.dp)
            .border(
                BorderStroke(1.dp, chipLine(selected)),
                shape,
            )
            .focusRing(shape)
            .clickable(onClick = onClick),
        color = chipFill(selected),
        contentColor = chipInk(selected),
        shape = shape,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            StyleIllustration(style, ink = chipInk(selected), backing = MaterialTheme.colorScheme.background)
            Text(
                when (style) {
                    WallpaperStyle.ZOOM -> "Zoom"
                    WallpaperStyle.STRETCH -> "Stretch"
                    WallpaperStyle.BLUR -> "Blur"
                    WallpaperStyle.BORDERS -> "Borders"
                },
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}

@Composable
private fun StyleIllustration(style: WallpaperStyle, ink: Color, backing: Color) {
    Canvas(modifier = Modifier.size(width = 25.dp, height = 30.dp)) {
        val outline = ink
        val image = ink.copy(alpha = 0.55f)
        val mist = ink.copy(alpha = 0.2f)
        drawRoundRect(outline, style = Stroke(1.dp.toPx()), cornerRadius = androidx.compose.ui.geometry.CornerRadius(2.dp.toPx()))
        when (style) {
            WallpaperStyle.ZOOM -> {
                drawRect(image, topLeft = Offset(size.width * 0.18f, 0f), size = Size(size.width * 0.64f, size.height))
                drawLine(ink, Offset(size.width * 0.24f, size.height * 0.72f), Offset(size.width * 0.52f, size.height * 0.44f), 1.2.dp.toPx())
            }
            WallpaperStyle.STRETCH -> drawRect(image, size = size)
            WallpaperStyle.BLUR -> {
                drawRoundRect(mist, size = size, cornerRadius = androidx.compose.ui.geometry.CornerRadius(2.dp.toPx()))
                drawRect(image, topLeft = Offset(size.width * 0.12f, size.height * 0.29f), size = Size(size.width * 0.76f, size.height * 0.42f))
            }
            WallpaperStyle.BORDERS -> {
                drawRect(backing, size = size)
                drawRect(image, topLeft = Offset(0f, size.height * 0.29f), size = Size(size.width, size.height * 0.42f))
            }
        }
    }
}

@Composable
private fun ConditionalPanel(content: @Composable ColumnScope.() -> Unit) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 2.dp),
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(11.dp),
    ) {
        Column(modifier = Modifier.padding(13.dp), content = content)
    }
}

@Composable
private fun SmallChoice(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier.focusRing(RoundedCornerShape(8.dp)).clickable(onClick = onClick),
        color = chipFill(selected),
        contentColor = chipInk(selected),
        border = BorderStroke(1.dp, chipLine(selected)),
        shape = RoundedCornerShape(8.dp),
    ) {
        Box(
            modifier = Modifier.padding(horizontal = 7.dp, vertical = 7.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(label, style = MaterialTheme.typography.labelMedium)
        }
    }
}

@Composable
private fun SelectionRow(
    title: String,
    detail: String? = null,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val shape = RoundedCornerShape(9.dp)
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 4.dp)
            .border(
                BorderStroke(1.dp, chipLine(selected)),
                shape,
            )
            .focusRing(shape)
            .clickable(onClick = onClick),
        color = chipFill(selected),
        contentColor = chipInk(selected),
        shape = shape,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 11.dp, vertical = 9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val mark = chipInk(selected)
            Canvas(modifier = Modifier.size(16.dp)) {
                drawCircle(mark, style = Stroke(1.4.dp.toPx()))
                if (selected) drawCircle(mark, radius = 3.5.dp.toPx())
            }
            Column(modifier = Modifier.padding(start = 12.dp)) {
                Text(title, style = MaterialTheme.typography.labelLarge)
                if (!detail.isNullOrEmpty()) {
                    Text(
                        detail,
                        style = MaterialTheme.typography.bodySmall,
                        color = chipInk(selected).copy(alpha = 0.7f),
                    )
                }
            }
        }
    }
}

/** A single on/off setting, styled like [SelectionRow] but with a switch instead of a bullet. */
@Composable
private fun ToggleRow(
    title: String,
    detail: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    val shape = RoundedCornerShape(9.dp)
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 4.dp)
            .border(
                BorderStroke(1.dp, chipLine(checked)),
                shape,
            )
            .focusRing(shape)
            .clickable { onCheckedChange(!checked) },
        color = chipFill(checked),
        contentColor = chipInk(checked),
        shape = shape,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 11.dp, vertical = 9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.labelLarge)
                Text(
                    detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = chipInk(checked).copy(alpha = 0.7f),
                )
            }
            Switch(
                checked = checked,
                // The row is the one D-pad stop; a focusable switch inside it would be a second.
                onCheckedChange = null,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = MaterialTheme.colorScheme.onSurface,
                    checkedTrackColor = MaterialTheme.colorScheme.primary,
                ),
            )
        }
    }
}

@Composable
private fun CustomColorControls(
    color: Int,
    currentPictureColors: List<Int>,
    onColor: (Int) -> Unit,
) {
    Text("Suggested palettes", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 18.dp))
    CURATED_PALETTES.forEach { palette ->
        Text(
            palette.first,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.muted,
            modifier = Modifier.padding(top = 8.dp),
        )
        Swatches(palette.second, color, onColor)
    }
    if (currentPictureColors.isNotEmpty()) {
        Text(
            "From this painting",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.muted,
            modifier = Modifier.padding(top = 10.dp),
        )
        Swatches(currentPictureColors, color, onColor)
    }

    // The wheel answers touch only; on a television the swatches are the whole palette.
    if (!LocalContext.current.isTelevision()) {
        Text("Any color", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 18.dp, bottom = 10.dp))
        HsvColorWheel(color, onColor)
    }
}

@Composable
private fun Swatches(colors: List<Int>, selectedColor: Int, onColor: (Int) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(top = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        colors.forEach { color ->
            val selected = (selectedColor or (0xff shl 24)) == (color or (0xff shl 24))
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .focusRing(CircleShape)
                    .border(
                        if (selected) 3.dp else 1.dp,
                        if (selected) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.hairline,
                        CircleShape,
                    )
                    .padding(4.dp)
                    .background(Color(color), CircleShape)
                    .semantics {
                        contentDescription = "Choose color"
                        role = Role.Button
                    }
                    .clickable(onClick = { onColor(color) }),
            )
        }
    }
}

@Composable
private fun HsvColorWheel(color: Int, onColor: (Int) -> Unit) {
    val hsv = remember(color) {
        FloatArray(3).also { AndroidColor.colorToHSV(color, it) }
    }
    val hueColors = remember {
        (0..12).map { Color.hsv((it % 12) * 30f, 1f, 1f) }
    }

    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(
            modifier = Modifier
            .size(210.dp)
            .pointerInput(hsv[2]) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    var change = down
                    do {
                        val center = Offset(size.width / 2f, size.height / 2f)
                        val dx = change.position.x - center.x
                        val dy = change.position.y - center.y
                        val saturation = (hypot(dx, dy) / (minOf(size.width, size.height) / 2f)).coerceIn(0f, 1f)
                        val hue = ((atan2(dy, dx) * 180f / PI.toFloat()) + 360f) % 360f
                        onColor(AndroidColor.HSVToColor(floatArrayOf(hue, saturation, hsv[2].coerceAtLeast(0.05f))))
                        change.consume()
                        val event = awaitPointerEvent()
                        change = event.changes.first()
                    } while (change.pressed)
                }
            },
        ) {
            val radius = size.minDimension / 2f
            drawCircle(brush = Brush.sweepGradient(hueColors), radius = radius)
            drawCircle(
                brush = Brush.radialGradient(listOf(Color.White, Color.Transparent), radius = radius),
                radius = radius,
            )
            if (hsv[2] < 1f) drawCircle(Color.Black.copy(alpha = 1f - hsv[2]), radius = radius)

            val angle = hsv[0] * PI.toFloat() / 180f
            val marker = Offset(
                center.x + cos(angle) * radius * hsv[1],
                center.y + sin(angle) * radius * hsv[1],
            )
            drawCircle(Color.White, radius = 8.dp.toPx(), center = marker, style = Stroke(3.dp.toPx()))
            drawCircle(Color.Black.copy(alpha = 0.45f), radius = 10.dp.toPx(), center = marker, style = Stroke(1.dp.toPx()))
        }
    }
    Text(
        "Brightness",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.padding(top = 10.dp),
    )
    Slider(
        value = hsv[2].coerceAtLeast(0.05f),
        onValueChange = {
            onColor(AndroidColor.HSVToColor(floatArrayOf(hsv[0], hsv[1], it)))
        },
        valueRange = 0.05f..1f,
    )
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .background(Color(color), CircleShape)
                .border(1.dp, MaterialTheme.colorScheme.onSurface, CircleShape),
        )
        Text(
            "#%06X".format(color and 0xffffff),
            style = MaterialTheme.typography.labelLarge,
            modifier = Modifier.padding(start = 10.dp),
        )
    }
}

private val CURATED_PALETTES = listOf(
    "Neutral" to listOf(0xff000000, 0xff28262b, 0xff6f6860, 0xffd8d0c4, 0xfff4f0e8).map(Long::toInt),
    "Warm" to listOf(0xff6e2639, 0xffb85c45, 0xffc99431, 0xff7b4931, 0xffd9aaa0).map(Long::toInt),
    "Cool" to listOf(0xff1d3557, 0xff3434c8, 0xff2a6f6b, 0xff738b6f, 0xffa9a0e8).map(Long::toInt),
)

private const val PREVIEW_WIDTH = 400
private const val PREVIEW_HEIGHT_SHARE = 0.47f

// Long enough to stay crisp at the largest zoom in the preview, short enough to cost about 12 MB.
private const val SHARP_EDGE = 2400
private const val NOT_USED_FOR_ARTIST = "Not used while an artist is chosen."

/**
 * A light 2 dp outline while the element holds D-pad focus, so a remote has a visible
 * cursor. Must sit *before* the `clickable` it decorates: focus is reported to the
 * modifiers outside the focusable node, not to those inside it.
 */
internal fun Modifier.focusRing(shape: Shape): Modifier = composed {
    var focused by remember { mutableStateOf(false) }
    val ink = MaterialTheme.colorScheme.onSurface
    onFocusChanged { focused = it.isFocused }
        .then(if (focused) Modifier.border(2.dp, ink, shape) else Modifier)
}

// The two looks every choosable thing has, as on the desktop's pills: chosen is the accent
// with white on it, plain is a wash with a hairline and the ink. Both draw the same
// 1 dp border, so choosing never changes a size.
@Composable
private fun chipFill(selected: Boolean): Color =
    if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.wash

@Composable
private fun chipInk(selected: Boolean): Color =
    if (selected) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface

@Composable
private fun chipLine(selected: Boolean): Color =
    if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.hairline
