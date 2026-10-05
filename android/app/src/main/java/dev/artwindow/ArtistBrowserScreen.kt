package dev.artwindow

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Where Settings sends someone to choose painters: a shelf of one picture per painter,
 * and, once one is tapped, that painter's best-known painting large with what to know
 * about them. Built like [FavouritesScreen] so the two read as one kind of screen.
 *
 * Android's own copy of `Pending::artist_cards` on the desktop and the browser in
 * `ios/ArtWindow` — the rules for what a card says and when *Choose* is refused belong in
 * all three. Choosing only stages: [onChoose] changes the same staged choice the Settings
 * row shows, and *Apply changes* there still commits it. This screen owns no copy.
 *
 * [painters] are already ordered; [paintings] is each one's count from
 * [Catalogue.paintingsBy]; [blocks] is [Catalogue.artistBlocks] for what is staged.
 */
@Composable
fun ArtistBrowserScreen(
    painters: List<Artists.Painter>,
    paintings: Map<String, Int>,
    chosen: Set<String>,
    blocks: Map<String, ArtistBlock>,
    onChoose: (String) -> Unit,
    onClose: () -> Unit,
) {
    var selectedName by rememberSaveable { mutableStateOf<String?>(null) }
    val selected = painters.firstOrNull { it.name == selectedName }
    BackHandler { if (selected != null) selectedName = null else onClose() }

    if (selected == null) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 20.dp, vertical = 16.dp),
        ) {
            Text(
                "Artists",
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.padding(bottom = 14.dp),
            )
            LazyVerticalGrid(
                columns = GridCells.Adaptive(140.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(painters, key = { it.name }) { painter ->
                    PainterCard(painter, ticked = painter.name in chosen) { selectedName = painter.name }
                }
            }
        }
    } else {
        PainterDetail(
            painter = selected,
            paintings = paintings[selected.name] ?: 0,
            isChosen = selected.name in chosen,
            blocked = blockedReason(selected.name, chosen, blocks),
            onBack = { selectedName = null },
            onChoose = { onChoose(selected.name) },
        )
    }
}

/**
 * Why [name] cannot be made the staged choice, or null when it can. A painter
 * already chosen is never blocked, so the screen never disagrees with what is staged. A painter
 * wins over Shape, Origins and the floor, so only Subject and the religious toggle (or,
 * with neither to blame, the screen's size limits) can empty one: [blocks] says which.
 */
internal fun blockedReason(name: String, chosen: Set<String>, blocks: Map<String, ArtistBlock>): String? {
    if (name in chosen) return null
    return when (blocks[name] ?: return null) {
        ArtistBlock.SUBJECT -> "None of $name's paintings match the chosen subject."
        ArtistBlock.RELIGIOUS -> "None of $name's paintings are left with religious scenes hidden."
        ArtistBlock.SIZE -> "None of $name's paintings can be shown on this screen."
    }
}

@Composable
private fun PainterCard(painter: Artists.Painter, ticked: Boolean, onClick: () -> Unit) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .focusRing(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick),
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(10.dp),
    ) {
        Column {
            Box {
                PainterImage(
                    painter,
                    targetWidth = THUMBNAIL_WIDTH,
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(0.72f),
                )
                if (ticked) Tick(Modifier.align(Alignment.TopEnd).padding(8.dp))
            }
            Text(
                painter.name,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 2,
                modifier = Modifier.padding(10.dp),
            )
        }
    }
}

/** The mark of a chosen painter on the shelf, drawn rather than pulling in an icon library for one glyph. */
@Composable
private fun Tick(modifier: Modifier = Modifier) {
    val disc = MaterialTheme.colorScheme.primary
    val mark = MaterialTheme.colorScheme.onPrimary
    Canvas(modifier = modifier.size(22.dp).clip(CircleShape)) {
        drawCircle(disc)
        val stroke = 2.dp.toPx()
        val corner = Offset(size.width * 0.44f, size.height * 0.68f)
        drawLine(mark, Offset(size.width * 0.27f, size.height * 0.52f), corner, stroke, cap = StrokeCap.Round)
        drawLine(mark, corner, Offset(size.width * 0.74f, size.height * 0.34f), stroke, cap = StrokeCap.Round)
    }
}

@Composable
private fun PainterDetail(
    painter: Artists.Painter,
    paintings: Int,
    isChosen: Boolean,
    blocked: String?,
    onBack: () -> Unit,
    onChoose: () -> Unit,
) {
    val context = LocalContext.current
    val about = remember(painter.aboutUrl) { Intent(Intent.ACTION_VIEW, Uri.parse(painter.aboutUrl)) }
    // A TV often has no browser; offering a button that does nothing would be worse than none.
    val canRead = remember(about) { about.resolveActivity(context.packageManager) != null }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 20.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TextButton(onClick = onBack, colors = quietButtonColors(), modifier = Modifier.align(Alignment.Start)) { Text("Back to artists") }
        PainterImage(
            painter,
            targetWidth = DETAIL_WIDTH,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
        )
        Column(modifier = Modifier.fillMaxWidth().padding(top = 12.dp)) {
            Text(painter.name, style = MaterialTheme.typography.titleLarge)
            Text(Artists.line(painter, paintings), style = MaterialTheme.typography.bodyMedium)
            blocked?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
        }
        Row(
            modifier = Modifier.padding(top = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Button(onClick = onChoose, enabled = blocked == null) { Text(if (isChosen) "Chosen" else "Choose") }
            if (canRead) {
                TextButton(onClick = { context.startActivity(about) }, colors = quietButtonColors()) { Text("Read more") }
            }
        }
    }
}

@Composable
private fun PainterImage(painter: Artists.Painter, targetWidth: Int, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var bitmap by remember(painter.file) { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(painter.file, targetWidth) {
        bitmap = withContext(Dispatchers.Default) { loadPicture(context, painter, targetWidth) }
    }
    DisposableEffect(painter.file) {
        onDispose { bitmap?.recycle() }
    }
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        bitmap?.let {
            Image(
                bitmap = it.asImageBitmap(),
                contentDescription = painter.name,
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

// A picture that cannot be read is an empty frame with the painter's name under it, not a failed screen.
private fun loadPicture(context: Context, painter: Artists.Painter, targetWidth: Int): Bitmap? =
    runCatching { Artists.load(context).picture(context, painter, targetWidth) }.getOrNull()

private const val THUMBNAIL_WIDTH = 360
private const val DETAIL_WIDTH = 900
