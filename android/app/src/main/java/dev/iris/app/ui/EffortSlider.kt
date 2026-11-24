package dev.iris.app.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
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
private val INSET = 11.dp

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
    // What was just picked, shown at once while the change travels to the server.
    var local by remember(value) { mutableStateOf<String?>(null) }
    val current = local ?: value
    val auto = current.isEmpty()
    val restIndex = ids.indexOf(if (auto) autoLevel.ifEmpty { "medium" } else current).let { if (it < 0) 2 else it }

    var dragging by remember { mutableStateOf(false) }
    var dragPos by remember { mutableFloatStateOf(0f) }
    var widthPx by remember { mutableFloatStateOf(0f) }
    val haptic = LocalHapticFeedback.current

    val pos by animateFloatAsState(
        if (dragging) dragPos else restIndex.toFloat(),
        if (dragging) snap() else spring(dampingRatio = 0.6f, stiffness = 400f),
        label = "effort-thumb",
    )
    val dim by animateFloatAsState(if (auto && !dragging) 0.35f else 1f, tween(250), label = "effort-dim")
    val thumbScale by animateFloatAsState(if (dragging) 1.15f else 1f, spring(dampingRatio = 0.5f, stiffness = 500f), label = "effort-scale")
    val heat = (pos / last).coerceIn(0f, 1f)

    val activeIndex = if (dragging) dragPos.roundToInt().coerceIn(0, last) else restIndex
    val showsAuto = auto && !dragging
    val name = if (showsAuto) autoLabel else REASONING_LEVELS[activeIndex].second
    val hint = if (showsAuto) autoHint else EFFORT_HINTS[ids[activeIndex]].orEmpty()

    fun pick(i: Int) {
        val id = ids[i]
        local = id
        if (id != value) onChange(id)
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
                        if (value.isNotEmpty()) onChange("")
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
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) break
                            // Someone else (the sheet's scroll) took the gesture before it became a drag.
                            if (!moved && change.isConsumed) { cancelled = true; break }
                            if (!moved && abs(change.position.x - down.position.x) > viewConfiguration.touchSlop) {
                                moved = true
                                dragging = true
                                dragPos = at(x)
                            }
                            if (moved) {
                                x = change.position.x
                                dragPos = at(x)
                                val s = dragPos.roundToInt()
                                if (s != stop) {
                                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                    stop = s
                                }
                                change.consume()
                            }
                        }
                        val i = at(x).roundToInt().coerceIn(0, last)
                        if (moved) dragPos = i.toFloat()
                        dragging = false
                        if (!cancelled) pick(i)
                    }
                }
                .drawBehind {
                    val inset = INSET.toPx()
                    val cy = size.height / 2
                    val usable = size.width - 2 * inset
                    val tx = inset + usable * pos / last
                    val h = 6.dp.toPx()
                    val r = CornerRadius(h / 2)
                    val hot = lerp(C.accent, WARM, heat)
                    drawRoundRect(C.lineStrong, Offset(inset, cy - h / 2), Size(usable, h), r)
                    if (tx > inset) {
                        drawRoundRect(
                            Brush.horizontalGradient(listOf(C.accent, hot), startX = inset, endX = tx),
                            Offset(inset, cy - h / 2), Size(tx - inset, h), r, alpha = dim,
                        )
                    }
                    for (i in 0..last) {
                        val dx = inset + usable * i / last
                        val on = i <= pos + 0.01f
                        drawCircle(
                            if (on) Color.White.copy(alpha = 0.85f * dim) else C.faint.copy(alpha = 0.6f),
                            radius = 3.dp.toPx(), center = Offset(dx, cy),
                        )
                    }
                    drawCircle(
                        hot.copy(alpha = (0.15f + 0.3f * heat) * dim),
                        radius = (14.dp.toPx() + 8.dp.toPx() * heat) * thumbScale, center = Offset(tx, cy),
                    )
                    drawCircle(hot.copy(alpha = dim), radius = 11.dp.toPx() * thumbScale, center = Offset(tx, cy))
                    drawCircle(Color(0xFFF7F5FF).copy(alpha = 0.35f + 0.65f * dim), radius = 9.dp.toPx() * thumbScale, center = Offset(tx, cy))
                },
        )

        // A label under each level, centred on its dot.
        Layout(
            content = {
                REASONING_LEVELS.forEachIndexed { i, (_, label) ->
                    val on = !showsAuto && i == activeIndex
                    val scale by animateFloatAsState(if (on) 1.08f else 1f, spring(dampingRatio = 0.55f, stiffness = 500f), label = "effort-label")
                    Box(
                        Modifier.graphicsLayer { scaleX = scale; scaleY = scale }
                            .clip(RoundedCornerShape(6.dp))
                            .press { pick(i) }
                            .padding(horizontal = 4.dp, vertical = 2.dp),
                    ) {
                        Txt(label, 12.sp, if (on) C.accentHi else C.faint, if (on) FontWeight.Medium else FontWeight.Normal)
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
