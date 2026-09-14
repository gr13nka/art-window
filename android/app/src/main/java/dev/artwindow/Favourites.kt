package dev.artwindow

import android.content.Context
import android.util.AtomicFile
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import org.json.JSONArray
import org.json.JSONObject

data class Favourite(val key: String, val artwork: Artwork)

/** Durable copies of paintings the user chose to keep, and the index describing them. */
class Favourites(context: Context) {
    private val directory = File(context.filesDir, DIRECTORY)
    private val index = AtomicFile(File(directory, INDEX))

    fun list(): List<Favourite> = read().map { Favourite(it.key, it.artwork) }

    fun holds(artwork: Artwork): Boolean = read().any { it.matches(artwork) }

    fun get(key: String): Artwork? = read().firstOrNull { it.key == key }?.artwork

    fun keep(artwork: Artwork): Favourite {
        val kept = read().toMutableList()
        kept.firstOrNull { it.matches(artwork) }?.let { return Favourite(it.key, it.artwork) }

        directory.mkdirs()
        val alreadyOwned = sameFile(artwork.path.parentFile, directory)
        val copy = if (alreadyOwned) artwork.path else freeName(artwork.path, kept)
        if (!alreadyOwned) artwork.path.copyTo(copy)
        val item = Kept(
            key = copy.name,
            origin = artwork.path.path,
            artwork = artwork.copy(path = copy),
        )
        return try {
            kept += item
            write(kept)
            Favourite(item.key, item.artwork)
        } catch (error: Exception) {
            if (!alreadyOwned) copy.delete()
            throw error
        }
    }

    fun forget(key: String) {
        val kept = read()
        if (kept.none { it.key == key }) return
        write(kept.filterNot { it.key == key })
    }

    /** Deletes only unclaimed files this module owns, sparing the picture still represented as shown. */
    fun discardAllBut(keep: File?) {
        val claimed = read().map { normalized(it.artwork.path) }.toSet()
        directory.listFiles()?.forEach { file ->
            if (file.isFile && !file.name.startsWith(INDEX) && normalized(file) !in claimed && !sameFile(file, keep)) {
                file.delete()
            }
        }
    }

    private fun read(): List<Kept> {
        val text = try {
            index.openRead().bufferedReader().use { it.readText() }
        } catch (_: FileNotFoundException) {
            return emptyList()
        } catch (error: Exception) {
            throw IOException("Could not read favourites", error)
        }
        return try {
            val array = JSONArray(text)
            List(array.length()) { position ->
                val json = array.getJSONObject(position)
                val name = json.getString("file")
                if (File(name).name != name) throw IOException("Invalid favourite file name")
                Kept(
                    key = name,
                    origin = json.getString("origin"),
                    artwork = Artwork(
                        title = json.optString("title"),
                        byline = json.optString("byline"),
                        attribution = json.optString("attribution"),
                        detailsUrl = json.nullableString("detailsUrl"),
                        origin = json.nullableString("artworkOrigin"),
                        path = File(directory, name),
                    ),
                )
            }
        } catch (error: Exception) {
            throw IOException("The favourites index is damaged", error)
        }
    }

    private fun write(kept: List<Kept>) {
        directory.mkdirs()
        val array = JSONArray()
        kept.forEach { item ->
            array.put(
                JSONObject()
                    .put("file", item.key)
                    .put("origin", item.origin)
                    .put("title", item.artwork.title)
                    .put("byline", item.artwork.byline)
                    .put("attribution", item.artwork.attribution)
                    .put("detailsUrl", item.artwork.detailsUrl ?: JSONObject.NULL)
                    .put("artworkOrigin", item.artwork.origin ?: JSONObject.NULL),
            )
        }
        var output: java.io.FileOutputStream? = null
        try {
            output = index.startWrite()
            output.write(array.toString(2).toByteArray(Charsets.UTF_8))
            index.finishWrite(output)
        } catch (error: Exception) {
            output?.let(index::failWrite)
            throw IOException("Could not save favourites", error)
        }
    }

    private fun freeName(origin: File, kept: List<Kept>): File {
        val extension = origin.extension.takeIf { it.isNotEmpty() }?.let { ".$it" }.orEmpty()
        val stem = origin.nameWithoutExtension.ifEmpty { "painting" }
        var candidate = File(directory, "$stem$extension")
        var number = 2
        while (candidate.exists() || kept.any { sameFile(it.artwork.path, candidate) }) {
            candidate = File(directory, "$stem-$number$extension")
            number++
        }
        return candidate
    }

    private data class Kept(val key: String, val origin: String, val artwork: Artwork) {
        fun matches(other: Artwork): Boolean {
            return sameArtwork(artwork, other) || sameFile(File(origin), other.path)
        }
    }

    private companion object {
        const val DIRECTORY = "favourites"
        const val INDEX = "index.json"
    }
}

private fun JSONObject.nullableString(key: String): String? =
    if (isNull(key)) null else optString(key).takeIf { it.isNotEmpty() }

private fun normalized(file: File?): File? = file?.let { path ->
    runCatching { path.canonicalFile }.getOrElse { path.absoluteFile }
}

private fun sameFile(left: File?, right: File?): Boolean = normalized(left) == normalized(right)

internal fun sameArtwork(left: Artwork, right: Artwork): Boolean {
    val leftId = idOf(left.path)
    val rightId = idOf(right.path)
    return sameFile(left.path, right.path) || (leftId != null && leftId == rightId)
}
