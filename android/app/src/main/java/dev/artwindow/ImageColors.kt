package dev.artwindow

import kotlin.math.ceil

/** A uniform colour matched to the outside five percent of an image. */
internal fun edgeAverageColor(pixels: IntArray, width: Int, height: Int): Int {
    if (width <= 0 || height <= 0 || pixels.size < width * height) return 0xff000000.toInt()
    val band = ceil(minOf(width, height) * 0.05).toInt().coerceAtLeast(1)
    var red = 0L
    var green = 0L
    var blue = 0L
    var count = 0L
    for (y in 0 until height) {
        for (x in 0 until width) {
            if (x < band || x >= width - band || y < band || y >= height - band) {
                val color = pixels[y * width + x]
                red += color ushr 16 and 0xff
                green += color ushr 8 and 0xff
                blue += color and 0xff
                count++
            }
        }
    }
    return opaque((red / count).toInt(), (green / count).toInt(), (blue / count).toInt())
}

/** The most frequent, visibly distinct colours in a small sampled bitmap. */
internal fun commonImageColors(pixels: IntArray, limit: Int = 5): List<Int> {
    data class Bucket(var count: Int = 0, var red: Long = 0, var green: Long = 0, var blue: Long = 0)

    val buckets = HashMap<Int, Bucket>()
    for (color in pixels) {
        val r = color ushr 16 and 0xff
        val g = color ushr 8 and 0xff
        val b = color and 0xff
        val key = (r shr 4 shl 8) or (g shr 4 shl 4) or (b shr 4)
        buckets.getOrPut(key) { Bucket() }.apply {
            count++
            red += r
            green += g
            blue += b
        }
    }

    val result = ArrayList<Int>(limit)
    for (bucket in buckets.values.sortedByDescending { it.count }) {
        val candidate = opaque(
            (bucket.red / bucket.count).toInt(),
            (bucket.green / bucket.count).toInt(),
            (bucket.blue / bucket.count).toInt(),
        )
        if (result.any { colorDistanceSquared(it, candidate) < MIN_COLOR_DISTANCE_SQUARED }) continue
        result += candidate
        if (result.size == limit) break
    }
    return result
}

private fun colorDistanceSquared(a: Int, b: Int): Int {
    val dr = (a ushr 16 and 0xff) - (b ushr 16 and 0xff)
    val dg = (a ushr 8 and 0xff) - (b ushr 8 and 0xff)
    val db = (a and 0xff) - (b and 0xff)
    return dr * dr + dg * dg + db * db
}

private fun opaque(red: Int, green: Int, blue: Int): Int =
    (0xff shl 24) or (red shl 16) or (green shl 8) or blue

private const val MIN_COLOR_DISTANCE_SQUARED = 42 * 42
