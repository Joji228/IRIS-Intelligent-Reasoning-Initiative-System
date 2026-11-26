package dev.iris.app.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.min
import kotlin.math.sign
import kotlin.math.sqrt
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.REASONING_LEVELS
import kotlin.math.abs
import kotlin.math.roundToInt

/** A short line under each level's name (the desktop's EffortSlider.svelte uses the same words). */
private val EFFORT_HINTS = mapOf(
    "" to "The model decides",
    "off" to "Answers right away",
    "low" to "A quick think",
    "medium" to "Balanced",
    "high" to "Thinks it through",
    "max" to "Deepest and slowest",
)

/** The warm end of the fill: the higher the level, the pinker it glows. */
private val WARM = Color(0xFFF472B6)

/** Room at both ends of the track so the thumb stays inside it (half the thumb). */
private val INSET = 14.dp
/** Smooth deceleration with no overshoot (the desktop's cubic-bezier(0.22, 1, 0.36, 1)). */
private val GLIDE = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)
/** The settle spring: critically damped (no overshoot), about a third of a second. */
private const val SETTLE_STIFFNESS = 260f
private val SETTLE = spring<Float>(dampingRatio = 1f, stiffness = SETTLE_STIFFNESS)
/** Following the finger: stiff enough to keep up, smooth enough not to jolt. */
private val FOLLOW = spring<Float>(dampingRatio = 1f, stiffness = 2200f)

/**
 * How hard an agent thinks, as a slider over [REASONING_LEVELS] (Off … Max) with an "Auto" pill.
 * `value` "" is Auto (shown dimmed at [autoLevel], or Medium when that isn't known). Taps pick
 * the nearest level; a drag follows the finger, ticks at each level and picks on release, so
 * [onChange] runs once per gesture and only when the level changes.
 */
