package dev.iris.app.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.BitmapFactory
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.foundation.text.BasicText
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.Agent
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.Message
import dev.iris.app.data.Prefixes
import dev.iris.app.data.Round
import dev.iris.app.data.ToolStep
import dev.iris.app.data.modelName
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import java.text.DateFormat
import java.util.Date

@Composable
fun MessageItem(vm: IrisViewModel, message: Message, agent: Agent?, isLast: Boolean) {
    val c = message.content
    when {
        message.role == "user" && c.startsWith(Prefixes.ROUTINE) -> Note(Lucide.Clock, C.accentHi) {
            "Routine “${c.substring(Prefixes.ROUTINE.length, c.indexOf(']').coerceAtLeast(Prefixes.ROUTINE.length)).trim()}”"
        }
        message.role == "user" && c.startsWith(Prefixes.SETUP) -> Note(Lucide.Sparkles, C.accentHi) {
            c.removePrefix(Prefixes.SETUP).replace(" Now do the user's request above.", "").trim()
        }
        message.role == "user" && c.startsWith(Prefixes.RELEASE) -> Note(
            Lucide.Rocket, if (Regex("FAILED|never reported").containsMatchIn(c)) C.danger else C.success,
        ) { c.removePrefix(Prefixes.RELEASE).lineSequence().first().split(". ").first().trim() + "." }
        message.role == "user" && c.startsWith(Prefixes.UPDATE) -> Note(Lucide.CircleCheck, C.success) {
            "${c.removePrefix(Prefixes.UPDATE).substringBefore(" has finished").trim()} finished its task · ${agent?.name ?: "the agent"} is reporting back"
        }
        message.role == "user" -> UserMessage(vm, message)
        message.role == "assistant" -> AssistantMessage(vm, message, agent, isLast)
    }
}

/** A small centered status line the engine posted (routine, update, setup). */
@Composable
private fun Note(icon: androidx.compose.ui.graphics.vector.ImageVector, tint: androidx.compose.ui.graphics.Color, text: () -> String) {
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Row(
            Modifier.widthIn(max = 520.dp).background(C.surface, RoundedCornerShape(16.dp)).padding(horizontal = 12.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(icon, tint, 12.dp, Modifier.padding(top = 2.dp))
            Txt(text(), 12.sp, C.faint, lineHeight = 17.sp)
        }
    }
}

@Composable
private fun UserMessage(vm: IrisViewModel, message: Message) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (a in message.attachments) {
            if (a.kind == "image") {
                val image by produceState<ImageBitmap?>(null, a.id) {
                    value = withContext(Dispatchers.IO) {
                        vm.attachment(message.threadId, a.id)?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
                    }
                }
                image?.let {
                    Image(
                        it, a.name, contentScale = ContentScale.Crop,
                        modifier = Modifier.heightIn(max = 200.dp).widthIn(max = 240.dp).clip(RoundedCornerShape(14.dp)),
                    )
                } ?: FileChip(a.name)
            } else FileChip(a.name)
        }
        if (message.content.isNotEmpty()) {
            val deep = message.content.startsWith(Prefixes.RESEARCH)
            val time = clock(message.createdAt)
            Column(
                Modifier.widthIn(max = 310.dp)
                    .shadow(10.dp, ME_SHAPE, ambientColor = C.bubbleMe, spotColor = C.bubbleMe)
                    .background(C.bubbleMe, ME_SHAPE)
                    .padding(start = 15.dp, end = 12.dp, top = 9.dp, bottom = 7.dp),
            ) {
                if (deep) {
                    Pill(Modifier.padding(bottom = 4.dp), Color.White.copy(alpha = 0.15f)) {
                        Icon(Lucide.Telescope, Color.White, 11.dp)
                        Txt("Deep research", 11.sp, Color.White, FontWeight.Medium)
                    }
                }
                Box {
                    SelectionContainer {
                        // Room for the time at the end of the last line (it sits on top).
                        BasicText(
                            buildAnnotatedString {
                                append(if (deep) message.content.removePrefix(Prefixes.RESEARCH).trim() else message.content)
                                withStyle(SpanStyle(color = Color.Transparent, fontSize = 11.sp)) { append("\u2002\u2002$time") }
                            },
                            style = style(15.sp, Color.White, lineHeight = 22.sp),
                        )
                    }
                    Txt(time, 10.5.sp, Color.White.copy(alpha = 0.65f), modifier = Modifier.align(Alignment.BottomEnd).padding(bottom = 1.dp))
                }
            }
        }
    }
}

