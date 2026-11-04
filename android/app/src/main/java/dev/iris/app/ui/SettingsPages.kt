package dev.iris.app.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.Device
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.LinkStatus
import dev.iris.app.data.McpServer
import dev.iris.app.data.Pairing
import dev.iris.app.data.Routine
import dev.iris.app.data.Skill
import dev.iris.app.data.ToolLogEntry
import dev.iris.app.data.VaultAccount
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.text.DateFormat
import java.util.Date

/** The list-style sections, each loading its own data when opened. */
@Composable
fun SettingsPage(vm: IrisViewModel, page: String, back: () -> Unit) {
    when (page) {
        "routines" -> RoutinesPage(vm, back)
        "skills" -> SkillsPage(vm, back)
        "connections" -> ConnectionsPage(vm, back)
        "activity" -> ActivityPage(vm, back)
        "devices" -> DevicesPage(vm, back)
        "remote" -> RemotePage(vm, back)
        "providers" -> ProvidersPage(vm, back)
        "accounts" -> AccountsPage(vm, back)
        else -> Page("Settings", back) {}
    }
}

private fun when_(ms: Long?): String = ms?.let { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(it)) } ?: "—"

@Composable
private fun Empty(text: String) = Card { Hint(text) }

@Composable
private fun Loading() = Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) { Spinner(20.dp, C.faint) }

// ---- Routines -------------------------------------------------------------------------

private val SCHEDULES = listOf("every 30m", "every 2h", "daily 08:00", "weekdays 09:00", "weekly mon 09:00")
private val KINDS = listOf(
    Triple("prompt", "Task for the agent", "The agent gets this as a message and does it."),
    Triple("heartbeat", "Heartbeat", "The agent checks a list; you only hear about it when something needs you."),
    Triple("command", "Command (no AI)", "PowerShell runs on its own; the agent only hears about failures."),
)

@Composable
private fun RoutinesPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    var routines by remember { mutableStateOf<List<Routine>?>(null) }
    var editing by remember { mutableStateOf<Routine?>(null) }
    suspend fun load() { vm.attempt { routines() }?.let { routines = it } }
    LaunchedEffect(Unit) { load() }

    val form = editing
    if (form != null) {
        RoutineForm(vm, form, onDone = { editing = null; scope.launch { load() } })
        return
    }
    Page("Routines", back, action = {
        IconButton(Lucide.Plus, { editing = Routine("", vm.orderedAgents.firstOrNull()?.id.orEmpty(), schedule = "daily 08:00") }, C.accentHi)
    }) {
        item { Hint("Things agents do on a schedule. A task wakes the agent; a command runs without the AI and only wakes it when something fails. Agents can create these too.") }
        val list = routines
        when {
            list == null -> item { Loading() }
            list.isEmpty() -> item { Empty("No routines yet. Tap + to add one, or ask Iris: \"every morning at 8, check my sites\".") }
            else -> items(list, key = { it.id }) { r ->
                val agent = vm.agent(r.agentId)
                Card {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Orb(agent, 30.dp)
                        Column(Modifier.weight(1f)) {
                            Txt(r.name, 15.sp, C.text, FontWeight.SemiBold, maxLines = 1)
                            Txt("${agent?.name ?: "?"} · ${r.schedule}", 12.5.sp, C.faint, mono = false, maxLines = 1)
                        }
                        Switch(r.enabled, { on ->
                            routines = list.map { if (it.id == r.id) it.copy(enabled = on) else it }
                            scope.launch { vm.attempt { editRoutine(r.id, buildJsonObject { put("enabled", on) }) } ?: load() }
                        })
                    }
                    Txt(r.body, 13.sp, C.muted, maxLines = 3, lineHeight = 18.sp, mono = r.kind == "command")
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Txt("Next: ${if (r.enabled) when_(r.nextRun) else "off"}", 11.5.sp, C.faint)
                        Txt("Last: ${when_(r.lastRun)}", 11.5.sp, C.faint)
                    }
                    if (r.lastResult.isNotEmpty()) Txt(r.lastResult, 12.sp, C.faint, maxLines = 2)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button("Run now", {
                            scope.launch { vm.attempt { runRoutine(r.id) }?.let { vm.showToast("“${r.name}” started") } }
                        }, icon = Lucide.Play, primary = false)
                        Button("Edit", { editing = r }, icon = Lucide.Pencil, primary = false)
                        Button("Delete", {
                            overlays.menu("Delete “${r.name}”?", listOf(MenuItem("Delete routine", Lucide.Trash, danger = true) {
                                scope.launch { vm.attempt { deleteRoutine(r.id) }; load() }
                            }))
                        }, icon = Lucide.Trash, primary = false)
                    }
                }
            }
        }
    }
}

