package dev.iris.app.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.formatTokens

/** A full-screen page with a back button and a title. */
@Composable
fun Page(title: String, onBack: () -> Unit, subtitle: String? = null, action: (@Composable () -> Unit)? = null, content: LazyListScope.() -> Unit) {
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(Lucide.ChevronLeft, onBack, C.muted, 26.dp)
            Column(Modifier.weight(1f).padding(start = 2.dp)) {
                Txt(title, 19.sp, C.text, FontWeight.SemiBold, maxLines = 1)
                if (subtitle != null) Txt(subtitle, 12.5.sp, C.faint, maxLines = 1)
            }
            action?.invoke()
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
        LazyColumn(
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 48.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
            content = content,
        )
    }
}

/** A rounded group of settings, optionally highlighted (the desktop's accent box). */
@Composable
fun Card(modifier: Modifier = Modifier, accent: Boolean = false, padding: Boolean = true, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(22.dp)
    Column(
        modifier.fillMaxWidth()
            .background(if (accent) C.accentSoft.copy(alpha = 0.08f) else C.surface, shape)
            .border(1.dp, if (accent) C.accent.copy(alpha = 0.3f) else C.line, shape)
            .clip(shape)
            .then(if (padding) Modifier.padding(18.dp) else Modifier),
        verticalArrangement = Arrangement.spacedBy(14.dp),
        content = content,
    )
}

@Composable
fun CardTitle(title: String, hint: String? = null) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Txt(title, 16.sp, C.text, FontWeight.SemiBold)
        if (hint != null) Txt(hint, 13.sp, C.faint, lineHeight = 18.sp)
    }
}

@Composable
fun Hint(text: String, color: Color = C.faint) = Txt(text, 13.sp, color, lineHeight = 18.sp)

/** A tappable line in a list: icon, title, what it's set to, chevron. */
@Composable
fun NavRow(icon: ImageVector, title: String, value: String? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).press(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(38.dp).background(C.accentSoft, RoundedCornerShape(12.dp)), contentAlignment = Alignment.Center) {
            Icon(icon, C.accentHi, 19.dp)
        }
        Column(Modifier.weight(1f)) {
            Txt(title, 16.sp, C.text, FontWeight.Medium)
            if (value != null) Txt(value, 13.sp, C.faint, maxLines = 1)
        }
        Icon(Lucide.ChevronRight, C.faint, 18.dp)
    }
}

@Composable
fun Divider(inset: Boolean = false) =
    Box(Modifier.fillMaxWidth().padding(start = if (inset) 68.dp else 0.dp).height(1.dp).background(C.line))

/** An on/off switch with a springy knob. */
@Composable
fun Switch(checked: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true) {
    val x by animateDpAsState(if (checked) 20.dp else 0.dp, spring(dampingRatio = 0.8f, stiffness = 600f), label = "knob")
    val track by animateColorAsState(if (checked) C.accent else C.lineStrong, Motion.standard(), label = "track")
    Box(
        Modifier.width(52.dp).height(32.dp).background(track, CircleShape)
            .press(enabled, Role.Switch) { onChange(!checked) }.padding(4.dp),
    ) {
        Box(Modifier.offset(x = x).size(24.dp).background(Color.White, CircleShape))
    }
}

/** A line with a title, a hint and a switch. */
@Composable
fun SwitchRow(title: String, hint: String?, checked: Boolean, onChange: (Boolean) -> Unit, badge: (@Composable () -> Unit)? = null) {
    Row(
        Modifier.fillMaxWidth().press { onChange(!checked) },
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Txt(title, 15.5.sp, C.text, FontWeight.Medium)
                badge?.invoke()
            }
            if (hint != null) Txt(hint, 13.sp, C.faint, lineHeight = 18.sp)
        }
        Switch(checked, onChange)
    }
}

/** One of a few choices, each with a short explanation (the desktop's choice buttons). */
@Composable
fun ChoiceCards(options: List<Triple<String, String, String>>, selected: String, onSelect: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((id, title, text) in options) {
            val on = id == selected
            val border by animateColorAsState(if (on) C.accent.copy(alpha = 0.7f) else C.lineStrong, label = "b")
            val bg by animateColorAsState(if (on) C.accentSoft else Color.Transparent, label = "bg")
            Row(
                Modifier.fillMaxWidth().background(bg, RoundedCornerShape(18.dp)).border(1.dp, border, RoundedCornerShape(18.dp))
                    .press { onSelect(id) }.padding(horizontal = 16.dp, vertical = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Radio(on)
                Column(Modifier.weight(1f)) {
                    Txt(title, 15.sp, C.text, FontWeight.Medium)
                    if (text.isNotEmpty()) Txt(text, 13.sp, C.faint, lineHeight = 17.sp)
                }
            }
        }
    }
}

@Composable
private fun Radio(on: Boolean) {
    val dot by animateFloatAsState(if (on) 1f else 0f, spring(dampingRatio = 0.6f), label = "dot")
    Box(
        Modifier.size(22.dp).border(2.dp, if (on) C.accentHi else C.faint, CircleShape),
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.size(11.dp * dot).background(C.accentHi, CircleShape)) }
}

