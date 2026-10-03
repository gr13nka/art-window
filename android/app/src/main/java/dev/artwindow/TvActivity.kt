package dev.artwindow

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The television's launchable art mode: the current painting full screen, and — on the
 * remote's OK button — a small overlay of the few things worth doing to it.
 *
 * Like [ArtDream] it only observes [Frame]; placement was decided when the picture was
 * pinned, so this never crops or scales anything itself. Settings and Favourites are the
 * phone's own screens, reached through the overlay.
 */
class TvActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            hide(WindowInsetsCompat.Type.systemBars())
        }

        RotationJob.scheduleWatcher(applicationContext)
        if (State(applicationContext).isDue(Day.today())) {
            RotationJob.scheduleNow(applicationContext, force = false)
        }

        setContent { TvApp(applicationContext, openDreamSettings = { openDreamSettings() }) }
    }

    /** Returns whether the system has a screensaver picker; Google TV hides it for third-party dreams. */
    private fun openDreamSettings(): Boolean = try {
        startActivity(Intent(Settings.ACTION_DREAM_SETTINGS))
        true
    } catch (_: ActivityNotFoundException) {
        false
    }
}

private enum class TvDestination { ARTWORK, FAVOURITES, SETTINGS }

private const val SCREENSAVER_ADB_COMMAND =
    "adb shell settings put secure screensaver_components dev.artwindow/.ArtDream"

@Composable
private fun TvApp(context: Context, openDreamSettings: () -> Boolean) {
    var destination by remember { mutableStateOf(TvDestination.ARTWORK) }
    var overlayOpen by remember { mutableStateOf(false) }
    var screensaverHint by remember { mutableStateOf(false) }
    val status by Rotation.status.collectAsState()
    val rendered by Frame.shown.collectAsState()
    var shownArtwork by remember { mutableStateOf(State(context).shownArtwork) }
    val initialFavourites = remember { runCatching { Favourites(context).list() } }
    var favourites by remember { mutableStateOf(initialFavourites.getOrDefault(emptyList())) }
    var favouritesError by remember { mutableStateOf(initialFavourites.exceptionOrNull()?.message) }
    val store = remember { WallpaperPreferencesStore(context) }
    var savedPreferences by remember { mutableStateOf(store.load()) }
    var draftPreferences by remember { mutableStateOf(savedPreferences) }
    var applyMessage by remember { mutableStateOf<String?>(null) }
    var choicesAvailable by remember { mutableStateOf(true) }
    val scope = rememberCoroutineScope()
    val physicalScreen = remember { context.screen() }
    val artwork = rendered?.artwork ?: shownArtwork

    // A cold start has nothing published yet; render once from the saved state.
    LaunchedEffect(Unit) {
        withContext(Dispatchers.Default) { Frame.current(context) }
    }
    LaunchedEffect(status) {
        if (status is Status.Idle) {
            shownArtwork = State(context).shownArtwork
            runCatching { Favourites(context).list() }
                .onSuccess {
                    favourites = it
                    favouritesError = null
                }
                .onFailure { favouritesError = it.message ?: it.toString() }
        }
    }

    BackHandler(enabled = destination != TvDestination.ARTWORK || overlayOpen) {
        if (destination != TvDestination.ARTWORK) destination = TvDestination.ARTWORK else overlayOpen = false
    }

    val root = remember { FocusRequester() }
    val firstButton = remember { FocusRequester() }
    LaunchedEffect(destination, overlayOpen) {
        if (destination != TvDestination.ARTWORK) return@LaunchedEffect
        if (overlayOpen) firstButton.requestFocus() else root.requestFocus()
    }

    ArtWindowTheme {
        // Black behind a painting; the other screens are ordinary ones.
        Surface(
            color = if (destination == TvDestination.ARTWORK) Color.Black else MaterialTheme.colorScheme.background,
            modifier = Modifier.fillMaxSize(),
        ) {
            when (destination) {
                TvDestination.ARTWORK -> Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .focusRequester(root)
                        .onPreviewKeyEvent { event ->
                            val select = event.key == Key.DirectionCenter || event.key == Key.Enter || event.key == Key.NumPadEnter
                            if (select && event.type == KeyEventType.KeyUp && !overlayOpen) {
                                overlayOpen = true
                                true
                            } else {
                                select && !overlayOpen
                            }
                        }
                        .focusable(),
                ) {
                    rendered?.let {
                        Image(
                            bitmap = it.bitmap.asImageBitmap(),
                            contentDescription = it.artwork.title,
                            contentScale = ContentScale.Fit,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    if (overlayOpen) {
                        Overlay(
                            artwork = artwork,
                            statusText = statusLine(status, owed = State(context).isDue(Day.today())),
                            isFavourite = artwork?.let { shown -> favourites.any { sameArtwork(it.artwork, shown) } } == true,
                            fetching = status is Status.Fetching,
                            canAct = !status.isBusy,
                            canFavourite = artwork != null && !status.isBusy,
                            screensaverHint = screensaverHint,
                            firstButton = firstButton,
                            onNext = { RotationJob.scheduleNow(context, force = true) },
                            onFavourite = {
                                scope.launch {
                                    val changed = withContext(Dispatchers.IO) { Rotation.toggleFavourite(context) }
                                    if (!changed && Rotation.status.value !is Status.Failed) {
                                        favouritesError = "Another wallpaper update is already running"
                                    }
                                }
                            },
                            onSettings = { destination = TvDestination.SETTINGS },
                            onFavourites = { destination = TvDestination.FAVOURITES },
                            onScreensaver = { screensaverHint = !openDreamSettings() },
                            favouritesError = favouritesError,
                            modifier = Modifier.align(Alignment.BottomCenter),
                        )
                    }
                }
                TvDestination.FAVOURITES -> FavouritesScreen(
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
                TvDestination.SETTINGS -> Column(modifier = Modifier.fillMaxSize()) {
                    Box(modifier = Modifier.weight(1f)) {
                        SettingsScreen(
                            context = context,
                            artwork = artwork,
                            screen = physicalScreen,
                            preferences = draftPreferences,
                            savedPreferences = savedPreferences,
                            onPreferencesChange = {
                                draftPreferences = it
                                applyMessage = null
                            },
                            onChoicesAvailable = { choicesAvailable = it },
                        )
                    }
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
                                        "Picture and settings updated"
                                    } else {
                                        "Settings saved for the first picture"
                                    }
                                } else if (Rotation.status.value !is Status.Failed) {
                                    applyMessage = "Another picture update is already running"
                                }
                            }
                        },
                        enabled = !status.isBusy && draftPreferences != savedPreferences && choicesAvailable,
                        modifier = Modifier
                            .align(Alignment.CenterHorizontally)
                            .focusRing(RoundedCornerShape(20.dp))
                            .padding(top = 8.dp),
                    ) {
                        Text(if (status is Status.Applying) "Applying…" else "Apply changes")
                    }
                    (applyMessage ?: (status as? Status.Failed)?.message)?.let {
                        Text(
                            text = it,
                            style = MaterialTheme.typography.bodySmall,
                            color = if (status is Status.Failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.muted,
                            modifier = Modifier
                                .align(Alignment.CenterHorizontally)
                                .padding(vertical = 6.dp),
                        )
                    }
                }
            }
        }
    }
}

