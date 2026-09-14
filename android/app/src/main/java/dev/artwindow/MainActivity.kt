package dev.artwindow

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import java.io.File
import java.time.LocalDate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        RotationJob.scheduleDaily(applicationContext)
        if (State(applicationContext).isDue(LocalDate.now())) {
            RotationJob.scheduleNow(applicationContext, force = false)
        }

        setContent { ArtWindowApp(applicationContext) }
    }
}

private enum class Destination { ARTWORK, FAVOURITES, SETTINGS }

private val settingsColors = darkColorScheme(
    primary = Color(0xffa990ff),
    onPrimary = Color(0xff17121f),
    background = Color(0xff111015),
    surface = Color(0xff1a181f),
    onSurface = Color(0xffeeeaf3),
)

@Composable
private fun MainActivity.ArtWindowApp(context: Context) {
    var destination by remember { mutableStateOf(Destination.ARTWORK) }
    val status by Rotation.status.collectAsState()
    var artwork by remember { mutableStateOf(State(context).shownArtwork) }
    val initialFavourites = remember { runCatching { Favourites(context).list() } }
    var favourites by remember { mutableStateOf(initialFavourites.getOrDefault(emptyList())) }
    var favouritesError by remember { mutableStateOf(initialFavourites.exceptionOrNull()?.message) }
    val store = remember { WallpaperPreferencesStore(context) }
    var savedPreferences by remember { mutableStateOf(store.load()) }
    var draftPreferences by remember { mutableStateOf(savedPreferences) }
    var applyMessage by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val physicalScreen = remember { context.screen() }

    BackHandler(enabled = destination != Destination.ARTWORK) {
        destination = Destination.ARTWORK
    }

    LaunchedEffect(destination) {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }
    }
    LaunchedEffect(status) {
        if (status is Status.Idle) {
            artwork = State(context).shownArtwork
            runCatching { Favourites(context).list() }
                .onSuccess {
                    favourites = it
                    favouritesError = null
                }
                .onFailure { favouritesError = it.message ?: it.toString() }
        }
    }

    val scheme = if (destination == Destination.SETTINGS) settingsColors else darkColorScheme()
    MaterialTheme(colorScheme = scheme) {
        Surface(color = MaterialTheme.colorScheme.background) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .safeDrawingPadding(),
            ) {
                Box(modifier = Modifier.weight(1f)) {
                    when (destination) {
                        Destination.ARTWORK -> ArtworkScreen(
                            context = context,
                            artwork = artwork,
                            isFavourite = artwork?.let { shown ->
                                favourites.any { saved -> sameArtwork(saved.artwork, shown) }
                            } == true,
                            favouriteCount = favourites.size,
                            favouritesError = favouritesError,
                            actionsEnabled = !status.isBusy,
                            onToggleFavourite = {
                                scope.launch {
                                    val changed = withContext(Dispatchers.IO) { Rotation.toggleFavourite(context) }
                                    if (!changed && Rotation.status.value !is Status.Failed) {
                                        favouritesError = "Another wallpaper update is already running"
                                    }
                                }
                            },
                            onViewFavourites = { destination = Destination.FAVOURITES },
                        )
                        Destination.FAVOURITES -> FavouritesScreen(
                            favourites = favourites,
                            enabled = !status.isBusy,
                            error = favouritesError ?: (status as? Status.Failed)?.message,
                            onShow = { key ->
                                scope.launch {
                                    val changed = withContext(Dispatchers.IO) { Rotation.showFavourite(context, key) }
                                    if (!changed && Rotation.status.value !is Status.Failed) {
                                        favouritesError = "Another wallpaper update is already running"
                                    }
                                }
                            },
                            onForget = { key ->
                                scope.launch {
                                    val changed = withContext(Dispatchers.IO) { Rotation.forgetFavourite(context, key) }
                                    if (!changed && Rotation.status.value !is Status.Failed) {
                                        favouritesError = "Another wallpaper update is already running"
                                    }
                                }
                            },
                        )
                        Destination.SETTINGS -> SettingsScreen(
                            artwork = artwork,
                            screen = physicalScreen,
                            preferences = draftPreferences,
                            onPreferencesChange = {
                                draftPreferences = it
                                applyMessage = null
                            },
                        )
                    }
                }

                if (destination == Destination.ARTWORK) {
                    Button(
                        onClick = { RotationJob.scheduleNow(context, force = true) },
                        enabled = !status.isBusy,
                        modifier = Modifier
                            .align(Alignment.CenterHorizontally)
                            .size(width = 220.dp, height = 44.dp),
                    ) {
                        Text(if (status is Status.Fetching) "Fetching…" else "Next picture")
                    }
                    Text(
                        text = statusLine(status, owed = State(context).isDue(LocalDate.now())),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier
                            .align(Alignment.CenterHorizontally)
                            .padding(top = 6.dp),
                    )
                } else if (destination == Destination.SETTINGS) {
                    Button(
                        onClick = {
                            val requested = draftPreferences
                            scope.launch {
                                val applied = withContext(Dispatchers.IO) {
                                    Rotation.applyPreferences(context, requested)
                                }
                                if (applied) {
                                    savedPreferences = requested
                                    applyMessage = if (artwork?.path?.isFile == true) {
                                        "Wallpaper and settings updated"
                                    } else {
                                        "Settings saved for the first picture"
                                    }
                                } else if (Rotation.status.value !is Status.Failed) {
                                    applyMessage = "Another wallpaper update is already running"
                                }
                            }
                        },
                        enabled = !status.isBusy && draftPreferences != savedPreferences,
                        modifier = Modifier
                            .align(Alignment.CenterHorizontally)
                            .size(width = 220.dp, height = 44.dp),
                    ) {
                        Text(if (status is Status.Applying) "Applying…" else "Apply changes")
                    }
                    (applyMessage ?: (status as? Status.Failed)?.message)?.let {
                        Text(
                            text = it,
                            style = MaterialTheme.typography.bodySmall,
                            color = if (status is Status.Failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                            modifier = Modifier
                                .align(Alignment.CenterHorizontally)
                                .padding(top = 6.dp),
                        )
                    }
                }

                NavigationPill(
                    destination = if (destination == Destination.FAVOURITES) Destination.ARTWORK else destination,
                    onDestination = { destination = it },
                    modifier = Modifier
                        .align(Alignment.CenterHorizontally)
                        .padding(top = 8.dp, bottom = 20.dp),
                )
            }
        }
    }
}