/** Small round choices in a wrapping row. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Chips(options: List<Pair<String, String>>, selected: String?, mono: Boolean = false, onSelect: (String) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((value, label) in options) {
            val on = value == selected
            val bg by animateColorAsState(if (on) C.accentSoft else Color.Transparent, Motion.quick(), label = "chip")
            val edge by animateColorAsState(if (on) C.accent.copy(alpha = 0.7f) else C.lineStrong, Motion.quick(), label = "chipEdge")
            Txt(
                label, 14.sp, if (on) C.text else C.muted, if (on) FontWeight.Medium else FontWeight.Normal, mono = mono,
                modifier = Modifier
                    .background(bg, RoundedCornerShape(50))
                    .border(1.dp, edge, RoundedCornerShape(50))
                    .press { onSelect(value) }
                    .padding(horizontal = 16.dp, vertical = 10.dp),
            )
        }
    }
}

/** A tappable field that opens a picker. */
@Composable
fun SelectField(label: String?, value: String, onClick: () -> Unit, mono: Boolean = false) {
    Column {
        if (label != null) Label(label)
        Row(
            Modifier.fillMaxWidth().background(C.bg.copy(alpha = 0.4f), RoundedCornerShape(16.dp))
                .border(1.dp, C.lineStrong, RoundedCornerShape(16.dp)).press(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Txt(value, 15.sp, C.text, mono = mono, maxLines = 1, modifier = Modifier.weight(1f))
            Icon(Lucide.ChevronDown, C.faint, 18.dp)
        }
    }
}

/** A multi-line text box (instructions, memory, skills). */
@Composable
fun TextArea(
    value: String,
    onChange: (String) -> Unit,
    placeholder: String = "",
    minHeight: Int = 120,
    maxHeight: Int = 360,
    mono: Boolean = false,
    maxLength: Int? = null,
) {
    BasicTextField(
        value = value,
        onValueChange = { onChange(if (maxLength != null) it.take(maxLength) else it) },
        textStyle = style(if (mono) 13.sp else 14.5.sp, C.text, lineHeight = if (mono) 19.sp else 21.sp, mono = mono),
        cursorBrush = SolidColor(C.accentHi),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        modifier = Modifier.fillMaxWidth().heightIn(min = minHeight.dp, max = maxHeight.dp)
            .background(C.bg.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
            .border(1.dp, C.lineStrong, RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 12.dp),
        decorationBox = { inner ->
            Box {
                if (value.isEmpty()) Txt(placeholder, 14.sp, C.faint, lineHeight = 20.sp)
                inner()
            }
        },
    )
}

/** How much of a token limit is used. */
@Composable
fun UsageBar(label: String, used: Long, limit: Long?, hint: String) {
    val ratio = if (limit != null && limit > 0) (used.toFloat() / limit).coerceIn(0f, 1f) else 0f
    val anim by animateFloatAsState(ratio, label = "usage")
    val color = when {
        ratio >= 1f -> C.danger
        ratio >= 0.8f -> C.warn
        else -> C.accentHi
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row {
            Txt(label, 14.sp, C.text, FontWeight.Medium, Modifier.weight(1f))
            Txt(if (limit != null && limit > 0) "${formatTokens(used)} / ${formatTokens(limit)}" else "${formatTokens(used)} · no limit", 12.5.sp, C.muted)
        }
        Box(Modifier.fillMaxWidth().height(6.dp).background(C.hover, CircleShape)) {
            if (limit != null && limit > 0) Box(Modifier.fillMaxWidth(anim).fillMaxHeight().background(color, CircleShape))
        }
        Txt(hint, 11.5.sp, C.faint)
    }
}

/** A small status label ("Connected", "Asks first"). */
@Composable
fun Badge(text: String, color: Color, icon: ImageVector? = null) {
    Row(
        Modifier.background(color.copy(alpha = 0.12f), RoundedCornerShape(50)).padding(horizontal = 7.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        if (icon != null) Icon(icon, color, 10.dp)
        Txt(text, 10.5.sp, color, FontWeight.Medium)
    }
}

/** A small heading over a group of settings (Assistant, Automation, Safety…). */
@Composable
fun GroupTitle(text: String) =
    Txt(text.uppercase(), 11.5.sp, C.faint, FontWeight.SemiBold, Modifier.padding(start = 8.dp, top = 4.dp))

/** A compact switch between a few options, with the chosen one highlighted. */
@Composable
fun Segmented(options: List<Pair<String, String>>, selected: String, onSelect: (String) -> Unit) {
    Row(
        Modifier.border(1.dp, C.lineStrong, RoundedCornerShape(12.dp)).padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for ((id, label) in options) {
            val on = id == selected
            val bg by animateColorAsState(if (on) C.accentSoft else Color.Transparent, Motion.quick(), label = "seg")
            val fg by animateColorAsState(if (on) C.accentHi else C.muted, Motion.quick(), label = "segText")
            Txt(
                label, 12.5.sp, fg, if (on) FontWeight.SemiBold else FontWeight.Normal,
                Modifier.clip(RoundedCornerShape(9.dp)).background(bg)
                    .press(role = Role.RadioButton) { onSelect(id) }
                    .padding(horizontal = 8.dp, vertical = 6.dp),
            )
        }
    }
}

/** Fades and lifts a block in when it first shows; `index` staggers a list of them. */
fun Modifier.appearIn(index: Int = 0): Modifier = composed {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val k by animateFloatAsState(if (shown) 1f else 0f, tween(320, delayMillis = 45 * index, easing = Motion.ease), label = "appear")
    graphicsLayer {
        alpha = k
        translationY = (1f - k) * 14.dp.toPx()
    }
}
