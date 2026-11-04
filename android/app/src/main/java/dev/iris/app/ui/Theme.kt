package dev.iris.app.ui

import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.foundation.text.selection.LocalTextSelectionColors

/** The desktop app's dark theme (app/src/app.css), one to one. */
object C {
    val bg = Color(0xFF07070B)
    val rail = Color(0xFF0B0B11)
    val panel = Color(0xFF0F0F16)
    val surface = Color(0xFF15151D)
    val elevated = Color(0xFF1B1B26)
    val hover = Color(0xFF21212D)
    // Chat bubbles: the agent's, and the user's own (violet).
    val bubble = Color(0xFF15151E)
    val bubbleLine = Color.White.copy(alpha = 0.05f)
    val bubbleMe = Color(0xFF6B46E5)
    val line = Color.White.copy(alpha = 0.07f)
    val lineStrong = Color.White.copy(alpha = 0.12f)

    val text = Color(0xFFECECF1)
    val muted = Color(0xFFA3A3B8)
    val faint = Color(0xFF6C6C82)

    val accent = Color(0xFF8B5CF6)
    val accentHi = Color(0xFFA78BFA)
    val accentSoft = Color(0xFF8B5CF6).copy(alpha = 0.14f)
    val onAccent = Color.White

    val danger = Color(0xFFF87171)
    val dangerSoft = Color(0xFFF87171).copy(alpha = 0.12f)
    val success = Color(0xFF34D399)
    val warn = Color(0xFFFBBF24)
    val code = Color(0xFF0D0D14)

    val rainbow = listOf(Color(0xFFF472B6), Color(0xFFA78BFA), Color(0xFF60A5FA), Color(0xFF34D399), Color(0xFFFBBF24))
    val rainbowBrush get() = Brush.linearGradient(rainbow)
}

fun style(
    size: TextUnit = 14.sp,
    color: Color = C.text,
    weight: FontWeight = FontWeight.Normal,
    lineHeight: TextUnit = TextUnit.Unspecified,
    mono: Boolean = false,
) = TextStyle(
    color = color,
    fontSize = size,
    fontWeight = weight,
    lineHeight = lineHeight,
    fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default,
)

@Composable
fun IrisTheme(content: @Composable () -> Unit) {
    // Text reads 10% larger than the phone's setting: sizes were tuned on a desktop, and a
    // narrow phone (360 dp) needs them a step up to stay easy to read.
    val density = LocalDensity.current
    CompositionLocalProvider(
        LocalDensity provides Density(density.density, density.fontScale * 1.1f),
        LocalTextSelectionColors provides TextSelectionColors(handleColor = C.accent, backgroundColor = C.accent.copy(alpha = 0.35f)),
        content = content,
    )
}

/** One feel for every motion: quick ease-outs, soft springs without wobble. */
object Motion {
    val ease = CubicBezierEasing(0.2f, 0f, 0f, 1f)
    fun <T> quick() = tween<T>(180, easing = ease)
    fun <T> standard() = tween<T>(260, easing = ease)
    fun <T> soft() = spring<T>(dampingRatio = 0.9f, stiffness = 520f)
}

/** Parses "#rrggbb" (agent colors). */
fun parseColor(hex: String, fallback: Color = Color(0xFFC4B5FD)): Color = runCatching {
    val h = hex.trim().removePrefix("#")
    when (h.length) {
        6 -> Color(0xFF000000 or h.toLong(16))
        8 -> Color(h.toLong(16))
        else -> fallback
    }
}.getOrDefault(fallback)

/** CSS color-mix(in srgb, a p%, b). */
fun mix(a: Color, p: Float, b: Color) = Color(
    red = a.red * p + b.red * (1 - p),
    green = a.green * p + b.green * (1 - p),
    blue = a.blue * p + b.blue * (1 - p),
    alpha = 1f,
)