@Composable
private fun FileChip(name: String) {
    Row(
        Modifier.background(C.elevated, RoundedCornerShape(12.dp)).border(1.dp, C.line, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Lucide.FileText, C.accentHi, 15.dp)
        Txt(name, 12.5.sp, C.text, maxLines = 1, modifier = Modifier.widthIn(max = 200.dp))
    }
}

@Composable
private fun AssistantMessage(vm: IrisViewModel, message: Message, agent: Agent?, isLast: Boolean) {
    val streaming = message.status == "streaming"
    // Replies from before tools existed have no rounds: show them as one.
    val rounds = message.rounds.ifEmpty { listOf(Round(message.reasoning, message.content)) }
    val lastRound = rounds.last()
    val idle = streaming && lastRound.content.isEmpty() && lastRound.reasoning.isEmpty() && lastRound.tools.isEmpty()
    // On a phone, every agent's tool steps fold into one box; only the answer, a question for
    // you (ask_user) and steps waiting for your OK stay out.
    val asks = lastRound.tools.isNotEmpty() && lastRound.tools.all { it.name == "ask_user" }
    val finalIdx = if (lastRound.tools.isEmpty() || asks) rounds.size - 1 else rounds.size
    // A reply that is only a question has no work to fold.
    val compact = rounds.subList(0, finalIdx).any { it.tools.isNotEmpty() }
    // What the user answered: their next message after this reply.
    val answer = run {
        val i = vm.messages.indexOfFirst { it.id == message.id }
        if (i < 0) null
        else vm.messages.drop(i + 1).firstOrNull {
            it.role == "user" && !it.content.startsWith("[I.R.I.S.") && !it.content.startsWith(Prefixes.ROUTINE)
        }?.content
    }
    val context = LocalContext.current

    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.padding(start = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Orb(agent, 26.dp, working = streaming)
            Txt(agent?.name ?: "Agent", 13.5.sp, C.text, FontWeight.SemiBold)
            Txt(clock(message.createdAt), 11.sp, C.faint)
        }
        Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (compact) {
                val work = rounds.subList(0, finalIdx)
                WorkCard(vm, work, streaming)
                for (step in work.flatMap { it.tools }.filter { it.status == "awaiting_approval" && it.approvalId != null }) {
                    ToolCard(vm, step)
                }
                if (finalIdx < rounds.size) {
                    if (lastRound.reasoning.isNotEmpty()) Thinking(lastRound.reasoning, streaming && lastRound.content.isEmpty())
                    if (lastRound.content.isNotEmpty()) Bubble(lastRound.content, first = true)
                    for (step in lastRound.tools.filter { it.name == "ask_user" }) QuestionCard(vm, agent, step, answer, live = !streaming)
                }
            } else {
                rounds.forEachIndexed { i, round ->
                    val isLastRound = i == rounds.size - 1
                    if (round.reasoning.isNotEmpty()) {
                        Thinking(round.reasoning, streaming && isLastRound && round.content.isEmpty() && round.tools.isEmpty())
                    }
                    if (round.content.isNotEmpty()) Bubble(round.content, first = rounds.subList(0, i).none { it.content.isNotEmpty() })
                    for (step in round.tools) {
                        if (step.name == "ask_user") QuestionCard(vm, agent, step, answer, live = !streaming) else ToolCard(vm, step)
                    }
                }
            }
            if (idle) Shimmer("Thinking…", 14.sp)
            else if (streaming && lastRound.content.isNotEmpty() && lastRound.tools.isEmpty()) Caret()
            when (message.status) {
                "error" -> Row(
                    Modifier.fillMaxWidth().background(C.dangerSoft, RoundedCornerShape(14.dp))
                        .border(1.dp, C.danger.copy(alpha = 0.3f), RoundedCornerShape(14.dp)).padding(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon(Lucide.CircleAlert, C.danger, 16.dp, Modifier.padding(top = 1.dp))
                    Column(Modifier.weight(1f)) {
                        Txt("The reply failed", 13.5.sp, C.danger, FontWeight.Medium)
                        Txt(message.error.orEmpty(), 13.sp, C.muted)
                    }
                    if (isLast) Txt("Retry", 13.sp, C.text, FontWeight.Medium, Modifier.press { vm.regenerate() }.padding(4.dp))
                }
                "cancelled" -> Txt("Stopped", 12.5.sp, C.faint)
            }
            if (!streaming && message.content.isNotEmpty()) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    IconButton(Lucide.Copy, {
                        (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager)
                            .setPrimaryClip(ClipData.newPlainText("reply", message.content))
                    }, size = 17.dp, modifier = Modifier.size(44.dp))
                    if (isLast) IconButton(Lucide.RotateCcw, { vm.regenerate() }, size = 17.dp, modifier = Modifier.size(44.dp))
                    message.model?.let { Txt(modelName(it), 11.sp, C.faint, modifier = Modifier.padding(start = 6.dp)) }
                }
            }
        }
    }
}