@Composable
private fun RoutineForm(vm: IrisViewModel, r: Routine, onDone: () -> Unit) {
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    var name by remember { mutableStateOf(r.name) }
    var agentId by remember { mutableStateOf(r.agentId) }
    var schedule by remember { mutableStateOf(r.schedule) }
    var kind by remember { mutableStateOf(r.kind) }
    var body by remember { mutableStateOf(r.body) }
    var notify by remember { mutableStateOf(r.notify) }
    var busy by remember { mutableStateOf(false) }
    androidx.activity.compose.BackHandler(onBack = onDone)
    Page(if (r.id.isEmpty()) "New routine" else "Edit routine", onDone) {
        item {
            Card {
                Column { Label("Name"); Field(name, { name = it }, placeholder = "Morning site check") }
                SelectField("Agent", vm.agent(agentId)?.name ?: "Choose", onClick = {
                    overlays.picker("Agent", vm.orderedAgents.map { it.id to it.name }, agentId) { agentId = it }
                })
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Label("When")
                    Field(schedule, { schedule = it }, mono = true)
                    Chips(SCHEDULES.map { it to it }, schedule, mono = true) { schedule = it }
                    Hint("Also: weekends 10:00, every 1d, once 2026-11-05 18:00. Local time; at most every 5 minutes.")
                }
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Label("Type")
                    ChoiceCards(KINDS, kind) { kind = it }
                }
                Column {
                    Label(if (kind == "command") "PowerShell command" else if (kind == "heartbeat") "Checklist" else "Task")
                    TextArea(body, { body = it }, if (kind == "command") "Get-PSDrive C | Select Free" else "Check that my sites load and tell me if one is slow or down.", mono = kind == "command")
                }
                if (kind == "command") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Label("Wake the agent")
                        Chips(listOf("failure" to "Only when it fails", "always" to "Every run"), notify) { notify = it }
                    }
                }
                Button(if (r.id.isEmpty()) "Create routine" else "Save", {
                    busy = true
                    scope.launch {
                        val fields = buildJsonObject {
                            put("agent_id", agentId); put("name", name.trim()); put("schedule", schedule.trim())
                            put("kind", kind); put("body", body.trim()); put("notify", notify)
                        }
                        val ok = vm.attempt { if (r.id.isEmpty()) createRoutine(fields) else editRoutine(r.id, fields) }
                        busy = false
                        if (ok != null) onDone()
                    }
                }, Modifier.fillMaxWidth(), enabled = name.isNotBlank() && schedule.isNotBlank() && body.isNotBlank() && agentId.isNotEmpty(), busy = busy)
            }
        }
    }
}

// ---- Skills ---------------------------------------------------------------------------

