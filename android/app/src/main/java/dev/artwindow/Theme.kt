package dev.artwindow

import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/**
 * The one place a colour of the app's own is chosen. Screens name a role — `primary` for
 * the accent, [muted], [wash], [hairline], [raised] — and never a colour, so the whole
 * app can be restyled here and nowhere else. It is neutral on purpose: no hue anywhere
 * except the paintings, the same roles the desktop window draws from the system's own
 * colours (`Tone` in `src/gallery/macos.rs`: ink, muted, accent, wash, line). Colours that
 * are *data* — the border picker's swatches, anything computed from a painting — are not
 * chrome and stay where they are made.
 */
private val Background = Color(0xFF1C1C1E)
private val Surface = Color(0xFF2A2A2C)
private val Raised = Color(0xFF323234)
private val Ink = Color(0xFFF2F2F2)
private val Accent = Color(0xFF6E6E73)
private val Tonal = Color(0xFF3A3A3C)
private val Error = Color(0xFFFF6B6B)

/** [Background] as an ARGB int, for the system bars, which are set outside Compose. */
internal const val BACKGROUND_ARGB = 0xFF1C1C1E.toInt()

// Every role is set, so that no Material default — a lavender in particular — can leak in
// through a component nobody remembered to style.
private val ArtWindowColors = darkColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    primaryContainer = Accent,
    onPrimaryContainer = Color.White,
    inversePrimary = Accent,
    secondary = Tonal,
    onSecondary = Ink,
    secondaryContainer = Tonal,
    onSecondaryContainer = Ink,
    tertiary = Tonal,
    onTertiary = Ink,
    tertiaryContainer = Tonal,
    onTertiaryContainer = Ink,
    background = Background,
    onBackground = Ink,
    surface = Surface,
    onSurface = Ink,
    surfaceVariant = Raised,
    onSurfaceVariant = Ink.copy(alpha = 0.6f),
    // Elevated surfaces are tinted with this; the surface's own colour tints nothing.
    surfaceTint = Surface,
    inverseSurface = Ink,
    inverseOnSurface = Background,
    outline = Color.White.copy(alpha = 0.12f),
    outlineVariant = Color.White.copy(alpha = 0.12f),
    scrim = Color.Black,
    error = Error,
    onError = Color.Black,
    errorContainer = Error,
    onErrorContainer = Color.Black,
    surfaceBright = Raised,
    surfaceDim = Background,
    surfaceContainerLowest = Background,
    surfaceContainerLow = Surface,
    surfaceContainer = Surface,
    surfaceContainerHigh = Raised,
    surfaceContainerHighest = Raised,
)

@Composable
fun ArtWindowTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = ArtWindowColors, content = content)
}

/** Secondary text: the ink, quieter. */
val ColorScheme.muted: Color get() = onSurface.copy(alpha = 0.6f)

/** The fill of an unselected chip or row: a veil over whatever is behind it. */
val ColorScheme.wash: Color get() = Color.White.copy(alpha = 0.06f)

/** The hairline around a control and between things. */
val ColorScheme.hairline: Color get() = outline

/** A surface standing above the others, such as the navigation pill. */
val ColorScheme.raised: Color get() = surfaceContainerHigh

/** What sits on a painting — a button over it, a caption's backing — which must read on any picture. */
val ColorScheme.overArt: Color get() = scrim.copy(alpha = 0.6f)

/** A text button reads as ink, not as the accent: the accent is too dim to read as text on the background. */
@Composable
fun quietButtonColors(): ButtonColors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface)
