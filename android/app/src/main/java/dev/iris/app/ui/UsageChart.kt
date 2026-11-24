package dev.iris.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.UsageDaily
import dev.iris.app.data.UsageDay
import dev.iris.app.data.formatTokens
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlin.math.max

private val MONEY = Color(0xFFF472B6)

fun money(v: Double): String = when {
    v >= 1 -> "$" + String.format("%.2f", v)
    v >= 0.01 -> "$" + String.format("%.3f", v)
    v > 0 -> "$" + String.format("%.4f", v)
    else -> "$0"
}

private fun dayLabel(d: String, long: Boolean): String = runCatching {
    val date = LocalDate.parse(d)
    if (long) date.format(DateTimeFormatter.ofPattern("EEE d MMM")) else date.format(DateTimeFormatter.ofPattern("d MMM"))
}.getOrDefault(d)

/**
 * Settings → Usage: tokens and money per day (the desktop's UsageChart). `mode` is
 * "tokens", "money" or "both" (bars for tokens, a line for money). Tap or drag over
 * the chart for a day's breakdown.
 */
@Composable
fun UsageChart(data: UsageDaily, mode: String) {
    val days = data.days
    var picked by remember(days) { mutableStateOf(days.lastIndex) }
    val pick = days.getOrNull(picked)
    val maxTokens = max(1L, days.maxOfOrNull { it.tokens } ?: 1L).toFloat()
    val maxCost = max(0.0001, days.maxOfOrNull { it.cost } ?: 0.0001).toFloat()
    val accent = C.accent
    val accentHi = C.accentHi
    val axis = C.lineStrong

    fun sum(list: List<UsageDay>) = Triple(list.sumOf { it.tokens }, list.sumOf { it.cost }, list.any { it.estimated })

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // Totals: today, last 7 days, the whole range.
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((label, list) in listOf("Today" to days.takeLast(1), "7 days" to days.takeLast(7), "${days.size} days" to days)) {
                val (t, c, est) = sum(list)
                Column(
                    Modifier.weight(1f).background(C.surface, RoundedCornerShape(14.dp)).border(1.dp, C.line, RoundedCornerShape(14.dp))
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                ) {
                    Txt(label, 11.5.sp, C.faint)
                    if (mode != "money") Txt(formatTokens(t), 16.sp, C.text, FontWeight.SemiBold)
                    if (mode != "tokens") {
                        Txt((if (est) "≈ " else "") + money(c), if (mode == "money") 16.sp else 12.5.sp, if (mode == "money") C.text else C.muted,
                            if (mode == "money") FontWeight.SemiBold else FontWeight.Normal)
                    }
                }
            }
        }
        // The picked day.
        if (pick != null) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Txt(dayLabel(pick.day, true), 13.sp, C.text, FontWeight.Medium)
                val parts = buildList {
                    if (mode != "money") add("${formatTokens(pick.tokens)} tokens")
                    if (mode != "tokens") add((if (pick.estimated) "≈ " else "") + money(pick.cost))
                }
                Txt(parts.joinToString(" · "), 12.5.sp, C.muted)
            }
            if (pick.tokens > 0) {
                // Older replies didn't record how much input came from the cache.
                Txt(
                    if (pick.estimated) "${formatTokens(pick.input)} input · ${formatTokens(pick.output)} output · cache split not recorded"
                    else "${formatTokens(pick.cached)} cached · ${formatTokens(max(0L, pick.input - pick.cached))} new input · ${formatTokens(pick.output)} output",
                    11.5.sp, C.faint,
                )
            }
        }
        // The chart.
        Box(Modifier.fillMaxWidth().height(150.dp)) {
            Canvas(
                Modifier.fillMaxWidth().height(150.dp)
                    .pointerInput(days) {
                        detectTapGestures { o -> picked = (o.x / size.width * days.size).toInt().coerceIn(0, days.lastIndex) }
                    }
                    .pointerInput(days) {
                        detectDragGestures { change, _ -> picked = (change.position.x / size.width * days.size).toInt().coerceIn(0, days.lastIndex) }
                    },
            ) {
                if (days.isEmpty()) return@Canvas
                val bottom = size.height - 18.dp.toPx()
                val slot = size.width / days.size
                val barW = (slot * 0.62f).coerceIn(2.dp.toPx(), 18.dp.toPx())
                drawLine(axis, Offset(0f, bottom), Offset(size.width, bottom), 1.dp.toPx())
                days.forEachIndexed { i, d ->
                    val v = if (mode == "money") (d.cost.toFloat() / maxCost) else (d.tokens / maxTokens)
                    val h = (bottom * v).let { if (v > 0) max(it, 1.5.dp.toPx()) else 0f }
                    val on = i == picked
                    val color = when {
                        mode == "money" && on -> MONEY
                        mode == "money" -> MONEY.copy(alpha = 0.55f)
                        on -> accentHi
                        else -> accent.copy(alpha = 0.55f)
                    }
                    drawRoundRect(color, Offset(i * slot + (slot - barW) / 2, bottom - h), Size(barW, h), CornerRadius(minOf(3.dp.toPx(), barW / 2)))
                }
                if (mode == "both") {
                    val path = Path()
                    days.forEachIndexed { i, d ->
                        val x = i * slot + slot / 2
                        val y = bottom * (1 - d.cost.toFloat() / maxCost)
                        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
                    }
                    drawPath(path, MONEY, style = Stroke(width = 2.dp.toPx()))
                    days.getOrNull(picked)?.let { d ->
                        drawCircle(MONEY, 3.5.dp.toPx(), Offset(picked * slot + slot / 2, bottom * (1 - d.cost.toFloat() / maxCost)))
                    }
                }
            }
            // A few day labels under the bars, placed by position (the end ones hug the edges).
            val every = when { days.size > 45 -> 14; days.size > 20 -> 7; days.size > 10 -> 3; else -> 1 }
            val shown = days.indices.filter { it % every == days.lastIndex % every }
            Layout(
                content = { shown.forEach { i -> Txt(dayLabel(days[i].day, false), 9.5.sp, C.faint, maxLines = 1) } },
                modifier = Modifier.fillMaxWidth().align(Alignment.BottomStart),
            ) { measurables, constraints ->
                val placeables = measurables.map { it.measure(Constraints()) }
                val w = constraints.maxWidth
                val slot = w.toFloat() / days.size
                val h = placeables.maxOfOrNull { it.height } ?: 0
                layout(w, h) {
                    placeables.forEachIndexed { k, p ->
                        val cx = (shown[k] * slot + slot / 2).toInt()
                        p.place((cx - p.width / 2).coerceIn(0, (w - p.width).coerceAtLeast(0)), 0)
                    }
                }
            }
        }
        if (mode == "both") {
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(10.dp).background(accent.copy(alpha = 0.55f), RoundedCornerShape(2.dp)))
                    Txt("Tokens", 11.5.sp, C.faint)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(width = 14.dp, height = 2.dp).background(MONEY, RoundedCornerShape(1.dp)))
                    Txt("Money", 11.5.sp, C.faint)
                }
            }
        }
        val p = data.prices
        Hint(
            "${(data.cacheRate * 100).toInt()}% of input came from DeepSeek's cache, which costs a fraction of new input. " +
                (if (p != null) "Prices for ${p.model}: \$${p.cached} cached / \$${p.input} new input / \$${p.output} output per 1M tokens at peak hours (01–04 and 06–10 UTC on weekdays), half the rest of the time. " else "") +
                "Amounts marked ≈ include days from before the cache split was recorded. Your DeepSeek balance is the final word.",
        )
    }
}
