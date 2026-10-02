package dev.artwindow

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * [picture] — a painting already as it hangs — placed in whatever box [modifier] gives
 * it, where [Screen.frame] puts it for [framing]. Only the drawing changes when the
 * framing does, never the pixels, which is what lets a gesture move it at the screen's
 * own frame rate; the renderer asks the same [Screen.frame] at wallpaper size, so what
 * is shown here is what gets hung.
 */
@Composable
fun FramedPicture(picture: Bitmap, base: Base, framing: Framing, modifier: Modifier = Modifier) {
    val image = remember(picture) { picture.asImageBitmap() }
    Canvas(modifier.clipToBounds()) {
        val box = Screen(size.width.roundToInt().coerceAtLeast(1), size.height.roundToInt().coerceAtLeast(1))
        val rect = box.frame(picture.width, picture.height, base, framing)
        drawImage(
            image,
            dstOffset = IntOffset(rect.left, rect.top),
            dstSize = IntSize(rect.width, rect.height),
            filterQuality = FilterQuality.Medium,
        )
    }
}

/**
 * Pinch to zoom and drag to move a painting of [paintingWidth] x [paintingHeight] framed
 * from [base], reporting each new [Framing] through [onFraming] as the fingers move. The
 * picture follows them exactly ([Screen.reframe]) and stops at its edges.
 *
 * A single finger is taken only if it moves the way the picture can: a mostly vertical
 * drag on a painting that can only move sideways is left unconsumed, so the column the
 * preview sits in still scrolls. Two fingers are always the picture's. Once the gesture
 * is claimed, everything it moves is consumed.
 */
fun Modifier.frameGestures(
    key: Any?,
    paintingWidth: Int,
    paintingHeight: Int,
    base: Base,
    framing: () -> Framing,
    onFraming: (Framing) -> Unit,
): Modifier = pointerInput(key, paintingWidth, paintingHeight, base) {
    val box = Screen(size.width.coerceAtLeast(1), size.height.coerceAtLeast(1))
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        // Kept here rather than read back from the preferences, which only catch up on the
        // next recomposition: two events in one frame would otherwise lose the first.
        var current = framing()
        var zoomSoFar = 1f
        var panSoFar = Offset.Zero
        var claimed = false
        do {
            val event = awaitPointerEvent()
            if (event.changes.any { it.isConsumed }) return@awaitEachGesture
            // The event that lifts the last finger has no fingers left to measure: its
            // centroid is unspecified, and framing about it wrote a NaN into the pan —
            // the picture sprang to its left edge on release and never moved again.
            if (event.changes.none { it.pressed }) break
            val zoomChange = event.calculateZoom()
            val panChange = event.calculatePan()
            val pinching = event.changes.count { it.pressed } > 1
            if (!claimed) {
                zoomSoFar *= zoomChange
                panSoFar += panChange
                val zoomMotion = abs(1f - zoomSoFar) * event.calculateCentroidSize(useCurrent = false)
                if (zoomMotion <= viewConfiguration.touchSlop && panSoFar.getDistance() <= viewConfiguration.touchSlop) {
                    continue
                }
                if (!pinching) {
                    val (acrossMoves, downMoves) = box.canMove(paintingWidth, paintingHeight, base, current)
                    val usable = if (abs(panSoFar.x) >= abs(panSoFar.y)) acrossMoves else downMoves
                    if (!usable) return@awaitEachGesture
                }
                claimed = true
            }
            val centroid = event.calculateCentroid()
            val (acrossMoves, downMoves) = box.canMove(paintingWidth, paintingHeight, base, current)
            // The first claimed event carries everything moved while deciding, so the
            // picture does not start a slop behind the finger.
            val scale = if (panSoFar != Offset.Zero || zoomSoFar != 1f) zoomSoFar else zoomChange
            val move = if (panSoFar != Offset.Zero) panSoFar else panChange
            zoomSoFar = 1f
            panSoFar = Offset.Zero
            current = box.reframe(
                paintingWidth, paintingHeight, base, current,
                scaleChange = scale, x = centroid.x, y = centroid.y,
                dx = if (acrossMoves || pinching) move.x else 0f,
                dy = if (downMoves || pinching) move.y else 0f,
            )
            onFraming(current)
            event.changes.forEach { if (it.positionChanged()) it.consume() }
        } while (event.changes.any { it.pressed })
    }
}
