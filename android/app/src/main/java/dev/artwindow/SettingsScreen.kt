package dev.artwindow

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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
    artwork: Artwork?,
    screen: Screen,
    preferences: WallpaperPreferences,
    onPreferencesChange: (WallpaperPreferences) -> Unit,
) {
    var preview by remember { mutableStateOf<Bitmap?>(null) }
    var commonColors by remember { mutableStateOf(emptyList<Int>()) }
    var previewError by remember { mutableStateOf(false) }
    val path = artwork?.path?.takeIf(File::isFile)
    val previewScreen = remember(screen) {
        Screen(PREVIEW_WIDTH, (PREVIEW_WIDTH / screen.aspectRatio).roundToInt())
    }

    LaunchedEffect(path, preferences) {
        if (path == null) {
            preview?.recycle()
            preview = null
            previewError = false
            return@LaunchedEffect
        }
        val next = withContext(Dispatchers.Default) {
            runCatching {
                WallpaperRenderer.render(path, previewScreen, preferences, enforceEnlargementLimit = false)
            }.getOrNull()
        }
        preview?.recycle()
        preview = next
        previewError = next == null
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
        onDispose { preview?.recycle() }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Text(
            "Wallpaper studio",
            style = MaterialTheme.typography.headlineSmall,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            "Frame each work for this screen.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.58f),
            textAlign = TextAlign.Center,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 3.dp, bottom = 14.dp),
        )
        Surface(
            modifier = Modifier
                .align(Alignment.CenterHorizontally)
                .width(150.dp)
                .aspectRatio(screen.aspectRatio.toFloat()),
            shape = RoundedCornerShape(14.dp),
            color = Color(0xff242129),
            shadowElevation = 3.dp,
        ) {
            when {
                preview != null -> Image(
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
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
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

        SectionTitle("Artwork shapes")
        Text(
            "This changes future downloads. It does not fetch a new painting when you apply.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
            modifier = Modifier.padding(bottom = 8.dp),
        )
        ArtworkShape.entries.forEach { shape ->
            SelectionRow(
                title = when (shape) {
                    ArtworkShape.PHONE -> "Phone-shaped"
                    ArtworkShape.NEAR_SQUARE -> "Include near-square"
                    ArtworkShape.ANY -> "Any shape"
                },
                detail = when (shape) {
                    ArtworkShape.PHONE -> "Tall paintings that need little cropping"
                    ArtworkShape.NEAR_SQUARE -> "Tall, square, and slightly wide paintings"
                    ArtworkShape.ANY -> "Also allow fully horizontal paintings"
                },
                selected = preferences.artworkShape == shape,
                onClick = { onPreferencesChange(preferences.copy(artworkShape = shape)) },
            )
        }
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
                BorderStroke(1.dp, if (selected) Color(0xffa990ff) else Color(0xff34303a)),
                shape,
            )
            .clickable(onClick = onClick),
        color = if (selected) Color(0xff282238) else Color(0xff19171d),
        shape = shape,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            StyleIllustration(style)
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
private fun StyleIllustration(style: WallpaperStyle) {
    Canvas(modifier = Modifier.size(width = 25.dp, height = 30.dp)) {
        val outline = Color(0xffaa96ff)
        val image = Color(0xff7965c8)
        val mist = Color(0xff494256)
        drawRoundRect(outline, style = Stroke(1.dp.toPx()), cornerRadius = androidx.compose.ui.geometry.CornerRadius(2.dp.toPx()))
        when (style) {
            WallpaperStyle.ZOOM -> {
                drawRect(image, topLeft = Offset(size.width * 0.18f, 0f), size = Size(size.width * 0.64f, size.height))
                drawLine(Color(0xfff1ecff), Offset(size.width * 0.24f, size.height * 0.72f), Offset(size.width * 0.52f, size.height * 0.44f), 1.2.dp.toPx())
            }
            WallpaperStyle.STRETCH -> drawRect(image, size = size)
            WallpaperStyle.BLUR -> {
                drawRoundRect(mist, size = size, cornerRadius = androidx.compose.ui.geometry.CornerRadius(2.dp.toPx()))
                drawRect(image, topLeft = Offset(size.width * 0.12f, size.height * 0.29f), size = Size(size.width * 0.76f, size.height * 0.42f))
            }
            WallpaperStyle.BORDERS -> {
                drawRect(Color(0xff08070a), size = size)
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
        color = Color(0xff1a181f),
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
        modifier = modifier.clickable(onClick = onClick),
        color = if (selected) Color(0xff7258e8) else Color(0xff2a2730),
        contentColor = if (selected) Color(0xfff7f3ff) else Color(0xffc8c1d0),
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
    detail: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val shape = RoundedCornerShape(9.dp)
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 4.dp)
            .border(
                BorderStroke(1.dp, if (selected) Color(0xff8f79ee) else Color(0xff2e2b33)),
                shape,
            )
            .clickable(onClick = onClick),
        color = if (selected) Color(0xff252031) else Color(0xff17161b),
        shape = shape,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 11.dp, vertical = 9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Canvas(modifier = Modifier.size(16.dp)) {
                drawCircle(if (selected) Color(0xffa990ff) else Color(0xff77717e), style = Stroke(1.4.dp.toPx()))
                if (selected) drawCircle(Color(0xffa990ff), radius = 3.5.dp.toPx())
            }
            Column(modifier = Modifier.padding(start = 12.dp)) {
                Text(title, style = MaterialTheme.typography.labelLarge)
                Text(
                    detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                )
            }
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
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
            modifier = Modifier.padding(top = 8.dp),
        )
        Swatches(palette.second, color, onColor)
    }
    if (currentPictureColors.isNotEmpty()) {
        Text(
            "From this painting",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
            modifier = Modifier.padding(top = 10.dp),
        )
        Swatches(currentPictureColors, color, onColor)
    }

    Text("Any color", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 18.dp, bottom = 10.dp))
    HsvColorWheel(color, onColor)
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
                    .border(
                        if (selected) 3.dp else 1.dp,
                        if (selected) Color(0xffa990ff) else Color(0xff57515e),
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
                .border(1.dp, Color(0xffc8c5cf), CircleShape),
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

private const val PREVIEW_WIDTH = 300
