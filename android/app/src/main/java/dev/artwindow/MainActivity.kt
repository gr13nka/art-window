package dev.artwindow

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import java.io.File
import java.time.LocalDate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The one screen: today's painting, who painted it, and a way to ask for another.
 *
 * It never fetches on its own — [Rotation] is the only place a fetch happens — it
 * only ensures the schedule exists, shows whatever [State] and [Rotation.status]
 * currently say, and re-reads [State] once a turn reports it is done.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        RotationJob.scheduleDaily(applicationContext)
        if (State(applicationContext).isDue(LocalDate.now())) {
            RotationJob.scheduleNow(applicationContext, force = false)
        }

        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(color = MaterialTheme.colorScheme.background) {
                    ArtWindowScreen(applicationContext)
                }
            }
        }
    }
}

@Composable
private fun ArtWindowScreen(context: Context) {
    val status by Rotation.status.collectAsState()
    var artwork by remember { mutableStateOf(State(context).artwork) }
    var preview by remember { mutableStateOf<Bitmap?>(null) }
    val screen = remember { context.screen() }

    // The activity re-reads State only when a turn finishes, rather than polling or
    // fetching anything itself — see the class doc.
    LaunchedEffect(status) {
        if (status is Status.Idle) {
            artwork = State(context).artwork
        }
    }
    LaunchedEffect(artwork?.path) {
        val path = artwork?.path
        preview = if (path != null) withContext(Dispatchers.Default) { decodeSampled(path, TARGET_PREVIEW_WIDTH) } else null
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
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
        }

        Column(modifier = Modifier.padding(top = 16.dp)) {
            Text(artwork?.title ?: "No painting yet", style = MaterialTheme.typography.titleLarge)
            artwork?.byline?.takeIf { it.isNotEmpty() }?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
            artwork?.attribution?.takeIf { it.isNotEmpty() }?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            artwork?.detailsUrl?.let { url ->
                Text(
                    "See it at the Met",
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier
                        .padding(top = 4.dp)
                        .clickable { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) },
                )
            }
        }

        Button(
            onClick = { RotationJob.scheduleNow(context, force = true) },
            enabled = status !is Status.Fetching,
            modifier = Modifier.padding(top = 16.dp),
        ) {
            Text("Next picture")
        }

        Text(
            text = statusLine(status, owed = State(context).isDue(LocalDate.now())),
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(top = 8.dp),
        )
    }
}

/** [owed] separates a day already settled from one whose picture is still waiting — usually for Wi-Fi. */
private fun statusLine(status: Status, owed: Boolean): String = when (status) {
    is Status.Fetching -> "Fetching…"
    is Status.Failed -> status.message
    is Status.Idle -> if (owed) "Today's painting arrives on the next Wi-Fi check" else "A new painting arrives tomorrow"
}

private const val TARGET_PREVIEW_WIDTH = 720

/** Decodes [file] at roughly [targetWidth] pixels wide — cheap enough for a screen-sized preview box, however big the original. */
private fun decodeSampled(file: File, targetWidth: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    if (bounds.outWidth <= 0) return null

    var sampleSize = 1
    while (bounds.outWidth / (sampleSize * 2) >= targetWidth) sampleSize *= 2

    val options = BitmapFactory.Options().apply { inSampleSize = sampleSize }
    return BitmapFactory.decodeFile(file.path, options)
}