@Composable
private fun SkillsPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    var skills by remember { mutableStateOf<List<Skill>?>(null) }
    var editing by remember { mutableStateOf<Pair<String?, Skill>?>(null) }
    suspend fun load() { vm.attempt { skills() }?.let { skills = it } }
    LaunchedEffect(Unit) { load() }

    editing?.let { (original, s) ->
        var name by remember(s) { mutableStateOf(s.name) }
        var description by remember(s) { mutableStateOf(s.description) }
        var body by remember(s) { mutableStateOf(s.body) }
        var busy by remember { mutableStateOf(false) }
        androidx.activity.compose.BackHandler { editing = null }
        Page(if (original == null) "New skill" else s.name, { editing = null }) {
            item {
                Card {
                    Column { Label("Name"); Field(name, { name = it }, placeholder = "check-sites", mono = true) }
                    Column { Label("When to use it"); Field(description, { description = it }, placeholder = "Checking whether websites are up and fast", singleLine = false) }
                    Column { Label("Instructions (SKILL.md)"); TextArea(body, { body = it }, "Step by step, how to do it well.", minHeight = 200, maxHeight = 480, mono = true) }
                    Button("Save skill", {
                        busy = true
                        scope.launch {
                            val saved = vm.attempt { putSkill(name.trim(), description.trim(), body) }
                            if (saved != null && original != null && original != saved.name) vm.attempt { deleteSkill(original) }
                            busy = false
                            if (saved != null) { editing = null; load() }
                        }
                    }, Modifier.fillMaxWidth(), enabled = name.isNotBlank() && body.isNotBlank(), busy = busy)
                }
            }
        }
        return
    }
    Page("Skills", back, action = { IconButton(Lucide.Plus, { editing = null to Skill("") }, C.accentHi) }) {
        item { Hint("How-tos the team saved (skills/<name>/SKILL.md on the server). Agents see the names and load the instructions when a job needs them.") }
        val list = skills
        when {
            list == null -> item { Loading() }
            list.isEmpty() -> item { Empty("No skills yet. Agents save one when they learn how to do something well.") }
            else -> items(list, key = { it.name }) { s ->
                Card(Modifier.pressLong(onLong = {
                    overlays.menu(s.name, listOf(
                        MenuItem("Edit", Lucide.Pencil) { editing = s.name to s },
                        MenuItem("Delete skill", Lucide.Trash, danger = true, confirm = "Tap again to delete") {
                            scope.launch { vm.attempt { deleteSkill(s.name) }; load() }
                        },
                    ))
                }) { editing = s.name to s }) {
                    Txt(s.name, 14.5.sp, C.text, FontWeight.SemiBold, mono = true)
                    Txt(s.description.ifEmpty { "No description." }, 13.sp, C.muted, maxLines = 2)
                }
            }
        }
    }
}

// ---- Connections (MCP) ----------------------------------------------------------------

@Composable
private fun ConnectionsPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var servers by remember { mutableStateOf<List<McpServer>?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.attempt { mcp() }?.let { servers = it } }
    Page("Connections", back) {
        item { Hint("Services agents can use through MCP. Iris and agents with the Connections ability get their tools; calls that change something ask you first. Add or edit them on the server.") }
        val list = servers
        when {
            list == null -> item { Loading() }
            list.isEmpty() -> item { Empty("Nothing connected yet. Add services in Settings → Connections on the server.") }
            else -> {
                items(list, key = { it.name }) { s ->
                    Card {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                Txt(s.name, 15.sp, C.text, FontWeight.SemiBold)
                                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    when {
                                        !s.enabled -> Badge("Off", C.faint)
                                        s.connected -> Badge("Connected · ${s.tools.size} tools", C.success)
                                        else -> Badge("Not connected", C.danger)
                                    }
                                    if (s.autoApprove.contains("*")) Badge("Trusted", C.warn)
                                }
                            }
                            Switch(s.enabled, { on ->
                                scope.launch { vm.attempt { setMcpEnabled(s, on) }?.let { servers = it } }
                            })
                        }
                        Txt(if (s.transport == "http") s.url else (listOf(s.command) + s.args).joinToString(" "), 12.sp, C.faint, mono = true, maxLines = 2)
                        s.error?.let { Txt(it, 12.sp, C.danger, maxLines = 3) }
                    }
                }
                item {
                    Button("Reconnect all", {
                        busy = true
                        scope.launch { vm.attempt { refreshMcp() }?.let { servers = it }; busy = false }
                    }, Modifier.fillMaxWidth(), primary = false, busy = busy)
                }
            }
        }
    }
}

