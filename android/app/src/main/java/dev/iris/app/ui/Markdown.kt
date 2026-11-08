package dev.iris.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.coerceIn
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import kotlinx.coroutines.delay
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Markdown blocks, as much of GitHub's flavor as chat replies use. */
private sealed interface Block {
    data class Heading(val level: Int, val text: String) : Block
    data class Para(val text: String) : Block
    data class Code(val lang: String, val text: String) : Block
    data class Item(val depth: Int, val marker: String, val text: String) : Block
    data class Quote(val text: String) : Block
    data class Table(val rows: List<List<String>>) : Block
    data object Rule : Block
}

private val ordered = Regex("""^(\s*)(\d+)[.)]\s+(.*)""")
private val bullet = Regex("""^(\s*)[-*+]\s+(.*)""")
private val heading = Regex("""^(#{1,6})\s+(.*?)\s*#*$""")
private val rule = Regex("""^\s*([-*_])(\s*\1){2,}\s*$""")
private val tableDivider = Regex("""^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$""")

private fun parse(md: String): List<Block> {
    val out = mutableListOf<Block>()
    val lines = md.replace("\r\n", "\n").split('\n')
    var i = 0
    val para = StringBuilder()
    fun flush() {
        if (para.isNotBlank()) out += Block.Para(para.toString().trim('\n'))
        para.clear()
    }
    while (i < lines.size) {
        val line = lines[i]
        val trimmed = line.trimStart()
        when {
            trimmed.startsWith("```") || trimmed.startsWith("~~~") -> {
                flush()
                val fence = trimmed.take(3)
                val lang = trimmed.drop(3).trim()
                val code = StringBuilder()
                i++
                while (i < lines.size && !lines[i].trimStart().startsWith(fence)) {
                    code.append(lines[i]).append('\n')
                    i++
                }
                out += Block.Code(lang, code.toString().trimEnd('\n'))
            }
            line.isBlank() -> flush()
            heading.matches(trimmed) -> {
                flush()
                val m = heading.find(trimmed)!!
                out += Block.Heading(m.groupValues[1].length, m.groupValues[2])
            }
            rule.matches(line) && para.isEmpty() -> {
                flush()
                out += Block.Rule
            }
            trimmed.startsWith("|") && i + 1 < lines.size && tableDivider.matches(lines[i + 1]) -> {
                flush()
                val rows = mutableListOf(cells(line))
                i += 2
                while (i < lines.size && lines[i].trimStart().startsWith("|")) {
                    rows += cells(lines[i])
                    i++
                }
                out += Block.Table(rows)
                continue
            }
            trimmed.startsWith(">") -> {
                flush()
                val q = StringBuilder()
                while (i < lines.size && lines[i].trimStart().startsWith(">")) {
                    q.append(lines[i].trimStart().removePrefix(">").removePrefix(" ")).append('\n')
                    i++
                }
                out += Block.Quote(q.toString().trimEnd())
                continue
            }
            bullet.matches(line) -> {
                flush()
                val m = bullet.find(line)!!
                out += Block.Item(m.groupValues[1].length / 2, "•", m.groupValues[2])
            }
            ordered.matches(line) -> {
                flush()
                val m = ordered.find(line)!!
                out += Block.Item(m.groupValues[1].length / 2, "${m.groupValues[2]}.", m.groupValues[3])
            }
            // A wrapped line continues the list item above it.
            out.lastOrNull() is Block.Item && para.isEmpty() && line.startsWith("  ") -> {
                val last = out.removeAt(out.size - 1) as Block.Item
                out += last.copy(text = last.text + " " + trimmed)
            }
            else -> para.append(line).append('\n')
        }
        i++
    }
    flush()
    return out
}

private fun cells(line: String) = line.trim().removePrefix("|").removeSuffix("|").split('|').map { it.trim() }

private val linkStyle = TextLinkStyles(SpanStyle(color = C.accentHi, textDecoration = TextDecoration.Underline))
private val urlRegex = Regex("""https?://[^\s<>()\[\]]+[^\s<>()\[\].,;:!?'"]""")