/** The bottom sheet over the painting: what it is, what the app is doing, and the few actions on offer. */
@Composable
private fun Overlay(
    artwork: Artwork?,
    statusText: String,
    isFavourite: Boolean,
    fetching: Boolean,
    canAct: Boolean,
    canFavourite: Boolean,
    screensaverHint: Boolean,
    favouritesError: String?,
    firstButton: FocusRequester,
    onNext: () -> Unit,
    onFavourite: () -> Unit,
    onSettings: () -> Unit,
    onFavourites: () -> Unit,
    onScreensaver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.9f))))
            .padding(horizontal = 48.dp, vertical = 32.dp),
    ) {
        Text(artwork?.title ?: "No painting yet", style = MaterialTheme.typography.headlineSmall, color = Color.White)
        artwork?.byline?.takeIf { it.isNotEmpty() }?.let {
            Text(it, style = MaterialTheme.typography.bodyLarge, color = Color.White)
        }
        Text(
            statusText,
            style = MaterialTheme.typography.bodyMedium,
            color = Color.White.copy(alpha = 0.7f),
            modifier = Modifier.padding(top = 4.dp),
        )
        favouritesError?.let {
            Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
        Row(
            modifier = Modifier.padding(top = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OverlayButton(
                if (fetching) "Fetching…" else "Next picture",
                onNext,
                enabled = canAct,
                modifier = Modifier.focusRequester(firstButton),
            )
            OverlayButton(if (isFavourite) "♥ Favourite" else "♡ Favourite", onFavourite, enabled = canFavourite)
            OverlayButton("Favourites", onFavourites)
            OverlayButton("Settings", onSettings)
            OverlayButton("Use as screensaver", onScreensaver)
        }
        if (screensaverHint) {
            Text(
                "Google TV hides third-party screensavers. Run this from a computer instead:",
                style = MaterialTheme.typography.bodySmall,
                color = Color.White.copy(alpha = 0.7f),
                modifier = Modifier.padding(top = 12.dp),
            )
            Text(SCREENSAVER_ADB_COMMAND, style = MaterialTheme.typography.bodyMedium, color = Color.White)
        }
    }
}

@Composable
private fun OverlayButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.focusRing(RoundedCornerShape(20.dp)),
    ) {
        Text(label)
    }
}
