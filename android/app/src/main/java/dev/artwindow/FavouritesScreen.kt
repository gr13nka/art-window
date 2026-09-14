package dev.artwindow

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
fun FavouritesScreen(
    favourites: List<Favourite>,
    enabled: Boolean,
    error: String?,
    onShow: (String) -> Unit,
    onForget: (String) -> Unit,
) {
    var selectedKey by remember { mutableStateOf<String?>(null) }
    val selected = favourites.firstOrNull { it.key == selectedKey }
    LaunchedEffect(favourites, selectedKey) {
        if (selectedKey != null && selected == null) selectedKey = null
    }
    BackHandler(enabled = selected != null) { selectedKey = null }

    if (selected == null) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 20.dp, vertical = 16.dp),
        ) {
            Text("Favourites", style = MaterialTheme.typography.headlineSmall)
            Text(
                if (favourites.isEmpty()) "Tap the heart on a painting to keep it here."
                else "${favourites.size} saved ${if (favourites.size == 1) "painting" else "paintings"}",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                modifier = Modifier.padding(top = 4.dp, bottom = 14.dp),
            )
            error?.let {
                Text(
                    it,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(bottom = 10.dp),
                )
            }
            LazyVerticalGrid(
                columns = GridCells.Adaptive(140.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(favourites, key = { it.key }) { favourite ->
                    FavouriteCard(favourite) { selectedKey = favourite.key }
                }
            }
        }
    } else {
        FavouriteDetail(
            favourite = selected,
            enabled = enabled,
            onBack = { selectedKey = null },
            onShow = { onShow(selected.key) },
            onForget = {
                onForget(selected.key)
                selectedKey = null
            },
        )
    }
}

@Composable
private fun FavouriteCard(favourite: Favourite, onClick: () -> Unit) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(10.dp),
    ) {
        Column {
            SampledImage(
                favourite.artwork,
                targetWidth = THUMBNAIL_WIDTH,
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(0.72f),
            )
            Text(
                favourite.artwork.title,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 2,
                modifier = Modifier.padding(10.dp),
            )
        }
    }
}

@Composable
private fun FavouriteDetail(
    favourite: Favourite,
    enabled: Boolean,
    onBack: () -> Unit,
    onShow: () -> Unit,
    onForget: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 20.dp, vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TextButton(onClick = onBack, modifier = Modifier.align(Alignment.Start)) { Text("Back to favourites") }
        SampledImage(
            favourite.artwork,
            targetWidth = DETAIL_WIDTH,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
        )
        Column(modifier = Modifier.fillMaxWidth().padding(top = 12.dp)) {
            Text(favourite.artwork.title, style = MaterialTheme.typography.titleLarge)
            favourite.artwork.byline.takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodyMedium)
            }
            favourite.artwork.origin?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
            }
            favourite.artwork.attribution.takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall)
            }
        }
        Row(
            modifier = Modifier.padding(top = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Button(onClick = onShow, enabled = enabled) { Text("Set as wallpaper") }
            TextButton(onClick = onForget, enabled = enabled) { Text("Remove") }
        }
    }
}

@Composable
private fun SampledImage(artwork: Artwork, targetWidth: Int, modifier: Modifier = Modifier) {
    var bitmap by remember(artwork.path) { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(artwork.path, targetWidth) {
        bitmap = withContext(Dispatchers.Default) { decodeSampled(artwork.path, targetWidth) }
    }
    DisposableEffect(artwork.path) {
        onDispose { bitmap?.recycle() }
    }
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        bitmap?.let {
            Image(
                bitmap = it.asImageBitmap(),
                contentDescription = artwork.title,
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

private const val THUMBNAIL_WIDTH = 360
private const val DETAIL_WIDTH = 900
