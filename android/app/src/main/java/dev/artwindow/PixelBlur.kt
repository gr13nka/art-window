package dev.artwindow

/** Three box passes approximate a Gaussian blur without an API-31-only RenderEffect. */
internal fun blurPixels(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
    if (radius <= 0 || width <= 0 || height <= 0) return source.copyOf()
    var pixels = source.copyOf()
    repeat(3) {
        pixels = horizontalBlur(pixels, width, height, radius)
        pixels = verticalBlur(pixels, width, height, radius)
    }
    return pixels
}

private fun horizontalBlur(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
    val output = IntArray(source.size)
    for (y in 0 until height) {
        var a = 0L
        var r = 0L
        var g = 0L
        var b = 0L
        for (x in -radius..radius) {
            val color = source[y * width + x.coerceIn(0, width - 1)]
            a += color ushr 24 and 0xff
            r += color ushr 16 and 0xff
            g += color ushr 8 and 0xff
            b += color and 0xff
        }
        val divisor = radius * 2 + 1
        for (x in 0 until width) {
            output[y * width + x] = pack(a / divisor, r / divisor, g / divisor, b / divisor)
            val leaving = source[y * width + (x - radius).coerceIn(0, width - 1)]
            val entering = source[y * width + (x + radius + 1).coerceIn(0, width - 1)]
            a += (entering ushr 24 and 0xff) - (leaving ushr 24 and 0xff)
            r += (entering ushr 16 and 0xff) - (leaving ushr 16 and 0xff)
            g += (entering ushr 8 and 0xff) - (leaving ushr 8 and 0xff)
            b += (entering and 0xff) - (leaving and 0xff)
        }
    }
    return output
}

private fun verticalBlur(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
    val output = IntArray(source.size)
    for (x in 0 until width) {
        var a = 0L
        var r = 0L
        var g = 0L
        var b = 0L
        for (y in -radius..radius) {
            val color = source[y.coerceIn(0, height - 1) * width + x]
            a += color ushr 24 and 0xff
            r += color ushr 16 and 0xff
            g += color ushr 8 and 0xff
            b += color and 0xff
        }
        val divisor = radius * 2 + 1
        for (y in 0 until height) {
            output[y * width + x] = pack(a / divisor, r / divisor, g / divisor, b / divisor)
            val leaving = source[(y - radius).coerceIn(0, height - 1) * width + x]
            val entering = source[(y + radius + 1).coerceIn(0, height - 1) * width + x]
            a += (entering ushr 24 and 0xff) - (leaving ushr 24 and 0xff)
            r += (entering ushr 16 and 0xff) - (leaving ushr 16 and 0xff)
            g += (entering ushr 8 and 0xff) - (leaving ushr 8 and 0xff)
            b += (entering and 0xff) - (leaving and 0xff)
        }
    }
    return output
}

private fun pack(a: Long, r: Long, g: Long, b: Long): Int =
    (a.toInt() shl 24) or (r.toInt() shl 16) or (g.toInt() shl 8) or b.toInt()