/** Inline Markdown: bold, italic, strikethrough, code, links (images become links too). */
fun inline(text: String, codeBg: Color = C.code): AnnotatedString = buildAnnotatedString {
    var i = 0
    val n = text.length
    fun emphasis(marker: String, style: SpanStyle): Boolean {
        if (!text.startsWith(marker, i)) return false
        val end = text.indexOf(marker, i + marker.length)
        if (end <= i + marker.length) return false
        // "*" next to a space isn't emphasis ("2 * 3").
        if (text[i + marker.length].isWhitespace()) return false
        withStyle(style) { append(inline(text.substring(i + marker.length, end), codeBg)) }
        i = end + marker.length
        return true
    }
    while (i < n) {
        val c = text[i]
        when {
            c == '\\' && i + 1 < n && text[i + 1] in "\\`*_{}[]()#+-.!|~>" -> {
                append(text[i + 1]); i += 2
            }
            c == '`' -> {
                val end = text.indexOf('`', i + 1)
                if (end > i) {
                    // Drawn as a rounded chip by RichText (spans can't round their background).
                    pushStringAnnotation(CODE, "")
                    withStyle(SpanStyle(fontFamily = FontFamily.Monospace, fontSize = 0.86.em(), color = C.text)) {
                        append(" " + text.substring(i + 1, end) + " ")
                    }
                    pop()
                    i = end + 1
                } else { append(c); i++ }
            }
            (c == '[' || (c == '!' && i + 1 < n && text[i + 1] == '[')) -> {
                val start = if (c == '!') i + 1 else i
                val close = text.indexOf("](", start)
                val end = if (close > 0) text.indexOf(')', close) else -1
                if (close > 0 && end > 0 && !text.substring(start + 1, close).contains('\n')) {
                    val label = text.substring(start + 1, close).ifEmpty { "image" }
                    val url = text.substring(close + 2, end).trim().substringBefore(' ')
                    if (url.startsWith("http://") || url.startsWith("https://") || url.startsWith("mailto:")) {
                        withLink(LinkAnnotation.Url(url, linkStyle)) { append(inline(label, codeBg)) }
                    } else append(inline(label, codeBg))
                    i = end + 1
                } else { append(c); i++ }
            }
            c == 'h' && (text.startsWith("http://", i) || text.startsWith("https://", i)) -> {
                val m = urlRegex.matchAt(text, i)
                if (m != null) {
                    withLink(LinkAnnotation.Url(m.value, linkStyle)) { append(m.value) }
                    i += m.value.length
                } else { append(c); i++ }
            }
            c == '*' || c == '_' || c == '~' -> {
                val handled = emphasis("**", SpanStyle(fontWeight = FontWeight.SemiBold)) ||
                    emphasis("__", SpanStyle(fontWeight = FontWeight.SemiBold)) ||
                    emphasis("~~", SpanStyle(textDecoration = TextDecoration.LineThrough)) ||
                    (c != '~' && (i == 0 || !text[i - 1].isLetterOrDigit()) &&
                        emphasis(c.toString(), SpanStyle(fontStyle = FontStyle.Italic)))
                if (!handled) { append(c); i++ }
            }
            else -> { append(c); i++ }
        }
    }
}

private const val CODE = "code"

/** Text whose `code` ranges get a rounded chip behind them, like the web app's inline code. */
@Composable
fun RichText(text: AnnotatedString, style: TextStyle, modifier: Modifier = Modifier) {
    val ranges = remember(text) { text.getStringAnnotations(CODE, 0, text.length) }
    if (ranges.isEmpty()) {
        BasicText(text, modifier, style)
        return
    }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    BasicText(
        text,
        modifier.drawBehind {
            val l = layout ?: return@drawBehind
            val pad = 0.5.dp.toPx()
            val radius = CornerRadius(5.dp.toPx())
            for (r in ranges) {
                var start = r.start
                var guard = 0
                while (start < r.end && guard++ < 64) {
                    val line = l.getLineForOffset(start)
                    val lineEnd = l.getLineEnd(line, visibleEnd = true)
                    val end = minOf(r.end, lineEnd)
                    if (end > start) {
                        val left = l.getHorizontalPosition(start, true)
                        val right = if (end >= lineEnd) l.getLineRight(line) else l.getHorizontalPosition(end, true)
                        val top = l.getLineTop(line)
                        val h = l.getLineBottom(line) - top
                        val inset = h * 0.13f
                        val size = Size(kotlin.math.abs(right - left) + pad * 2, h - inset * 2)
                        val topLeft = Offset(minOf(left, right) - pad, top + inset)
                        drawRoundRect(C.elevated, topLeft, size, radius)
                        drawRoundRect(C.lineStrong, topLeft, size, radius, style = Stroke(1.dp.toPx()))
                    }
                    if (line >= l.lineCount - 1) break
                    start = maxOf(end, l.getLineStart(line + 1))
                }
            }
        },
        style,
        onTextLayout = { layout = it },
    )
}

private fun Double.em() = androidx.compose.ui.unit.TextUnit(this.toFloat(), androidx.compose.ui.unit.TextUnitType.Em)

