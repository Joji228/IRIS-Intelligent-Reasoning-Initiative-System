package dev.iris.app.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.spring
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.R
import dev.iris.app.data.Agent
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.MAX_AGENTS
import dev.iris.app.data.PendingApproval
import dev.iris.app.data.Prefixes
import dev.iris.app.data.Screen
import java.util.Calendar

/** Home on a phone: a greeting, anything waiting for you, and the chats with each agent. */
@Composable
fun HomeScreen(vm: IrisViewModel) {
    val agents = vm.orderedAgents
    val approvals = vm.pendingApprovals
    val working = agents.count { vm.isWorking(it.id) }
    val main = agents.firstOrNull { it.role == "main" }
    val others = agents.filter { it.role != "main" }

    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(top = 4.dp, bottom = 40.dp)) {
        item("bar") {
            Row(Modifier.fillMaxWidth().padding(start = 18.dp, end = 4.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Image(painterResource(R.drawable.iris_logo), null, Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)))
                Txt("I.R.I.S.", 15.sp, C.text, FontWeight.SemiBold, Modifier.padding(start = 10.dp).weight(1f))
                vm.balance?.let { Pill { Txt(it, 12.sp, C.muted, FontWeight.Medium) } }
                IconButton(Lucide.Settings, { vm.open(Screen.Settings()) }, C.muted, 22.dp)
            }
        }
        item("head") {
            Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 4.dp)) {
                Row(verticalAlignment = Alignment.Bottom) {
                    Txt(greeting(), 28.sp, C.text, FontWeight.SemiBold)
                    BasicText(".", style = TextStyle(brush = C.rainbowBrush, fontSize = 28.sp, fontWeight = FontWeight.SemiBold))
                }
                val summary = buildString {
                    append("${agents.size} ${if (agents.size == 1) "agent" else "agents"}")
                    if (approvals.isNotEmpty()) append(" · ${approvals.size} waiting for you")
                    if (working > 0) append(" · $working working right now")
                    else if (approvals.isEmpty()) append(" · all quiet")
                }
                Txt(summary, 14.sp, C.muted, modifier = Modifier.padding(top = 4.dp))
            }
        }

        vm.phoneUpdate?.let { (release, _) ->
            item("update") {
                Row(
                    Modifier.animateItem().padding(start = 16.dp, end = 16.dp, top = 14.dp).fillMaxWidth()
                        .background(C.accentSoft, RoundedCornerShape(20.dp))
                        .border(1.dp, C.accent.copy(alpha = 0.35f), RoundedCornerShape(20.dp))
                        .press { vm.open(Screen.Settings("about")) }.padding(horizontal = 16.dp, vertical = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(Lucide.Download, C.accentHi, 18.dp)
                    Column(Modifier.weight(1f)) {
                        Txt("Version ${release.version} is out", 14.sp, C.text, FontWeight.SemiBold)
                        Txt("Tap to update this app", 12.5.sp, C.faint, maxLines = 1)
                    }
                    Icon(Lucide.ChevronRight, C.faint, 16.dp)
                }
            }
        }

        if (vm.problems.isNotEmpty()) {
            item("problems") {
                val n = vm.problems.size
                Row(
                    Modifier.animateItem().padding(start = 16.dp, end = 16.dp, top = 14.dp).fillMaxWidth()
                        .background(C.danger.copy(alpha = 0.08f), RoundedCornerShape(20.dp))
                        .border(1.dp, C.danger.copy(alpha = 0.3f), RoundedCornerShape(20.dp))
                        .press { vm.open(Screen.Settings("problems")) }.padding(horizontal = 16.dp, vertical = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(Lucide.CircleAlert, C.danger, 18.dp)
                    Column(Modifier.weight(1f)) {
                        Txt(if (n == 1) "1 problem needs a look" else "$n problems need a look", 14.sp, C.text, FontWeight.SemiBold)
                        Txt("Failed replies or routines", 12.5.sp, C.faint, maxLines = 1)
                    }
                    Icon(Lucide.ChevronRight, C.faint, 16.dp)
                }
            }
        }

        if (approvals.isNotEmpty()) {
            item("approvals-title") { SectionTitle("Needs your attention", C.warn, Lucide.ShieldAlert) }
            items(approvals, key = { "ap-" + it.id }) { a ->
                Box(Modifier.animateItem().padding(horizontal = 16.dp, vertical = 5.dp)) { ApprovalCard(vm, a) }
            }
        }

        item("chats-title") { SectionTitle("Chats", C.faint) }
        if (main != null) {
            item(main.id) { Box(Modifier.animateItem()) { AgentRow(vm, main) } }
            if (others.isNotEmpty()) {
                item("sep") { Box(Modifier.padding(start = 86.dp, end = 20.dp, top = 2.dp, bottom = 2.dp).fillMaxWidth().height(1.dp).background(C.line)) }
            }
        }
        items(others, key = { it.id }) { agent -> Box(Modifier.animateItem()) { AgentRow(vm, agent) } }
        if (agents.size < MAX_AGENTS) {
            item("new") {
                Row(
                    Modifier.padding(horizontal = 8.dp).fillMaxWidth().clip(RoundedCornerShape(22.dp))
                        // A long press does nothing: only a deliberate tap summons an orb.
                        .pressLong(onLong = {}) { vm.summonAgent() }
                        .padding(horizontal = 12.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Box(
                        Modifier.size(52.dp).drawBehind {
                            drawCircle(C.lineStrong, size.minDimension / 2 - 1.dp.toPx(), style = Stroke(1.2.dp.toPx(), pathEffect = DASH))
                        },
                        contentAlignment = Alignment.Center,
                    ) { Icon(Lucide.Plus, C.muted, 20.dp) }
                    Column(Modifier.weight(1f)) {
                        Txt("New agent", 15.sp, C.muted, FontWeight.Medium)
                        Txt("${agents.size} of $MAX_AGENTS · becomes a specialist from its first message", 12.5.sp, C.faint, maxLines = 1)
                    }
                }
            }
        }
    }
}

private val DASH = PathEffect.dashPathEffect(floatArrayOf(10f, 8f))

@Composable
private fun SectionTitle(text: String, color: Color, icon: androidx.compose.ui.graphics.vector.ImageVector? = null) {
    Row(
        Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (icon != null) Icon(icon, color, 14.dp)
        Txt(text.uppercase(), 12.sp, color, FontWeight.SemiBold)
    }
}

/** One chat in the list: the live orb, the name, and the last thing said or what it's doing. */
@Composable
private fun AgentRow(vm: IrisViewModel, agent: Agent) {
    val overlays = LocalOverlays.current
    val unread = vm.unread[agent.id] == true
    val preview = vm.previews[agent.id]
    Row(
        Modifier.padding(horizontal = 8.dp).fillMaxWidth().clip(RoundedCornerShape(22.dp))
            .pressLong(onLong = { agentMenu(vm, overlays, agent) }) { vm.openAgent(agent.id) }
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        AgentOrb(vm, agent, if (agent.role == "main") 56.dp else 52.dp, chip = true)
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Txt(agent.name, 16.sp, C.text, if (unread) FontWeight.Bold else FontWeight.SemiBold, Modifier.weight(1f), maxLines = 1)
                if (preview != null) {
                    Txt(
                        listTime(preview.createdAt), 12.sp,
                        if (unread) C.accentHi else C.faint, if (unread) FontWeight.Medium else FontWeight.Normal,
                        Modifier.padding(start = 8.dp),
                    )
                }
            }
            Row(Modifier.padding(top = 3.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(Modifier.weight(1f)) {
                    when {
                        vm.isWaiting(agent.id) -> Txt("Waiting for your approval", 14.sp, C.warn, FontWeight.Medium, maxLines = 1)
                        vm.isWorking(agent.id) -> Shimmer(vm.statusLine(agent.id), 14.sp)
                        preview != null -> BasicText(
                            buildAnnotatedString {
                                if (preview.role == "user") withStyle(SpanStyle(color = C.muted)) { append("You: ") }
                                append(previewText(preview.text))
                            },
                            style = style(14.sp, if (unread) C.text else C.faint),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        else -> Txt(agent.description.ifEmpty { "No messages yet" }, 14.sp, C.faint, maxLines = 1)
                    }
                }
                AnimatedVisibility(unread, enter = scaleIn(spring(0.5f, 600f)), exit = scaleOut()) {
                    Box(Modifier.size(10.dp).background(C.accent, CircleShape))
                }
            }
        }
    }
}

/** A chat line as plain text: no research tag, Markdown marks or line breaks. */
private fun previewText(s: String) = s.removePrefix(Prefixes.RESEARCH)
    .replace(Regex("[#*_`>|]+"), "")
    .replace(Regex("\\s+"), " ")
    .trim()

@Composable
fun ApprovalCard(vm: IrisViewModel, a: PendingApproval, showAgent: Boolean = true) {
    val agent = vm.agent(a.agentId)
    Column(
        Modifier.fillMaxWidth()
            .background(C.warn.copy(alpha = 0.05f), RoundedCornerShape(22.dp))
            .border(1.dp, C.warn.copy(alpha = 0.35f), RoundedCornerShape(22.dp))
            .press { vm.openAgent(a.agentId) }
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            if (showAgent) Orb(agent, 34.dp, waiting = true)
            Column(Modifier.weight(1f)) {
                Txt("${agent?.name ?: "An agent"} wants to:", 13.sp, C.muted)
                Txt(a.summary, 13.5.sp, C.text, mono = true, maxLines = 3)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button("Approve", { vm.decide(a.id, true) }, icon = Lucide.Check)
            Button("Deny", { vm.decide(a.id, false) }, icon = Lucide.X, primary = false)
        }
    }
}

/** An agent's actions (the desktop's orb right-click menu). */
fun agentMenu(vm: IrisViewModel, overlays: Overlays, agent: Agent, inChat: Boolean = false) {
    val busy = vm.isWorking(agent.id)
    overlays.menu(
        title = agent.name,
        items = buildList {
            if (!inChat) add(MenuItem("Open chat", Lucide.ChevronRight) { vm.openAgent(agent.id) })
            add(MenuItem("Edit agent", Lucide.Pencil) { vm.open(Screen.AgentEdit(agent.id)) })
            add(MenuItem("Compact the chat", Lucide.Archive, enabled = !busy) { vm.compactChat(agent.id) })
            add(MenuItem("Clear chat", Lucide.Eraser, danger = true, enabled = !busy, confirm = "Tap again to clear the chat") { vm.clearChat(agent.id) })
            if (agent.role != "main") {
                add(MenuItem("Delete agent", Lucide.Trash, danger = true, confirm = "Tap again to delete ${agent.name}") { vm.deleteAgent(agent.id) })
            }
        },
    )
}

@Composable
fun Dot(color: Color) = Box(Modifier.size(6.dp).background(color, CircleShape))

fun greeting(): String {
    val h = Calendar.getInstance().get(Calendar.HOUR_OF_DAY)
    return when {
        h < 5 -> "Working late"
        h < 12 -> "Good morning"
        h < 18 -> "Good afternoon"
        else -> "Good evening"
    }
}

fun relativeTime(ms: Long, now: Long = System.currentTimeMillis()): String {
    val diff = now - ms
    if (diff < 60_000) return "just now"
    if (diff < 3_600_000) return "${diff / 60_000}m ago"
    val today = startOfDay(now)
    return when {
        ms >= today -> "${diff / 3_600_000}h ago"
        ms >= today - DAY -> "Yesterday"
        ms >= today - 6 * DAY -> android.text.format.DateFormat.format("EEE", ms).toString()
        else -> android.text.format.DateFormat.format("d MMM", ms).toString()
    }
}

/** A chat list's time: "14:05" today, then "Yesterday", "Mon", "2 Nov". */
fun listTime(ms: Long, now: Long = System.currentTimeMillis()): String {
    val today = startOfDay(now)
    return when {
        ms >= today -> java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(ms))
        ms >= today - DAY -> "Yesterday"
        ms >= today - 6 * DAY -> android.text.format.DateFormat.format("EEE", ms).toString()
        else -> android.text.format.DateFormat.format("d MMM", ms).toString()
    }
}

/** A chat's day divider: "Today", "Yesterday", "Monday", "Mon 2 Nov", "2 Nov 2025". */
fun dayLabel(ms: Long, now: Long = System.currentTimeMillis()): String {
    val today = startOfDay(now)
    val year = { x: Long -> Calendar.getInstance().apply { timeInMillis = x }.get(Calendar.YEAR) }
    return when {
        ms >= today -> "Today"
        ms >= today - DAY -> "Yesterday"
        ms >= today - 6 * DAY -> android.text.format.DateFormat.format("EEEE", ms).toString()
        year(ms) == year(now) -> android.text.format.DateFormat.format("EEE d MMM", ms).toString()
        else -> android.text.format.DateFormat.format("d MMM yyyy", ms).toString()
    }
}

fun startOfDay(ms: Long): Long = Calendar.getInstance().apply {
    timeInMillis = ms
    set(Calendar.HOUR_OF_DAY, 0); set(Calendar.MINUTE, 0); set(Calendar.SECOND, 0); set(Calendar.MILLISECOND, 0)
}.timeInMillis

private const val DAY = 86_400_000L
