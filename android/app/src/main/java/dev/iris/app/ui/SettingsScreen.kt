package dev.iris.app.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.BuildConfig
import dev.iris.app.data.Fallback
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.Limits
import dev.iris.app.data.MemoryText
import dev.iris.app.data.REASONING_LEVELS
import dev.iris.app.data.ResponseStyle
import dev.iris.app.data.Screen
import dev.iris.app.data.SearchStatus
import dev.iris.app.data.formatTokens
import dev.iris.app.data.modelName
import kotlinx.coroutines.launch

/** The sections, in the desktop's order (Settings.svelte), minus Appearance, which only the desktop app has. */
private data class Section(val id: String, val title: String, val icon: androidx.compose.ui.graphics.vector.ImageVector, val hint: String)

private val SECTIONS = listOf(
    Section("notifications", "Notifications", Lucide.Bell, "Push to this phone, what gets sent"),
    Section("voice", "Voice", Lucide.Mic, "Speech, voices, calls"),
    Section("models", "Models", Lucide.Cpu, "Default model, thinking, auto mode, fallback"),
    Section("responses", "Personality & memory", Lucide.MessageText, "How agents talk, what they know about you"),
    Section("routines", "Routines", Lucide.Calendar, "What agents do on a schedule"),
    Section("skills", "Skills", Lucide.BookOpen, "How-tos the team saved"),
    Section("connections", "Connections", Lucide.Plug, "Services connected over MCP"),
    Section("limits", "Limits", Lucide.Gauge, "Daily and weekly token caps"),
    Section("search", "Search", Lucide.Search, "Which web search agents use"),
    Section("activity", "Activity", Lucide.Activity, "Every tool the agents used"),
    Section("devices", "Devices", Lucide.Smartphone, "Phones and browsers with access"),
    Section("remote", "Remote access", Lucide.Globe, "The relay link, pairing a device"),
    Section("providers", "Providers", Lucide.Server, "AI services and their keys"),
    Section("accounts", "Accounts", Lucide.KeyRound, "Logins agents may use"),
    Section("about", "About", Lucide.Info, "This app, unpairing"),
)

@Composable
fun SettingsScreen(vm: IrisViewModel, page: String?) {
    val back = { vm.back(); Unit }
    when (page) {
        null -> Page("Settings", back, subtitle = "On your server · changes apply right away") {
            item {
                Card(padding = false) {
                    Column {
                        SECTIONS.forEachIndexed { i, s ->
                            if (i > 0) Divider(inset = true)
                            NavRow(s.icon, s.title, s.hint) { vm.open(Screen.Settings(s.id)) }
                        }
                    }
                }
            }
        }
        "models" -> ModelsPage(vm, back)
        "responses" -> ResponsesPage(vm, back)
        "limits" -> LimitsPage(vm, back)
        "search" -> SearchPage(vm, back)
        "about" -> AboutPage(vm, back)
        "notifications" -> NotificationsPage(vm, back)
        "voice" -> VoicePage(vm, back)
        "problems" -> ProblemsPage(vm, back)
        else -> SettingsPage(vm, page, back)
    }
}

// ---- Models ----------------------------------------------------------------------------