@Composable
private fun ArtworkScreen(
    context: Context,
    artwork: Artwork?,
    isFavourite: Boolean,
    favouriteCount: Int,
    favouritesError: String?,
    actionsEnabled: Boolean,
    onToggleFavourite: () -> Unit,
    onViewFavourites: () -> Unit,
) {
    var preview by remember { mutableStateOf<Bitmap?>(null) }
    val screen = remember { context.screen() }

    LaunchedEffect(artwork?.path) {
        val next = artwork?.path?.takeIf { it.isFile }?.let { path ->
            withContext(Dispatchers.Default) { decodeSampled(path, TARGET_PREVIEW_WIDTH) }
        }
        preview?.recycle()
        preview = next
    }
    DisposableEffect(Unit) {
        onDispose { preview?.recycle() }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(screen.width.toFloat() / screen.height.toFloat()),
        ) {
            preview?.let {
                Image(
                    bitmap = it.asImageBitmap(),
                    contentDescription = artwork?.title,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }
            Surface(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(10.dp)
                    .size(42.dp)
                    .semantics {
                        contentDescription = if (isFavourite) "Remove from favourites" else "Add to favourites"
                        role = Role.Button
                    }
                    .clickable(enabled = artwork != null && actionsEnabled, onClick = onToggleFavourite),
                color = Color(0xcc17121f),
                shape = RoundedCornerShape(21.dp),
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Text(
                        if (isFavourite) "♥" else "♡",
                        color = if (isFavourite) Color(0xffff8aa3) else Color.White,
                        style = MaterialTheme.typography.titleLarge,
                    )
                }
            }
        }

        Column(modifier = Modifier.padding(top = 16.dp)) {
            Text(artwork?.title ?: "No painting yet", style = MaterialTheme.typography.titleLarge)
            artwork?.byline?.takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodyMedium)
            }
            artwork?.attribution?.takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall)
            }
            artwork?.detailsUrl?.let { url ->
                Text(
                    "See it at the Met",
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier
                        .padding(top = 4.dp)
                        .clickable { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) },
                )
            }
            artwork?.origin?.let {
                Text(
                    "Origin: $it",
                    color = MaterialTheme.colorScheme.primary,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
            Text(
                "View favourites ($favouriteCount)",
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier
                    .padding(top = 12.dp)
                    .clickable(onClick = onViewFavourites),
            )
            favouritesError?.let {
                Text(
                    it,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
        }
    }
}

@Composable
private fun NavigationPill(
    destination: Destination,
    onDestination: (Destination) -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        color = Color(0xff292532),
        shape = RoundedCornerShape(12.dp),
        shadowElevation = 4.dp,
    ) {
        Row(
            modifier = Modifier.padding(4.dp),
            horizontalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            listOf(Destination.ARTWORK, Destination.SETTINGS).forEach { item ->
                val isSelected = item == destination
                Box(
                    modifier = Modifier
                        .size(width = 52.dp, height = 42.dp)
                        .semantics {
                            contentDescription = if (item == Destination.ARTWORK) "Artwork" else "Settings"
                            role = Role.Tab
                            selected = isSelected
                        }
                        .clickable { onDestination(item) },
                    contentAlignment = Alignment.Center,
                ) {
                    if (isSelected) {
                        Surface(
                            modifier = Modifier.fillMaxSize(),
                            color = Color(0xff7258e8),
                            shape = RoundedCornerShape(8.dp),
                        ) {}
                    }
                    NavigationIcon(
                        destination = item,
                        color = if (isSelected) Color(0xfff5f1ff) else Color(0xffa59caf),
                    )
                }
            }
        }
    }
}