@Composable
fun Markdown(text: String, modifier: Modifier = Modifier, color: Color = C.text, size: Float = 15f) {
    val blocks = remember(text) { parse(text) }
    val body = style(size.sp, color, lineHeight = (size * 1.6f).sp)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        for (b in blocks) {
            when (b) {
                is Block.Para -> RichText(inline(b.text), body)
                is Block.Heading -> RichText(
                    inline(b.text),
                    body.copy(
                        fontSize = when (b.level) { 1 -> (size + 5).sp; 2 -> (size + 3).sp; 3 -> (size + 1).sp; else -> size.sp },
                        fontWeight = FontWeight.SemiBold,
                        lineHeight = (size * 1.6f + 4).sp,
                    ),
                    modifier = Modifier.padding(top = 4.dp),
                )
                is Block.Item -> Row(Modifier.padding(start = (b.depth * 16).dp)) {
                    // Numbers line up whether they have one digit or two.
                    val numbered = b.marker != "•"
                    BasicText(
                        b.marker,
                        style = body.copy(color = C.faint, textAlign = if (numbered) TextAlign.End else TextAlign.Start),
                        modifier = Modifier.width(if (numbered) 26.dp else 16.dp).padding(end = if (numbered) 6.dp else 0.dp),
                    )
                    RichText(inline(b.text), body, Modifier.weight(1f))
                }
                is Block.Code -> Box(
                    Modifier.fillMaxWidth()
                        .background(C.code, RoundedCornerShape(12.dp))
                        .border(1.dp, C.line, RoundedCornerShape(12.dp)),
                ) {
                    Column {
                        CodeHeader(b.lang, b.text)
                        BasicText(
                            b.text,
                            style = style(12.5.sp, C.muted, lineHeight = 19.sp, mono = true),
                            softWrap = false,
                            modifier = Modifier.horizontalScroll(rememberScrollState()).padding(12.dp),
                        )
                    }
                }
                is Block.Quote -> Row(Modifier.height(IntrinsicSize.Min)) {
                    Box(Modifier.width(3.dp).fillMaxHeight().background(C.accent, RoundedCornerShape(2.dp)))
                    Markdown(b.text, Modifier.padding(start = 12.dp), C.muted, size)
                }
                is Block.Table -> Box(Modifier.horizontalScroll(rememberScrollState())) {
                    Column(
                        Modifier.width(IntrinsicSize.Max).clip(RoundedCornerShape(10.dp)).border(1.dp, C.lineStrong, RoundedCornerShape(10.dp)),
                    ) {
                        val cols = b.rows.maxOf { it.size }
                        val cellStyle = { r: Int -> style(13.5.sp, C.text, if (r == 0) FontWeight.SemiBold else FontWeight.Normal, 20.sp) }
                        // One width per column, from its widest cell (each row used to size its own
                        // cells, so the columns didn't line up from one row to the next).
                        val measurer = rememberTextMeasurer()
                        val density = LocalDensity.current
                        val widths = remember(b.rows, density) {
                            (0 until cols).map { c ->
                                val widest = b.rows.indices.maxOf { r ->
                                    measurer.measure(inline(b.rows[r].getOrElse(c) { "" }), cellStyle(r), softWrap = false).size.width
                                }
                                with(density) { (widest.toDp() + 22.dp).coerceIn(72.dp, 240.dp) }
                            }
                        }
                        b.rows.forEachIndexed { r, row ->
                            Row(Modifier.height(IntrinsicSize.Min).background(if (r == 0) C.elevated else Color.Transparent)) {
                                for (c in 0 until cols) {
                                    if (c > 0) Box(Modifier.width(1.dp).fillMaxHeight().background(C.line))
                                    RichText(
                                        inline(row.getOrElse(c) { "" }),
                                        cellStyle(r),
                                        Modifier.width(widths[c]).padding(horizontal = 11.dp, vertical = 7.dp),
                                    )
                                }
                            }
                            if (r < b.rows.size - 1) Box(Modifier.fillMaxWidth().height(1.dp).background(C.lineStrong))
                        }
                    }
                }
                Block.Rule -> Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
            }
        }
    }
}

/** A code block's header: its language and a Copy button. */
@Composable
private fun CodeHeader(lang: String, code: String) {
    val context = LocalContext.current
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) { if (copied) { delay(1500); copied = false } }
    Row(
        Modifier.fillMaxWidth().padding(start = 14.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicText(lang.ifEmpty { "code" }, Modifier.weight(1f), style(12.sp, C.faint, mono = true))
        Row(
            Modifier.press {
                (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("code", code))
                copied = true
            }.padding(horizontal = 8.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(if (copied) Lucide.Check else Lucide.Copy, if (copied) C.success else C.muted, 13.dp)
            BasicText(if (copied) "Copied" else "Copy", style = style(12.sp, C.muted))
        }
    }
    Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
}