// ---- Activity -------------------------------------------------------------------------

@Composable
private fun ActivityPage(vm: IrisViewModel, back: () -> Unit) {
    var log by remember { mutableStateOf<List<ToolLogEntry>?>(null) }
    LaunchedEffect(Unit) { log = vm.attempt { audit(150) }.orEmpty() }
    Page("Activity", back, subtitle = "The latest tool use, newest first") {
        val list = log
        when {
            list == null -> item { Loading() }
            list.isEmpty() -> item { Empty("No tool use yet.") }
            else -> items(list, key = { it.id }) { e ->
                val (label, color) = when {
                    e.summary.contains("needs your approval") -> "waiting" to C.warn
                    e.status == "done" -> "ok" to C.success
                    e.status == "error" -> "failed" to C.danger
                    e.status == "denied" -> "denied" to C.warn
                    e.status == "cancelled" -> "stopped" to C.faint
                    else -> e.status to C.accentHi
                }
                Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Orb(vm.agent(e.agentId), 24.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Txt(e.summary.ifEmpty { e.tool }, 13.5.sp, C.text, maxLines = 2)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Badge(label, color)
                            Txt("${vm.agent(e.agentId)?.name ?: "Removed agent"} · ${relativeTime(e.startedAt)}", 11.5.sp, C.faint, maxLines = 1)
                        }
                    }
                }
            }
        }
    }
}

// ---- Devices ----------------------------------------------------------------------------

@Composable
private fun DevicesPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    var devices by remember { mutableStateOf<List<Device>?>(null) }
    suspend fun load() { vm.attempt { devices() }?.let { devices = it } }
    LaunchedEffect(Unit) { load() }
    Page("Devices", back) {
        item { Hint("Everything that can reach your agents. Remove a lost phone or an old browser here: it's signed out right away.") }
        val list = devices
        if (list == null) item { Loading() } else items(list, key = { it.id }) { d ->
            val me = d.name == vm.deviceName && !d.local
            Card {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Box(Modifier.size(34.dp).background(C.accentSoft, RoundedCornerShape(10.dp)), contentAlignment = Alignment.Center) {
                        val phone = Regex("phone|android|iphone|ipad|app\\)", RegexOption.IGNORE_CASE).containsMatchIn(d.name)
                        Icon(if (d.local) Lucide.Server else if (phone) Lucide.Smartphone else Lucide.Globe, C.accentHi, 17.dp)
                    }
                    Column(Modifier.weight(1f)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Txt(d.name, 14.5.sp, C.text, FontWeight.Medium, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
                            if (me) Badge("This phone", C.accentHi)
                            if (d.local) Badge("The server", C.faint)
                        }
                        Txt("Added ${when_(d.createdAt)} · seen ${d.lastSeenAt?.let { relativeTime(it) } ?: "never"}", 12.sp, C.faint, maxLines = 1)
                    }
                    if (!d.local) {
                        IconButton(Lucide.Trash, {
                            overlays.menu(
                                if (me) "Remove this phone? You'll need to pair it again." else "Remove ${d.name}?",
                                listOf(MenuItem(if (me) "Remove and unpair" else "Remove device", Lucide.Trash, danger = true) {
                                    scope.launch {
                                        vm.attempt { deleteDevice(d.id) } ?: return@launch
                                        if (me) vm.signOut() else load()
                                    }
                                }),
                            )
                        }, C.faint, 17.dp)
                    }
                }
            }
        }
    }
}

// ---- Remote access ----------------------------------------------------------------------