@Composable
private fun Thinking(text: String, live: Boolean) {
    // Folded unless opened, so long reasoning never floods the chat.
    var open by remember { mutableStateOf(false) }
    Column {
        Row(
            Modifier.press { open = !open }.padding(vertical = 2.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Lucide.Sparkles, C.accentHi, 13.dp)
            if (live) Shimmer("Thinking…", 13.5.sp) else Txt("Thought process", 13.5.sp, C.muted)
            Chevron(open, C.muted)
        }
        AnimatedVisibility(open, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            Row(Modifier.padding(top = 8.dp).height(IntrinsicSize.Min)) {
                Box(Modifier.width(2.dp).fillMaxHeight().background(C.lineStrong))
                SelectionContainer {
                    Txt(text, 13.sp, C.faint, lineHeight = 20.sp, modifier = Modifier.padding(start = 12.dp).heightIn(max = 320.dp).verticalScroll(rememberScrollState()))
                }
            }
        }
    }
}

private fun toolIcon(name: String) = when (name) {
    "web_search" -> Lucide.Search
    "web_fetch" -> Lucide.Globe
    "get_time" -> Lucide.Clock
    "list_files", "read_file", "write_file" -> Lucide.FileText
    "run_command" -> Lucide.Terminal
    "list_agents", "create_agent", "dismiss_agent", "check_task" -> Lucide.Users
    "delegate_task" -> Lucide.Send
    else -> Lucide.Wrench
}

private fun args(step: ToolStep): JsonObject? = runCatching { Json.parseToJsonElement(step.arguments.ifEmpty { "{}" }).jsonObject }.getOrNull()

private val prettyJson = Json { prettyPrint = true }