@Composable
private fun ModelsPage(vm: IrisViewModel, back: () -> Unit) {
    val overlays = LocalOverlays.current
    val scope = rememberCoroutineScope()
    var reasoning by remember { mutableStateOf<String?>(null) }
    var auto by remember { mutableStateOf<String?>(null) }
    var fallback by remember { mutableStateOf<Fallback?>(null) }
    var saving by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        vm.loadModelSettings()
        reasoning = vm.attempt { reasoning() }?.effort
        auto = vm.attempt { autoDefault() }?.mode
        fallback = vm.attempt { fallback() }
    }
    val dm = vm.defaultModel
    val providerName = { id: String -> vm.providers.firstOrNull { it.id == id }?.name ?: id }

    /** Picks a provider (when there are several), then one of its live models. */
    fun pickModel(title: String, provider: String, current: String?, onPick: (String, String) -> Unit) {
        fun chooseModel(p: String) = scope.launch {
            val list = vm.attempt { models(p) }.orEmpty()
            if (list.isEmpty()) vm.showToast("${providerName(p)} didn't list any models")
            else overlays.picker(title, list.map { it to modelName(it).let { n -> if (n != it) "$n · $it" else it } }, current) { onPick(p, it) }
        }
        if (vm.chatProviders.size > 1) {
            overlays.picker("Provider", vm.chatProviders.map { it.id to it.name }, provider) { chooseModel(it) }
        } else chooseModel(provider)
    }

    Page("Models", back) {
        item {
            Card(accent = true) {
                CardTitle("Default model", "Every agent uses this model unless you give it its own (in the agent's settings).")
                SelectField(null, dm?.let { "${modelName(it.model)} · ${it.providerName}" } ?: "Loading…", onClick = {
                    if (dm != null) pickModel("Model for every agent", dm.provider, dm.model) { p, m ->
                        scope.launch { vm.setDefaultModel(p, m) }
                    }
                })
                Divider()
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Label("Reasoning effort")
                    Chips(listOf("" to "Auto") + REASONING_LEVELS, reasoning) { level ->
                        reasoning = level
                        scope.launch { vm.attempt { putReasoning(level) }?.let { reasoning = it.effort } }
                    }
                    Hint("How hard all agents think, including Iris. Higher levels answer more carefully but are slower and use more tokens.")
                }
                Divider()
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Label("Auto mode for new agents")
                    ChoiceCards(AUTO_MODES, auto ?: "") { mode ->
                        auto = mode
                        scope.launch { vm.attempt { putAutoDefault(mode) }?.let { auto = it.mode } }
                    }
                    Hint("New orbs and agents Iris creates start like this. Paying, installing and other risky steps always ask.")
                }
            }
        }
        item {
            val f = fallback
            Card {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Column(Modifier.weight(1f)) {
                        CardTitle("Fallback model", "If the main provider fails (down, out of credit, rate limited), the reply is retried on this one.")
                    }
                    Switch(f?.enabled == true, { on ->
                        val next = (f ?: Fallback()).copy(enabled = on)
                        fallback = next
                        if (!on || next.model.isNotEmpty()) scope.launch { vm.attempt { putFallback(next) }?.let { fallback = it } }
                    })
                }
                AnimatedVisibility(f?.enabled == true) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        val provider = f?.provider?.ifEmpty { null } ?: dm?.provider.orEmpty()
                        SelectField("Model", f?.model?.takeIf { it.isNotEmpty() }?.let { "${modelName(it)} · ${providerName(provider)}" } ?: "Choose a model", onClick = {
                            pickModel("Fallback model", provider, f?.model) { p, m ->
                                val next = Fallback(true, p, m)
                                fallback = next
                                saving = true
                                scope.launch {
                                    vm.attempt { putFallback(next) }?.let { fallback = it; vm.showToast("Fallback: ${modelName(it.model)}") }
                                    saving = false
                                }
                            }
                        })
                        if (f != null && f.provider == dm?.provider) {
                            Hint("Same provider as the default model: this only helps if that one model fails. Add another provider on the server for real backup.", C.warn)
                        }
                    }
                }
            }
        }
    }
}

val AUTO_MODES = listOf(
    Triple("off", "Off", "Every command asks you first."),
    Triple("careful", "Careful", "An AI check approves safe commands."),
    Triple("full", "Full auto", "Only catastrophic commands ask you."),
)

// ---- Personality & memory --------------------------------------------------------------

private val STYLES = listOf(
    Triple("balanced", "Balanced", "Clear and friendly, with detail when it helps."),
    Triple("concise", "Straight to the point", "The answer first. No fluff, no recap."),
    Triple("analyst", "Deep analyst", "Breaks it down, weighs options, risks and unknowns."),
    Triple("witty", "Witty", "Banter and dry humor, still useful and correct."),
    Triple("teacher", "Teacher", "Step by step in plain words, with examples."),
    Triple("professional", "Professional", "Polished and structured, ready to forward."),
)