@Composable
private fun NavigationIcon(destination: Destination, color: Color) {
    Canvas(modifier = Modifier.size(21.dp)) {
        val stroke = 1.7.dp.toPx()
        if (destination == Destination.ARTWORK || destination == Destination.FAVOURITES) {
            drawRoundRect(color, style = Stroke(stroke), cornerRadius = androidx.compose.ui.geometry.CornerRadius(3.dp.toPx()))
            drawCircle(color, radius = 1.7.dp.toPx(), center = Offset(size.width * 0.72f, size.height * 0.28f))
            drawLine(color, Offset(size.width * 0.12f, size.height * 0.78f), Offset(size.width * 0.42f, size.height * 0.48f), stroke, StrokeCap.Round)
            drawLine(color, Offset(size.width * 0.42f, size.height * 0.48f), Offset(size.width * 0.62f, size.height * 0.67f), stroke, StrokeCap.Round)
            drawLine(color, Offset(size.width * 0.62f, size.height * 0.67f), Offset(size.width * 0.88f, size.height * 0.42f), stroke, StrokeCap.Round)
        } else {
            val ys = listOf(0.25f, 0.5f, 0.75f)
            val knobs = listOf(0.34f, 0.68f, 0.45f)
            ys.forEachIndexed { index, y ->
                drawLine(color, Offset(size.width * 0.12f, size.height * y), Offset(size.width * 0.88f, size.height * y), stroke, StrokeCap.Round)
                drawCircle(color, 2.4.dp.toPx(), Offset(size.width * knobs[index], size.height * y))
            }
        }
    }
}

private fun statusLine(status: Status, owed: Boolean): String = when (status) {
    is Status.Fetching -> "Fetching…"
    is Status.Applying -> "Applying wallpaper…"
    is Status.SavingFavourite -> "Updating favourites…"
    is Status.Failed -> status.message
    is Status.Idle -> if (owed) "Today's painting arrives on the next Wi-Fi check" else "A new painting arrives tomorrow"
}

private const val TARGET_PREVIEW_WIDTH = 720

internal fun decodeSampled(file: File, targetWidth: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    if (bounds.outWidth <= 0) return null

    var sampleSize = 1
    while (bounds.outWidth / (sampleSize * 2) >= targetWidth) sampleSize *= 2
    return BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sampleSize })
}

private val Status.isBusy: Boolean
    get() = this is Status.Fetching || this is Status.Applying || this is Status.SavingFavourite
