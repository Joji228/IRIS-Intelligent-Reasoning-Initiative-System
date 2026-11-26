package dev.iris.app.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.animation.core.Animatable
import androidx.compose.runtime.LaunchedEffect
import kotlinx.coroutines.flow.collectLatest
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.link.LinkState

/** Clickable with a soft press dim instead of Material's ripple. */
fun Modifier.press(enabled: Boolean = true, role: Role = Role.Button, onClick: () -> Unit): Modifier = composed {
    val source = remember { MutableInteractionSource() }
    pressDim(source).clickable(interactionSource = source, indication = null, enabled = enabled, role = role, onClick = onClick)
}

/**
 * The press feedback every tappable shares: a plain dim while held, eased back on
 * release (no shrinking, no bounce). A quick tap always flashes it in full: inside a
 * scrolling list Compose reports the press only when the finger lifts, so a dim that
 * followed "is pressed" alone never showed for a tap there.
 */
fun Modifier.pressDim(source: MutableInteractionSource): Modifier = composed {
    val k = remember { Animatable(0f) }
    LaunchedEffect(source) {
        source.interactions.collectLatest { i ->
            when (i) {
                is PressInteraction.Press -> k.animateTo(1f, tween(70))
                is PressInteraction.Release, is PressInteraction.Cancel -> {
                    if (k.value < 1f) k.animateTo(1f, tween(60))
                    k.animateTo(0f, tween(220, easing = Motion.ease))
                }
            }
        }
    }
    graphicsLayer { alpha = 1f - 0.3f * k.value }
}

@Composable
fun Txt(
    text: String,
    size: TextUnit = 14.sp,
    color: Color = C.text,
    weight: FontWeight = FontWeight.Normal,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
    lineHeight: TextUnit = TextUnit.Unspecified,
    mono: Boolean = false,
    align: androidx.compose.ui.text.style.TextAlign? = null,
) {
    BasicText(
        text,
        modifier = modifier,
        style = style(size, color, weight, lineHeight, mono).let { if (align != null) it.copy(textAlign = align) else it },
        maxLines = maxLines,
        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
    )
}

/** The "Working…" text: a highlight sweeping across, like the web app's .shimmer. */
@Composable
fun Shimmer(text: String, size: TextUnit = 13.sp, weight: FontWeight = FontWeight.Medium, modifier: Modifier = Modifier) {
    val x by rememberInfiniteTransition("shimmer").animateFloat(
        0f, 1f, infiniteRepeatable(tween(2200, easing = LinearEasing)), "x",
    )
    BasicText(
        text,
        modifier = modifier,
        maxLines = 1,
        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
        style = TextStyle(
            fontSize = T.snap(size),
            fontWeight = weight,
            brush = Brush.linearGradient(
                0f to C.faint, 0.1f to C.text, 0.2f to C.faint, 1f to C.faint,
                start = androidx.compose.ui.geometry.Offset(-600f + 1200f * x, 0f),
                end = androidx.compose.ui.geometry.Offset(600f + 1200f * x, 0f),
            ),
        ),
    )
}

@Composable
fun Spinner(size: Dp = 16.dp, color: Color = C.accentHi) {
    val angle by rememberInfiniteTransition("spin").animateFloat(
        0f, 360f, infiniteRepeatable(tween(900, easing = LinearEasing)), "a",
    )
    Icon(Lucide.Loader, color, size, Modifier.rotate(angle))
}

@Composable
fun Button(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    primary: Boolean = true,
    enabled: Boolean = true,
    busy: Boolean = false,
    padding: PaddingValues = PaddingValues(horizontal = 18.dp, vertical = 12.dp),
) {
    val shape = RoundedCornerShape(Radius.md)
    Row(
        modifier
            .heightIn(min = 46.dp)
            .alpha(if (enabled) 1f else 0.5f)
            .then(if (primary) Modifier.background(C.accent, shape) else Modifier.border(1.dp, C.lineStrong, shape))
            .press(enabled && !busy, onClick = onClick)
            .padding(padding),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val fg = if (primary) C.onAccent else C.text
        if (busy) Spinner(16.dp, fg) else if (icon != null) Icon(icon, fg, 17.dp)
        // One line: a squeezed button must never break its label into letters.
        Txt(text, 14.5.sp, fg, FontWeight.Medium, maxLines = 1)
    }
}