@Composable
private fun ResponsesPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var saved by remember { mutableStateOf<ResponseStyle?>(null) }
    var preset by remember { mutableStateOf("balanced") }
    var custom by remember { mutableStateOf("") }
    var memory by remember { mutableStateOf<MemoryText?>(null) }
    var memoryText by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        vm.attempt { style() }?.let { saved = it; preset = it.preset; custom = it.custom }
        vm.attempt { userMemory() }?.let { memory = it; memoryText = it.text }
    }
    Page("Personality & memory", back) {
        item {
            Card {
                CardTitle("How agents talk to you", "Applies to every agent you created. Agents Iris creates talk to her, so they keep it short.")
                ChoiceCards(STYLES, preset) { preset = it }
                Label("In your own words (optional)")
                TextArea(custom, { custom = it }, "e.g. Use metric units. Answer in Greek when I write in Greek.", minHeight = 90, maxLength = 2000)
                val dirty = saved != null && (preset != saved?.preset || custom.trim() != saved?.custom)
                Button("Save", {
                    busy = true
                    scope.launch {
                        vm.attempt { putStyle(ResponseStyle(preset, custom.trim())) }?.let { saved = it; vm.showToast("Saved: agents talk this way from the next reply") }
                        busy = false
                    }
                }, enabled = dirty, busy = busy)
            }
        }
        item {
            val m = memory
            Card {
                CardTitle("What agents know about you", "USER.md: every agent reads it. They add to it themselves when you tell them something worth remembering.")
                TextArea(memoryText, { memoryText = it }, "Nothing yet.", minHeight = 140, mono = true, maxLength = m?.limit)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Txt("${memoryText.trim().length} / ${m?.limit ?: "…"}", 12.sp, C.faint, modifier = Modifier.weight(1f))
                    Button("Save memory", {
                        scope.launch { vm.attempt { putUserMemory(memoryText) }?.let { memory = it; memoryText = it.text; vm.showToast("Memory saved") } }
                    }, enabled = m != null && memoryText != m.text)
                }
            }
        }
    }
}

// ---- Limits ------------------------------------------------------------------------------

private val DAILY = listOf(0L, 100_000L, 250_000L, 500_000L, 1_000_000L, 2_000_000L)
private val WEEKLY = listOf(0L, 500_000L, 1_000_000L, 2_000_000L, 5_000_000L, 10_000_000L)

@Composable
private fun LimitsPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var limits by remember { mutableStateOf<Limits?>(null) }
    var daily by remember { mutableLongStateOf(0L) }
    var weekly by remember { mutableLongStateOf(0L) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.attempt { limits() }?.let { limits = it; daily = it.dailyTokens; weekly = it.weeklyTokens } }
    Page("Limits", back) {
        item { Hint("Caps for all agents together. When one is reached, agents stop until it resets (days at midnight, weeks on Monday) or you raise it.") }
        for ((label, presets, row) in listOf(Triple("Daily limit", DAILY, 0), Triple("Weekly limit", WEEKLY, 1))) {
            item(label) {
                val value = if (row == 0) daily else weekly
                Card {
                    UsageBar(label, if (row == 0) limits?.usedToday ?: 0 else limits?.usedThisWeek ?: 0, value, if (row == 0) "Used today" else "Used this week")
                    Chips(presets.map { it.toString() to if (it == 0L) "Off" else formatTokens(it) }, value.toString()) {
                        if (row == 0) daily = it.toLong() else weekly = it.toLong()
                    }
                    Field(
                        if (value == 0L || value in presets) "" else value.toString(),
                        { t -> val n = t.filter(Char::isDigit).take(10).toLongOrNull() ?: 0L; if (row == 0) daily = n else weekly = n },
                        placeholder = "Custom (tokens)",
                        keyboard = KeyboardOptions(keyboardType = KeyboardType.Number),
                    )
                }
            }
        }
        item {
            val l = limits
            Button("Save limits", {
                busy = true
                scope.launch {
                    vm.attempt { putLimits(daily, weekly) }?.let { limits = it; vm.showToast("Limits saved") }
                    busy = false
                }
            }, Modifier.fillMaxWidth(), enabled = l != null && (daily != l.dailyTokens || weekly != l.weeklyTokens), busy = busy)
        }
    }
}

// ---- Search ------------------------------------------------------------------------------