@Composable
fun EffortSlider(
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    autoLabel: String = "Auto",
    autoLevel: String = "",
    autoHint: String = "The model decides",
) {
    val ids = REASONING_LEVELS.map { it.first }
    val last = ids.size - 1
    // What was just picked, shown at once while the change travels to the server; it gives
    // way when the server's answer arrives (or after a while, if it never does). The gesture
    // code below lives as long as the slider, so it reads the latest value and callback.
    var local by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(value) { if (local == value) local = null }
    LaunchedEffect(local) {
        if (local != null) {
            delay(10_000)
            local = null
        }
    }
    val latestValue by rememberUpdatedState(value)
    val latestOnChange by rememberUpdatedState(onChange)
    val current = local ?: value
    val auto = current.isEmpty()
    val restIndex = ids.indexOf(if (auto) autoLevel.ifEmpty { "medium" } else current).let { if (it < 0) 2 else it }

    var dragging by remember { mutableStateOf(false) }
    var widthPx by remember { mutableFloatStateOf(0f) }
    val haptic = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()

    // The thumb's position in levels (fractional). While dragging it follows the finger
    // closely; on release a critically damped spring carries it on with the finger's speed
    // and settles on a level without overshooting (as on the desktop).
    val anim = remember { Animatable(restIndex.toFloat()) }
    LaunchedEffect(restIndex) { if (!dragging) anim.animateTo(restIndex.toFloat(), SETTLE) }
    val pos = anim.value
    val dim by animateFloatAsState(if (auto && !dragging) 0.4f else 1f, tween(220), label = "effort-dim")
    val thumbScale by animateFloatAsState(if (dragging) 1.12f else 1f, tween(180, easing = GLIDE), label = "effort-scale")
    val heat = (pos / last).coerceIn(0f, 1f)

    val activeIndex = if (dragging) pos.roundToInt().coerceIn(0, last) else restIndex
    val showsAuto = auto && !dragging
    val name = if (showsAuto) autoLabel else REASONING_LEVELS[activeIndex].second
    val hint = if (showsAuto) autoHint else EFFORT_HINTS[ids[activeIndex]].orEmpty()

    fun pick(i: Int) {
        val id = ids[i]
        local = id
        if (id != latestValue) latestOnChange(id)
    }

    Column(modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(Lucide.Brain, C.accentHi, 14.dp)
            Txt(name, 14.5.sp, C.text, FontWeight.Medium)
            Txt(hint, 12.5.sp, C.faint, modifier = Modifier.weight(1f).padding(start = 2.dp), maxLines = 1)
            val pill = RoundedCornerShape(50)
            Box(
                Modifier.clip(pill)
                    .background(if (auto) C.accentSoft else Color.Transparent)
                    .border(1.dp, if (auto) C.accent.copy(alpha = 0.5f) else C.lineStrong, pill)
                    .press {
                        local = ""
                        if (latestValue.isNotEmpty()) latestOnChange("")
                    }
                    .padding(horizontal = 11.dp, vertical = 4.dp),
            ) {
                Txt(autoLabel, 12.sp, if (auto) C.accentHi else C.muted, FontWeight.Medium)
            }
        }

        // The track: rail, glowing fill up to the thumb, a dot per level, and the thumb.
        Box(
            Modifier.padding(top = 8.dp).fillMaxWidth().height(36.dp)
                .onSizeChanged { widthPx = it.width.toFloat() }
                .semantics {
                    contentDescription = "Thinking effort: $name"
                    progressBarRangeInfo = ProgressBarRangeInfo(activeIndex.toFloat(), 0f..last.toFloat(), steps = last - 1)
                    setProgress { v ->
                        pick(v.roundToInt().coerceIn(0, last))
                        true
                    }
                }
                .pointerInput(last) {
                    val inset = INSET.toPx()
                    fun at(x: Float): Float {
                        val usable = (widthPx - 2 * inset).coerceAtLeast(1f)
                        return ((x - inset) / usable * last).coerceIn(0f, last.toFloat())
                    }
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        var x = down.position.x
                        var moved = false
                        var cancelled = false
                        var stop = at(x).roundToInt()
                        val tracker = VelocityTracker()
                        tracker.addPosition(down.uptimeMillis, down.position)
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) break
                            // Someone else (the sheet's scroll) took the gesture before it became a drag.
                            if (!moved && change.isConsumed) { cancelled = true; break }
                            tracker.addPosition(change.uptimeMillis, change.position)
                            if (!moved && abs(change.position.x - down.position.x) > viewConfiguration.touchSlop) {
                                moved = true
                                dragging = true
                            }
                            if (moved) {
                                x = change.position.x
                                val to = at(x)
                                // A very stiff spring: it keeps up with the finger but never jolts.
                                scope.launch { anim.animateTo(to, FOLLOW) }
                                val s = to.roundToInt()
                                if (s != stop) {
                                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                    stop = s
                                }
                                change.consume()
                            }
                        }
                        dragging = false
                        if (cancelled) return@awaitEachGesture
                        if (moved) {
                            // The release speed, in levels per second.
                            val usable = (widthPx - 2 * inset).coerceAtLeast(1f)
                            val vel = (tracker.calculateVelocity().x / usable * last).coerceIn(-14f, 14f)
                            val here = at(x)
                            val i = (here + vel * 0.12f).roundToInt().coerceIn(0, last)
                            // Only speed towards the level is kept, never enough to overshoot.
                            val d = i - here
                            val toward = if (vel * d > 0) sign(vel) * min(abs(vel), 0.9f * sqrt(SETTLE_STIFFNESS) * abs(d)) else 0f
                            scope.launch { anim.animateTo(i.toFloat(), SETTLE, initialVelocity = toward) }
                            pick(i)
                        } else {
                            // A tap: glide to the level nearest the tap.
                            val i = at(x).roundToInt().coerceIn(0, last)
                            scope.launch { anim.animateTo(i.toFloat(), SETTLE) }
                            pick(i)
                        }
                    }
                }
                .drawBehind {
                    val inset = INSET.toPx()
                    val cy = size.height / 2
                    val usable = size.width - 2 * inset
                    val tx = inset + usable * pos / last
                    // A 10dp bar; the fill reaches under the thumb's middle, with a soft highlight on top.
                    val h = 10.dp.toPx()
                    val r = CornerRadius(h / 2)
                    val hot = lerp(C.accent, WARM, heat * 0.85f)
                    // The bar reaches half its height past the end stops, so their dots sit inside it.
                    val start = inset - h / 2
                    drawRoundRect(C.lineStrong, Offset(start, cy - h / 2), Size(usable + h, h), r)
                    val fillEnd = maxOf(tx, start + h)
                    drawRoundRect(
                        Brush.horizontalGradient(listOf(C.accent, hot), startX = start, endX = fillEnd),
                        Offset(start, cy - h / 2), Size(fillEnd - start, h), r, alpha = dim,
                    )
                    drawRoundRect(
                        Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.18f), Color.Transparent), startY = cy - h / 2, endY = cy),
                        Offset(start, cy - h / 2), Size(fillEnd - start, h), r, alpha = dim,
                    )
                    for (i in 0..last) {
                        val dx = inset + usable * i / last
                        val on = i <= pos + 0.01f
                        drawCircle(
                            if (on) Color.White.copy(alpha = 0.7f * dim) else C.faint.copy(alpha = 0.6f),
                            radius = 2.dp.toPx(), center = Offset(dx, cy),
                        )
                    }
                    // The thumb: a soft glow that grows with the level, a shadow, a white disc
                    // with a thin tinted ring, and a tinted core.
                    val t = thumbScale
                    val center = Offset(tx, cy)
                    drawCircle(hot.copy(alpha = (0.12f + 0.25f * heat) * dim), radius = (15.dp.toPx() + 6.dp.toPx() * heat) * t, center = center)
                    drawCircle(Color.Black.copy(alpha = 0.28f * dim), radius = 13.6.dp.toPx() * t, center = center + Offset(0f, 1.dp.toPx()))
                    drawCircle(hot.copy(alpha = 0.8f * dim), radius = 13.2.dp.toPx() * t, center = center)
                    drawCircle(Color.White.copy(alpha = 0.55f + 0.45f * dim), radius = 12.dp.toPx() * t, center = center)
                    drawCircle(hot.copy(alpha = dim), radius = 4.dp.toPx() * (if (dragging) 1.25f else 1f) * t, center = center)
                },
        )

        // A label under each level, centred on its dot.
        Layout(
            content = {
                REASONING_LEVELS.forEachIndexed { i, (_, label) ->
                    val on = !showsAuto && i == activeIndex
                    // Just a colour change, no bounce.
                    val color by animateColorAsState(if (on) C.accentHi else C.faint, tween(200), label = "effort-label")
                    Box(
                        Modifier.clip(RoundedCornerShape(Radius.sm))
                            .press { pick(i) }
                            .padding(horizontal = 4.dp, vertical = 2.dp),
                    ) {
                        Txt(label, 12.sp, color, if (on) FontWeight.SemiBold else FontWeight.Normal)
                    }
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { measurables, constraints ->
            val placeables = measurables.map { it.measure(Constraints()) }
            val w = constraints.maxWidth
            val inset = INSET.roundToPx()
            val usable = w - 2 * inset
            val h = placeables.maxOf { it.height }
            layout(w, h) {
                placeables.forEachIndexed { i, p ->
                    val cx = inset + usable * i / last
                    p.place((cx - p.width / 2).coerceIn(0, (w - p.width).coerceAtLeast(0)), 0)
                }
            }
        }
    }
}