/** A square outlined button with a red icon, for deleting (same height as [Button]). */
@Composable
fun DangerIconButton(icon: ImageVector, label: String, onClick: () -> Unit) {
    val shape = RoundedCornerShape(Radius.md)
    Box(
        Modifier.size(46.dp).border(1.dp, C.danger.copy(alpha = 0.35f), shape).clip(shape)
            .semantics { contentDescription = label }
            .press(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, C.danger, 18.dp) }
}

@Composable
fun IconButton(icon: ImageVector, onClick: () -> Unit, tint: Color = C.faint, size: Dp = 20.dp, modifier: Modifier = Modifier) {
    // 48 dp: Android's comfortable size for a fingertip, whatever the icon's size.
    Box(modifier.size(48.dp).clip(CircleShape).press(onClick = onClick), contentAlignment = Alignment.Center) {
        Icon(icon, tint, size)
    }
}

@Composable
fun Field(
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    password: Boolean = false,
    mono: Boolean = false,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
    singleLine: Boolean = true,
    shape: Shape = RoundedCornerShape(Radius.md),
) {
    BasicTextField(
        value = value,
        onValueChange = onChange,
        singleLine = singleLine,
        textStyle = style(15.5.sp, C.text, mono = mono),
        cursorBrush = SolidColor(C.accentHi),
        keyboardOptions = keyboard,
        visualTransformation = if (password) PasswordVisualTransformation() else VisualTransformation.None,
        modifier = modifier
            .fillMaxWidth()
            .background(C.bg.copy(alpha = 0.4f), shape)
            .border(1.dp, C.lineStrong, shape)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        decorationBox = { inner ->
            Box {
                if (value.isEmpty()) Txt(placeholder, 15.5.sp, C.faint, mono = mono)
                inner()
            }
        },
    )
}

@Composable
fun Label(text: String) = Txt(text, 13.sp, C.muted, FontWeight.Medium, Modifier.padding(bottom = 8.dp))

/** Shown at the top while the link is down (the web app's red bar). */
@Composable
fun ConnectionBar(state: LinkState, providerOnline: Boolean, onRetry: () -> Unit) {
    val (text, color) = when (state) {
        LinkState.Online -> if (providerOnline) return else "Can't reach the AI provider right now" to C.warn
        LinkState.Connecting -> "Connecting to your server…" to C.muted
        LinkState.ServerOffline -> "Your server is offline: check that it's on and Remote access is on" to C.warn
        LinkState.Offline -> "No connection to the relay · tap to retry" to C.danger
    }
    Row(
        Modifier.fillMaxWidth().background(color.copy(alpha = 0.12f)).press(onClick = onRetry)
            .padding(horizontal = 16.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (state == LinkState.Connecting) Spinner(13.dp, color)
        Txt(text, 12.5.sp, color, FontWeight.Medium)
    }
}

@Composable
fun Toast(text: String, modifier: Modifier = Modifier) {
    Box(
        modifier
            .background(C.elevated, RoundedCornerShape(50))
            .border(1.dp, C.lineStrong, RoundedCornerShape(50))
            .padding(horizontal = 18.dp, vertical = 12.dp),
    ) { Txt(text, 14.sp, C.text) }
}

/** A small round-edged chip (status lines, notes). */
@Composable
fun Pill(modifier: Modifier = Modifier, bg: Color = C.surface, content: @Composable () -> Unit) {
    Row(
        modifier.background(bg, RoundedCornerShape(50)).padding(horizontal = 12.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) { content() }
}