@Composable
private fun SearchPage(vm: IrisViewModel, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf<SearchStatus?>(null) }
    var provider by remember { mutableStateOf("auto") }
    var key by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.attempt { search() }?.let { status = it; provider = it.provider } }
    Page("Search", back) {
        item {
            Card {
                CardTitle("Web search", status?.let { "Agents search with ${if (it.active == "tavily") "Tavily" else "DuckDuckGo"} right now." })
                ChoiceCards(
                    listOf(
                        Triple("auto", "Automatic", "Tavily when a key is saved, otherwise DuckDuckGo."),
                        Triple("duckduckgo", "DuckDuckGo", "Free, no key. Works, but can rate-limit busy agents."),
                        Triple("tavily", "Tavily", "Built for AI agents. Free plan: 1,000 searches a month, no card."),
                    ),
                    provider,
                ) { provider = it }
                Label(if (status?.hasTavilyKey == true) "Tavily key · saved (leave empty to keep it)" else "Tavily key")
                Field(key, { key = it.trim() }, placeholder = "tvly-…", password = true, keyboard = KeyboardOptions(keyboardType = KeyboardType.Password))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button("Save", {
                        busy = true
                        scope.launch {
                            vm.attempt { putSearch(provider, key.ifEmpty { null }) }?.let { status = it; key = ""; vm.showToast("Search saved") }
                            busy = false
                        }
                    }, enabled = status != null && (provider != status?.provider || key.isNotEmpty()), busy = busy)
                    if (status?.hasTavilyKey == true) {
                        Button("Remove key", {
                            scope.launch { vm.attempt { putSearch(provider, "") }?.let { status = it; vm.showToast("Tavily key removed") } }
                        }, primary = false)
                    }
                }
            }
        }
    }
}

// ---- About -------------------------------------------------------------------------------

@Composable
private fun AboutPage(vm: IrisViewModel, back: () -> Unit) {
    val overlays = LocalOverlays.current
    Page("About", back) {
        item {
            Card {
                CardTitle("I.R.I.S. for Android", "Version ${BuildConfig.VERSION_NAME}. Everything runs on your server; this app only shows it, through the end-to-end encrypted link.")
                Row { Txt("This device", 13.5.sp, C.muted, modifier = Modifier.weight(1f)); Txt(vm.deviceName, 13.5.sp, C.text, FontWeight.Medium) }
                vm.defaultModel?.let { Row { Txt("Default model", 13.5.sp, C.muted, modifier = Modifier.weight(1f)); Txt(modelName(it.model), 13.5.sp, C.text, FontWeight.Medium) } }
            }
        }
        item {
            Card {
                val offer = vm.phoneUpdate
                val progress = vm.updateProgress
                if (offer != null) {
                    val (release, _) = offer
                    CardTitle("Version ${release.version} is out", "You have ${BuildConfig.VERSION_NAME}. The update is downloaded from GitHub, checked against its checksum, and then Android asks you to confirm the install. Your chats and pairing stay.")
                    if (release.notes.isNotBlank()) Txt(release.notes, 12.5.sp, C.muted)
                    Button(
                        if (progress != null) "Downloading… $progress%" else "Download and install",
                        { vm.installUpdate() },
                        icon = Lucide.Download,
                        enabled = progress == null,
                    )
                } else {
                    val latest = vm.update?.latest
                    CardTitle(
                        "Updates",
                        when {
                            vm.update == null -> "Looking for updates needs the server to answer."
                            latest != null -> "You have the newest version (${BuildConfig.VERSION_NAME})."
                            else -> "No published release was found yet."
                        },
                    )
                    Button("Check now", { vm.checkUpdate(true) }, icon = Lucide.Info, primary = false)
                }
                vm.updateNote?.let { Txt(it, 12.5.sp, C.danger) }
            }
        }
        item {
            Card {
                CardTitle("Unpair this phone", "Removes the pairing from this phone. To revoke it on the server too, remove it in Devices.")
                Button("Unpair", {
                    overlays.menu("Unpair this phone?", listOf(MenuItem("Unpair", Lucide.LogOut, danger = true) { vm.signOut() }))
                }, icon = Lucide.LogOut, primary = false)
            }
        }
        item { Txt("MIT license · Lucide icons (ISC)", 11.5.sp, C.faint, modifier = Modifier.padding(horizontal = 4.dp)) }
    }
}