@Composable
fun ToolCard(vm: IrisViewModel, step: ToolStep) {
    val a = remember(step.arguments) { args(step) }
    val command = (a?.get("command") as? JsonPrimitive)?.content.orEmpty()
    val pretty = remember(a) { if (a != null && a.isNotEmpty()) prettyJson.encodeToString(JsonObject.serializer(), a) else "" }

    if (step.status == "awaiting_approval" && step.approvalId != null) {
        val detail = vm.approvals[step.approvalId]?.detail
        Column(
            Modifier.fillMaxWidth().background(C.warn.copy(alpha = 0.06f), RoundedCornerShape(14.dp))
                .border(1.dp, C.warn.copy(alpha = 0.4f), RoundedCornerShape(14.dp)).padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Lucide.ShieldAlert, C.warn, 16.dp)
                Txt("Approval needed", 13.5.sp, C.warn, FontWeight.SemiBold)
            }
            Txt(
                if (detail != null) "${step.summary}. Review the change:"
                else "The agent wants to ${if (command.isNotEmpty()) "run this command" else "use ${step.name}"}:",
                13.sp, C.muted,
            )
            CodeBox(detail ?: command.ifEmpty { pretty }, 260)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button("Approve", { vm.decide(step.approvalId, true) }, icon = Lucide.Check)
                Button("Deny", { vm.decide(step.approvalId, false) }, icon = Lucide.X, primary = false)
            }
        }
        return
    }

    var open by remember { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth().background(C.surface.copy(alpha = 0.6f), RoundedCornerShape(12.dp))
            .border(1.dp, C.line, RoundedCornerShape(12.dp)).animateContentSize(),
    ) {
        Row(
            Modifier.fillMaxWidth().press { open = !open }.padding(horizontal = 10.dp, vertical = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val err = step.status == "error"
            Box(
                Modifier.size(24.dp).background(if (err) C.dangerSoft else C.accentSoft, RoundedCornerShape(7.dp)),
                contentAlignment = Alignment.Center,
            ) { Icon(toolIcon(step.name), if (err) C.danger else C.accentHi, 13.dp) }
            Box(Modifier.weight(1f)) {
                if (step.status == "running") Shimmer(step.summary.ifEmpty { step.name }, 13.sp)
                else Txt(step.summary.ifEmpty { step.name }, 13.sp, C.muted, maxLines = 1)
            }
            if (step.autoApproved != null) {
                Pill(bg = C.accentSoft, modifier = Modifier) {
                    Icon(Lucide.ShieldCheck, C.accentHi, 10.dp)
                    Txt("Auto", 10.5.sp, C.accentHi, FontWeight.Medium)
                }
            }
            val fin = step.finishedAt
            if (fin != null && step.startedAt > 0) Txt("%.1fs".format((fin - step.startedAt) / 1000.0), 11.sp, C.faint)
            when (step.status) {
                "running" -> Spinner(14.dp)
                "done" -> Icon(Lucide.Check, C.success, 14.dp)
                "error" -> Icon(Lucide.X, C.danger, 14.dp)
                "denied" -> Icon(Lucide.Ban, C.warn, 14.dp)
                else -> Icon(Lucide.Ban, C.faint, 14.dp)
            }
            Chevron(open)
        }
        if (open) {
            Column(Modifier.padding(start = 10.dp, end = 10.dp, bottom = 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                step.autoApproved?.let { Txt("Auto-approved by the safety check: $it", 12.sp, C.accentHi) }
                if (pretty.isNotEmpty()) {
                    Txt("INPUT", 10.5.sp, C.faint, FontWeight.SemiBold)
                    CodeBox(command.ifEmpty { pretty }, 160)
                }
                if (step.output.isNotEmpty()) {
                    Txt("RESULT", 10.5.sp, C.faint, FontWeight.SemiBold)
                    CodeBox(step.output, 260)
                }
            }
        }
    }
}

@Composable
private fun CodeBox(text: String, maxHeight: Int) {
    SelectionContainer {
        Txt(
            text, 12.sp, C.muted, mono = true, lineHeight = 18.sp,
            modifier = Modifier.fillMaxWidth().heightIn(max = maxHeight.dp)
                .background(C.code, RoundedCornerShape(8.dp)).border(1.dp, C.line, RoundedCornerShape(8.dp))
                .verticalScroll(rememberScrollState()).padding(10.dp),
        )
    }
}

/** Iris's behind-the-scenes work folded into one box with a live status line. */
@Composable
private fun WorkCard(vm: IrisViewModel, rounds: List<Round>, live: Boolean) {
    var open by remember { mutableStateOf(false) }
    val steps = rounds.flatMap { it.tools }
    fun argOf(s: ToolStep, key: String) = (args(s)?.get(key) as? JsonPrimitive)?.content?.trim().orEmpty()
    val helpers = steps.mapNotNull {
        when (it.name) {
            "delegate_task" -> argOf(it, "agent")
            "create_agent" -> argOf(it, "name")
            else -> null
        }?.takeIf(String::isNotEmpty)
    }.distinctBy { it.lowercase() }.map { name -> name to vm.agents.firstOrNull { it.name.equals(name, true) } }
    val waitingOn = helpers.firstOrNull { (_, a) -> a != null && vm.isWaiting(a.id) }
    val workingOn = helpers.filter { (_, a) -> a != null && vm.isWorking(a.id) }
    val lastStep = steps.lastOrNull()
    val active = live || waitingOn != null || workingOn.isNotEmpty()
    val names = helpers.joinToString(", ") { it.first }
    val title = if (helpers.isNotEmpty()) {
        "$names ${if (active) (if (helpers.size > 1) "are" else "is") + " on it" else if (helpers.size > 1) "helped" else "helped out"}"
    } else lastStep?.summary ?: "Working…"
    val status = when {
        waitingOn != null -> "${waitingOn.first} needs your approval"
        workingOn.isNotEmpty() -> {
            val (name, a) = workingOn.first()
            val what = a?.let { vm.activityOf(it.id) }
            "$name is working${if (what != null && what != "thinking") " · ${what.replace('_', ' ')}" else "…"}"
        }
        live -> lastStep?.summary ?: "Working…"
        helpers.isEmpty() && steps.size == 1 -> "Done"
        helpers.isEmpty() -> "${steps.size} steps · tap to see them"
        else -> "${steps.size} ${if (steps.size == 1) "step" else "steps"}"
    }

    Column(
        Modifier.fillMaxWidth().background(C.surface.copy(alpha = 0.6f), RoundedCornerShape(14.dp))
            .border(1.dp, if (waitingOn != null) C.warn.copy(alpha = 0.45f) else C.line, RoundedCornerShape(14.dp))
            .animateContentSize(),
    ) {
        Row(
            Modifier.fillMaxWidth().press { open = !open }.padding(horizontal = 12.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (helpers.isNotEmpty()) {
                Row(horizontalArrangement = Arrangement.spacedBy((-8).dp)) {
                    for ((_, a) in helpers.take(3)) {
                        if (a != null) Orb(a, 24.dp, working = live && vm.isWorking(a.id), waiting = vm.isWaiting(a.id))
                        else Box(Modifier.size(24.dp).background(C.accentSoft, RoundedCornerShape(12.dp)), contentAlignment = Alignment.Center) {
                            Icon(Lucide.Users, C.accentHi, 12.dp)
                        }
                    }
                }
            } else {
                Box(Modifier.size(24.dp).background(C.accentSoft, RoundedCornerShape(7.dp)), contentAlignment = Alignment.Center) {
                    Icon(lastStep?.let { toolIcon(it.name) } ?: Lucide.Wrench, C.accentHi, 13.dp)
                }
            }
            Column(Modifier.weight(1f)) {
                Txt(title, 13.5.sp, C.text, FontWeight.Medium, maxLines = 1)
                if (active && waitingOn == null) Shimmer(status, 12.sp, FontWeight.Normal)
                else Txt(status, 12.sp, if (waitingOn != null) C.warn else C.faint, maxLines = 1)
            }
            when {
                waitingOn != null -> Icon(Lucide.ShieldAlert, C.warn, 15.dp)
                active -> Spinner(14.dp)
            }
            Chevron(open)
        }
        if (open) {
            Column(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (round in rounds) {
                    if (round.reasoning.isNotEmpty()) Thinking(round.reasoning, false)
                    if (round.content.isNotEmpty()) Markdown(round.content, color = C.muted, size = 14f)
                    for (step in round.tools) ToolCard(vm, step)
                }
            }
        }
    }
}

/** A chevron that turns to point down when its section is open. */
@Composable
fun Chevron(open: Boolean, tint: androidx.compose.ui.graphics.Color = C.faint) {
    val angle by animateFloatAsState(if (open) 90f else 0f, spring(dampingRatio = 0.8f, stiffness = 500f), label = "chevron")
    Icon(Lucide.ChevronRight, tint, 14.dp, Modifier.rotate(angle))
}

/** The pulsing bar at the end of a reply that is still coming in. */
@Composable
private fun Caret() {
    val a by rememberInfiniteTransition("caret").animateFloat(
        1f, 0.2f, infiniteRepeatable(tween(550), RepeatMode.Reverse), "alpha",
    )
    Box(Modifier.size(width = 3.dp, height = 16.dp).graphicsLayer { alpha = a }.background(C.accentHi, RoundedCornerShape(2.dp)))
}

/**
 * An agent's question with answers to tap (its ask_user tool). Picking one, or typing your own,
 * sends it as your next message; once answered the card shows what you chose.
 */
@Composable
private fun QuestionCard(vm: IrisViewModel, agent: Agent?, step: ToolStep, answer: String?, live: Boolean) {
    val a = remember(step.arguments) { args(step) }
    val question = (a?.get("question") as? JsonPrimitive)?.content.orEmpty()
    if (question.isEmpty() || agent == null) return
    val options = remember(a) {
        (a?.get("options") as? kotlinx.serialization.json.JsonArray).orEmpty()
            .mapNotNull { (it as? JsonPrimitive)?.content?.trim()?.takeIf(String::isNotEmpty) }
    }
    val allowOther = (a?.get("allow_other") as? JsonPrimitive)?.content != "false"
    val open = answer == null && live && step.status == "done"
    var sending by remember { mutableStateOf(false) }
    var other by remember { mutableStateOf("") }
    val haptics = LocalHapticFeedback.current
    fun pick(text: String) {
        val value = text.trim()
        if (value.isEmpty() || sending || !open) return
        sending = true
        haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
        vm.send(agent, value) { ok -> sending = false; if (ok) other = "" }
    }
    val shape = RoundedCornerShape(22.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(C.surface, shape)
            .border(1.dp, if (open) C.accent.copy(alpha = 0.45f) else C.lineStrong, shape),
    ) {
        Row(Modifier.padding(start = 14.dp, end = 14.dp, top = 13.dp, bottom = 10.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Icon(Lucide.MessageQuestion, C.accentHi, 18.dp, Modifier.padding(top = 1.dp))
            Txt(question, 14.5.sp, C.text, FontWeight.SemiBold, lineHeight = 20.sp)
        }
        options.forEachIndexed { i, o ->
            val chosen = answer != null && answer.trim() == o
            Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
            Row(
                Modifier.fillMaxWidth()
                    .background(if (chosen) C.accentSoft else androidx.compose.ui.graphics.Color.Transparent)
                    .press(open && !sending) { pick(o) }
                    .heightIn(min = 54.dp)
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.size(30.dp)
                        .background(if (chosen) C.accent else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(8.dp))
                        .border(1.dp, if (chosen) C.accent else C.lineStrong, RoundedCornerShape(8.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    if (chosen) Icon(Lucide.Check, C.onAccent, 14.dp)
                    else Txt(('A' + i).toString(), 12.sp, C.muted, FontWeight.SemiBold)
                }
                Txt(o, 14.sp, if (open || chosen) C.text else C.faint, lineHeight = 19.sp, modifier = Modifier.weight(1f))
            }
        }
        if (open && allowOther) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
            Row(Modifier.padding(start = 14.dp, end = 6.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                androidx.compose.foundation.text.BasicTextField(
                    value = other,
                    onValueChange = { other = it },
                    enabled = !sending,
                    singleLine = true,
                    textStyle = style(14.sp, C.text),
                    cursorBrush = androidx.compose.ui.graphics.SolidColor(C.accentHi),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                        capitalization = androidx.compose.ui.text.input.KeyboardCapitalization.Sentences,
                        imeAction = androidx.compose.ui.text.input.ImeAction.Send,
                    ),
                    keyboardActions = androidx.compose.foundation.text.KeyboardActions(onSend = { pick(other) }),
                    modifier = Modifier.weight(1f).padding(vertical = 10.dp),
                    decorationBox = { inner ->
                        Box {
                            if (other.isEmpty()) Txt("Type your own answer", 14.sp, C.faint)
                            inner()
                        }
                    },
                )
                if (sending) Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) { Spinner(15.dp) }
                else IconButton(Lucide.CornerDownLeft, { pick(other) }, if (other.isBlank()) C.faint else C.accentHi, 16.dp)
            }
        } else if (answer != null && options.none { it == answer.trim() }) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))
            Row(
                Modifier.fillMaxWidth().background(C.accentSoft).padding(horizontal = 14.dp, vertical = 11.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Lucide.Check, C.accentHi, 15.dp)
                Txt(answer, 14.sp, C.text, maxLines = 3)
            }
        }
    }
}

private val ME_SHAPE = RoundedCornerShape(20.dp, 20.dp, 6.dp, 20.dp)

/** An agent's words: hugs short replies, takes the full width for long ones. */
@Composable
private fun Bubble(text: String, first: Boolean) {
    val shape = RoundedCornerShape(if (first) 6.dp else 20.dp, 20.dp, 20.dp, 20.dp)
    Box(
        Modifier.background(C.bubble, shape).border(1.dp, C.bubbleLine, shape)
            .padding(horizontal = 15.dp, vertical = 10.dp),
    ) { SelectionContainer { Markdown(text) } }
}

private fun clock(ms: Long): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))