@Composable
private fun RemotePage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var link by remember { mutableStateOf<LinkStatus?>(null) }
    var pairing by remember { mutableStateOf<Pairing?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { link = vm.attempt { linkStatus() } }
    fun copy(text: String) {
        (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("pairing", text))
        vm.showToast("Copied")
    }
    Page("Remote access", back) {
        item {
            val l = link
            Card {
                CardTitle("I.R.I.S. Link", "Your server keeps one outgoing, end-to-end encrypted connection to your relay. Nothing on the server is open to the internet.")
                if (l == null) Loading() else {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Dot(if (l.connected) C.success else C.danger)
                        Txt(if (l.connected) "Connected" else if (l.enabled) "Not connected" else "Off", 14.sp, C.text, FontWeight.Medium)
                    }
                    Txt(l.url, 12.5.sp, C.muted, mono = true)
                    Txt(l.loginUser?.let { "Browser sign-in: $it" } ?: "No browser sign-in set", 12.5.sp, C.faint)
                    Hint("Turning the link off, the relay address and the sign-in are changed on the server (this phone talks through the link).")
                }
            }
        }
        item {
            val p = pairing
            Card {
                CardTitle("Pair another device", "A one-time code, valid for 10 minutes. Open the link on the other device, or type the code in its app.")
                if (p == null) {
                    Button("Get a pairing code", {
                        busy = true
                        scope.launch { pairing = vm.attempt { newPairing() }; busy = false }
                    }, icon = Lucide.QrCode, busy = busy)
                } else {
                    val left by produceState(p.expiresAt - System.currentTimeMillis(), p) {
                        while (value > 0) { delay(1000); value = p.expiresAt - System.currentTimeMillis() }
                    }
                    Txt(p.code.chunked(4).joinToString(" "), 30.sp, C.text, FontWeight.SemiBold, mono = true)
                    Txt(if (left > 0) "Expires in ${left / 60_000}:${"%02d".format(left / 1000 % 60)}" else "Expired", 12.5.sp, if (left > 0) C.faint else C.danger)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button("Copy link", { copy(p.url) }, icon = Lucide.Link, primary = false)
                        Button("Copy code", { copy(p.code) }, icon = Lucide.Copy, primary = false)
                    }
                }
            }
        }
    }
}

// ---- Providers & Accounts (read here, changed on the server) ---------------------------

@Composable
private fun ProvidersPage(vm: IrisViewModel, back: () -> Unit) {
    LaunchedEffect(Unit) { vm.loadModelSettings() }
    Page("Providers", back) {
        item { Hint("API keys never leave the server, so providers are added and changed there. The default model is picked in Models.") }
        items(vm.providers.toList(), key = { it.id }) { p ->
            Card {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Txt(p.name, 15.sp, C.text, FontWeight.SemiBold, modifier = Modifier.weight(1f))
                    if (p.id == vm.defaultModel?.provider) Badge("Default", C.accentHi)
                    Badge(if (p.hasKey) "Key saved" else "No key", if (p.hasKey) C.success else C.warn)
                }
                Txt(p.baseUrl, 12.sp, C.faint, mono = true, maxLines = 1)
                if (p.defaultModel.isNotEmpty()) Txt("Model: ${p.defaultModel}", 12.5.sp, C.muted)
            }
        }
    }
}

@Composable
private fun AccountsPage(vm: IrisViewModel, back: () -> Unit) {
    var accounts by remember { mutableStateOf<List<VaultAccount>?>(null) }
    LaunchedEffect(Unit) { accounts = vm.attempt { vault() }.orEmpty() }
    Page("Accounts", back) {
        item { Hint("Logins agents may use in their browser, only on that account's own site. Passwords stay encrypted on the server and never reach this phone; add or change them there.") }
        val list = accounts
        when {
            list == null -> item { Loading() }
            list.isEmpty() -> item { Empty("No saved logins.") }
            else -> items(list, key = { it.id }) { a ->
                Card {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Txt(a.name, 15.sp, C.text, FontWeight.SemiBold, modifier = Modifier.weight(1f))
                        Badge(if (a.hasPassword) "Password saved" else "No password", if (a.hasPassword) C.success else C.warn)
                    }
                    Txt("${a.username} · ${a.site}", 12.5.sp, C.muted, maxLines = 1)
                }
            }
        }
    }
}
