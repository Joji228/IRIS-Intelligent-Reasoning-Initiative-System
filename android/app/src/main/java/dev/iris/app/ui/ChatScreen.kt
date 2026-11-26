package dev.iris.app.ui

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.Image
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.FileProvider
import dev.iris.app.data.Upload
import dev.iris.app.data.Uploads
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.Agent
import dev.iris.app.data.Conn
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.OrbState
import dev.iris.app.data.Prefixes
import dev.iris.app.data.modelName
import dev.iris.app.data.orbState
import dev.iris.app.data.phrases
import dev.iris.app.data.reasoningLabel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Composable
fun ChatScreen(vm: IrisViewModel, agent: Agent) {
    val threadId = vm.thread?.id
    val running = vm.isRunning(threadId)
    var draft by rememberSaveable(agent.id) { mutableStateOf("") }

    Column(Modifier.fillMaxSize()) {
        Header(vm, agent)
        Box(Modifier.fillMaxWidth().height(1.dp).background(C.line))

        Box(Modifier.weight(1f).fillMaxWidth()) {
            Messages(vm, agent) { draft = it }
        }

        // Queue: messages waiting for a busy agent.
        val queue = threadId?.let { vm.queues[it] }.orEmpty()
        AnimatedVisibility(
            threadId != null && queue.isNotEmpty(),
            enter = slideInVertically { it / 2 } + fadeIn(), exit = slideOutVertically { it / 2 } + fadeOut(),
        ) {
            Column(
                Modifier.padding(horizontal = 10.dp).padding(bottom = 6.dp).fillMaxWidth()
                    .background(C.surface.copy(alpha = 0.92f), RoundedCornerShape(Radius.md))
                    .heightIn(max = 170.dp)
                    .verticalScroll(rememberScrollState()).padding(6.dp),
            ) {
                Row(Modifier.padding(horizontal = 6.dp, vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Lucide.ListOrdered, C.faint, 12.dp)
                    Txt("${queue.size} queued · ${if (running) "sends when ${agent.name} finishes" else "sending…"}", 11.5.sp, C.faint)
                }
                queue.forEachIndexed { i, q ->
                    Row(Modifier.padding(horizontal = 6.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Txt("${i + 1}", 11.sp, C.faint)
                        Txt(q.content.ifEmpty { "Files" }, 13.5.sp, C.text, maxLines = 1, modifier = Modifier.weight(1f))
                        Row(
                            Modifier.background(C.accentSoft, RoundedCornerShape(50))
                                .press { threadId?.let { vm.steerQueued(it, q.id) } }.padding(horizontal = 14.dp, vertical = 8.dp),
                            horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(Lucide.CornerDownRight, C.accentHi, 12.dp)
                            Txt("Steer", 12.sp, C.accentHi, FontWeight.Medium)
                        }
                        IconButton(Lucide.X, { threadId?.let { vm.removeQueued(it, q.id) } }, size = 16.dp)
                    }
                }
            }
        }

        // Compaction in progress (on its own at the limit, or from /compact or the menu).
        AnimatedVisibility(threadId != null && vm.compacting[threadId] == true, enter = fadeIn(), exit = fadeOut()) {
            Box(Modifier.fillMaxWidth().padding(top = 6.dp), contentAlignment = Alignment.Center) {
                Row(
                    Modifier.background(C.accentSoft, RoundedCornerShape(50))
                        .padding(horizontal = 14.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Spinner(13.dp)
                    Shimmer("Compacting the chat · summarizing older messages", 12.5.sp, FontWeight.Normal)
                }
            }
        }
        Composer(vm, agent, draft, { draft = it }, running, enabled = vm.conn == Conn.Online)
    }
}

@Composable
private fun Header(vm: IrisViewModel, agent: Agent) {
    val overlays = LocalOverlays.current
    val working = vm.isWorking(agent.id)
    val waiting = vm.isWaiting(agent.id)
    // The default effort, so the button shows the level the agent really uses ("High"), not "Auto".
    LaunchedEffect(Unit) { if (vm.defaultModel == null || vm.reasoningDefault.isEmpty()) vm.loadModelSettings() }
    // Just back, who you're talking to, call and ⋯: the model, thinking effort and command
    // approvals live in the ⋯ menu, so the name has room on a phone.
    Row(
        Modifier.fillMaxWidth().height(64.dp).padding(end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(Lucide.ChevronLeft, { vm.back() }, C.muted, 24.dp)
        AgentOrb(vm, agent, 32.dp)
        Column(Modifier.weight(1f).padding(start = 10.dp, end = 8.dp)) {
            Txt(agent.name, 16.sp, C.text, FontWeight.SemiBold, maxLines = 1)
            AnimatedContent(
                when { waiting -> 1; working -> 2; else -> 0 },
                transitionSpec = { fadeIn(tween(200)) togetherWith fadeOut(tween(120)) },
                label = "status",
            ) { s ->
                when (s) {
                    1 -> Txt("Waiting for your approval", 13.sp, C.warn, FontWeight.Medium, maxLines = 1)
                    // The exact step when there is one ("Searching the web for “…”"), else the plain phrase.
                    2 -> Shimmer(vm.detailOf(agent.id).ifEmpty { vm.statusLine(agent.id) }, 13.sp, FontWeight.Normal)
                    else -> Txt(agent.description.ifEmpty { "Ready" }, 13.sp, C.faint, maxLines = 1)
                }
            }
        }
        CallButton(vm, agent)
        IconButton(Lucide.Ellipsis, { chatMenu(vm, overlays, agent) }, C.muted, 22.dp)
    }
}

/** The chat's ⋯ menu: this agent's model and approvals first, then the usual agent actions. */
private fun chatMenu(vm: IrisViewModel, overlays: Overlays, agent: Agent) {
    val auto = autoMode(agent)
    agentMenu(
        vm, overlays, agent, inChat = true,
        extra = listOf(
            MenuItem(
                "Model & thinking", Lucide.Brain,
                value = reasoningLabel(agent.reasoningEffort.ifEmpty { vm.reasoningDefault }),
            ) { modelSheet(vm, overlays, agent) },
            MenuItem(
                "Command approvals",
                when (auto) { "full" -> Lucide.Zap; "careful" -> Lucide.ShieldCheck; else -> Lucide.ShieldOff },
                value = AUTO_MODES.firstOrNull { it.first == auto }?.second,
            ) { autoSheet(vm, overlays, agent) },
        ),
    )
}

private fun autoMode(a: Agent) = if (!a.autoApprove) "off" else if (a.autoFull) "full" else "careful"

/** The model and how hard this agent thinks. */
private fun modelSheet(vm: IrisViewModel, overlays: Overlays, agent: Agent) = overlays.show { dismiss ->
    val scope = rememberCoroutineScope()
    val a = vm.agent(agent.id) ?: agent
    LaunchedEffect(Unit) { vm.loadModelSettings() }
    val own = a.provider.isNotEmpty() && a.model.isNotEmpty()
    val dm = vm.defaultModel
    val providerName = { id: String -> vm.providers.firstOrNull { it.id == id }?.name ?: id }
    SheetTitle("${a.name} · model")
    Column(Modifier.padding(horizontal = 18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SelectField(
            null,
            if (own) "${modelName(a.model)} · ${providerName(a.provider)}"
            else dm?.let { "Default: ${modelName(it.model)}" } ?: "Default model",
            onClick = {
                fun choose(p: String) = scope.launch {
                    val list = vm.attempt { models(p) }.orEmpty()
                    if (list.isEmpty()) vm.showToast("${providerName(p)} didn't list any models")
                    else {
                        val options = buildList {
                            add("" to "Use the default model")
                            list.forEach { add(it to modelName(it).let { n -> if (n != it) "$n · $it" else it }) }
                        }
                        overlays.picker("Model for ${a.name}", options, if (own) a.model else "") { m ->
                            vm.patchAgent(
                                a.id,
                                buildJsonObject { put("provider", if (m.isEmpty()) "" else p); put("model", m) },
                                "${a.name} now uses ${if (m.isEmpty()) "the default model" else modelName(m)}",
                            )
                        }
                    }
                }
                val start = a.provider.ifEmpty { dm?.provider.orEmpty() }
                if (vm.chatProviders.size > 1) overlays.picker("Provider", vm.chatProviders.map { it.id to it.name }, start) { choose(it) }
                else if (start.isNotEmpty()) choose(start)
            },
        )
        Label("Thinking effort")
        // "" here means this agent follows the default level, hence "Default" on the pill.
        EffortSlider(a.reasoningEffort, { level ->
            vm.patchAgent(a.id, buildJsonObject { put("reasoning_effort", level) })
        }, autoLabel = "Default", autoLevel = vm.reasoningDefault,
            autoHint = "Like all agents" + vm.reasoningDefault.takeIf { it.isNotEmpty() }?.let { " (${reasoningLabel(it)})" }.orEmpty())
        Hint("Higher levels answer more carefully, but are slower and use more tokens. Default follows Settings → Models.")
        Spacer(Modifier.height(4.dp))
    }
}

/** Which commands this agent may run without asking. */
private fun autoSheet(vm: IrisViewModel, overlays: Overlays, agent: Agent) = overlays.show { _ ->
    val a = vm.agent(agent.id) ?: agent
    SheetTitle("${a.name} · command approvals")
    Column(Modifier.padding(horizontal = 18.dp).padding(bottom = 6.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        ChoiceCards(AUTO_MODES, autoMode(a)) { mode ->
            vm.patchAgent(a.id, buildJsonObject { put("auto_approve", mode != "off"); put("auto_full", mode == "full") })
        }
        Hint("Paying, installing and other risky steps always ask.")
    }
}

@Composable
private fun BoxScope.Messages(vm: IrisViewModel, agent: Agent, fill: (String) -> Unit) {
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val messages = vm.messages
    // Newest first in a reversed list: the view stays on the latest text while it streams in,
    // and stays put once the user scrolls up to read.
    val away by remember { derivedStateOf { list.firstVisibleItemIndex > 0 || list.firstVisibleItemScrollOffset > 200 } }

    // Sending a message always brings the view back down.
    val lastUserId = messages.lastOrNull { it.role == "user" }?.id
    LaunchedEffect(lastUserId) { if (lastUserId != null) list.animateScrollToItem(0) }

    when {
        vm.loadingThread && messages.isEmpty() -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Spinner(22.dp, C.faint) }
        messages.isEmpty() -> EmptyChat(vm, agent, fill)
        else -> {
            val summaryUntil = vm.thread?.summaryUntil ?: 0
            val firstKept = if (summaryUntil > 0 && vm.thread?.summary?.isNotEmpty() == true) {
                messages.indexOfFirst { it.createdAt > summaryUntil }
            } else -1
            LazyColumn(
                Modifier.fillMaxSize(),
                state = list,
                reverseLayout = true,
                contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 14.dp, bottom = 14.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp, Alignment.Bottom),
            ) {
                val n = messages.size
                itemsIndexed(messages.asReversed(), key = { _, m -> m.id }) { r, m ->
                    val i = n - 1 - r
                    val fresh = remember(m.id) { System.currentTimeMillis() - m.createdAt < 5000 }
                    Column(
                        Modifier.animateItem(fadeInSpec = null, placementSpec = spring(stiffness = 600f), fadeOutSpec = tween(150))
                            .appear(fresh, fromEnd = m.role == "user"),
                        verticalArrangement = Arrangement.spacedBy(20.dp),
                    ) {
                        if (i == 0 || startOfDay(messages[i - 1].createdAt) != startOfDay(m.createdAt)) DayDivider(m.createdAt)
                        if (i == firstKept) CompactedDivider(vm.thread?.summary.orEmpty(), firstKept)
                        MessageItem(vm, m, agent, i == n - 1)
                    }
                }
            }
            AnimatedVisibility(
                away, Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp),
                enter = fadeIn() + scaleIn(initialScale = 0.8f), exit = fadeOut() + scaleOut(targetScale = 0.8f),
            ) {
                Row(
                    Modifier.background(C.elevated, RoundedCornerShape(50))
                        .press { scope.launch { list.animateScrollToItem(0) } }.padding(horizontal = 16.dp, vertical = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Lucide.ArrowDown, C.muted, 15.dp)
                    Txt("Latest", 12.5.sp, C.muted)
                }
            }
        }
    }
}

/** New messages rise into place (from their own side) instead of popping in. */
@Composable
private fun Modifier.appear(fresh: Boolean, fromEnd: Boolean): Modifier {
    if (!fresh) return this
    val k = remember { Animatable(0f) }
    LaunchedEffect(Unit) { k.animateTo(1f, spring(dampingRatio = 0.78f, stiffness = 420f)) }
    return graphicsLayer {
        val v = k.value
        alpha = v.coerceIn(0f, 1f)
        translationY = (1 - v) * 18.dp.toPx()
        scaleX = 0.94f + 0.06f * v
        scaleY = 0.94f + 0.06f * v
        transformOrigin = TransformOrigin(if (fromEnd) 1f else 0f, 1f)
    }
}

@Composable
private fun DayDivider(ms: Long) {
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Txt(
            dayLabel(ms), 11.5.sp, C.muted, FontWeight.Medium,
            Modifier.background(C.surface, RoundedCornerShape(50))
                .padding(horizontal = 12.dp, vertical = 3.dp),
        )
    }
}

@Composable
private fun BoxScope.EmptyChat(vm: IrisViewModel, agent: Agent, fill: (String) -> Unit) {
    // Starters that fit the agent: Iris runs the team; a specialist does its job.
    val suggestions = when (agent.role) {
        "new" -> listOf(
            "Keep an eye on this server: disk space, updates and errors.",
            "Check every morning whether my websites are up and fast.",
            "Research topics in depth and give me sourced summaries.",
            "Help me write and polish emails.",
        )
        "main" -> listOf(
            "What can you and the team do for me?",
            "What do you remember about me?",
            "Put a small team on researching a topic for me.",
            "Which model are you running on?",
        )
        else -> listOfNotNull(
            "What can you help me with?",
            agent.description.takeIf { it.isNotEmpty() }?.let { "Go ahead and do your job: $it" },
            "What do you remember from earlier?",
        )
    }
    Column(
        Modifier.align(Alignment.Center).verticalScroll(rememberScrollState()).padding(horizontal = 18.dp, vertical = 20.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        AgentOrb(vm, agent, 76.dp)
        Spacer(Modifier.height(20.dp))
        Txt(if (agent.role == "new") "What should I do?" else "How can ${agent.name} help?", 22.sp, C.text, FontWeight.SemiBold)
        if (agent.description.isNotEmpty()) {
            BasicText(agent.description, Modifier.padding(top = 6.dp), style(14.sp, C.muted, lineHeight = 20.sp).copy(textAlign = TextAlign.Center))
        }
        Spacer(Modifier.height(24.dp))
        for (s in suggestions) {
            Txt(
                s, 14.sp, C.muted, lineHeight = 20.sp,
                modifier = Modifier.fillMaxWidth().padding(vertical = 5.dp)
                    .background(C.bubble, RoundedCornerShape(Radius.md))
                    .press { fill(s) }.padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
    }
}

@Composable
private fun CompactedDivider(summary: String, before: Int) {
    var open by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // A full-width divider in the accent color, so it's easy to spot.
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.weight(1f).height(1.dp).background(C.accent.copy(alpha = 0.35f)))
            Row(
                Modifier.clip(RoundedCornerShape(50)).press { open = !open }.background(C.accentSoft)
                    .padding(horizontal = 12.dp, vertical = 7.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(Lucide.Archive, C.accentHi, 13.dp)
                Txt("Compacted · $before earlier · ${if (open) "hide" else "show"}", 12.sp, C.accentHi, FontWeight.Medium)
            }
            Box(Modifier.weight(1f).height(1.dp).background(C.accent.copy(alpha = 0.35f)))
        }
        AnimatedVisibility(open) {
            Box(Modifier.fillMaxWidth().background(C.surface, RoundedCornerShape(Radius.md)).padding(14.dp)) {
                Markdown(summary, color = C.muted, size = 13.5f)
            }
        }
    }
}

/** The message box: one rounded pill (research, text, send). */
@Composable
private fun Composer(
    vm: IrisViewModel,
    agent: Agent,
    text: String,
    onText: (String) -> Unit,
    running: Boolean,
    enabled: Boolean,
) {
    val context = LocalContext.current
    val overlays = LocalOverlays.current
    val scope = rememberCoroutineScope()
    var research by rememberSaveable(agent.id) { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    // Files waiting to go with the next message, and whether a send is uploading them.
    val files = remember(agent.id) { mutableStateListOf<Upload>() }
    var reading by remember { mutableStateOf(0) }
    var sending by remember { mutableStateOf(false) }
    val haptics = LocalHapticFeedback.current
    val canSend = (text.isNotBlank() || files.isNotEmpty()) && enabled && !sending && reading == 0
    // No outline: the box just lifts a shade while you type.
    val fill by animateColorAsState(if (focused) C.elevated else C.surface, tween(200), label = "fill")

    fun add(uris: List<Uri>) {
        val room = Uploads.MAX_FILES - files.size
        if (uris.size > room) vm.showToast("Up to ${Uploads.MAX_FILES} files per message")
        for (uri in uris.take(room.coerceAtLeast(0))) {
            reading++
            scope.launch {
                try {
                    files.add(Uploads.read(context, uri))
                } catch (e: Uploads.TooBig) {
                    vm.showToast(e.message ?: "That file is too large")
                } catch (e: Exception) {
                    vm.showToast("Couldn't read that file")
                } finally {
                    reading--
                }
            }
        }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(Uploads.MAX_FILES)) { add(it) }
    val documents = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { add(it) }
    var shot by rememberSaveable { mutableStateOf<String?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { ok ->
        shot?.let { if (ok) add(listOf(Uri.parse(it))) }
        shot = null
    }
    fun attach() = overlays.menu(
        "Attach",
        listOf(
            MenuItem("Photos", Lucide.Image) {
                photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            },
            MenuItem("Take a photo", Lucide.Camera) {
                val dir = java.io.File(context.cacheDir, "camera").apply { mkdirs() }
                val file = java.io.File(dir, "photo-${System.currentTimeMillis()}.jpg")
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
                shot = uri.toString()
                runCatching { camera.launch(uri) }.onFailure { vm.showToast("No camera app found") }
            },
            MenuItem("Files", Lucide.FileText) { documents.launch(arrayOf("*/*")) },
        ),
    )

    fun submit() {
        if (!canSend) return
        val typed = text.trim()
        // Slash commands run here instead of being sent.
        if (typed.equals("/compact", ignoreCase = true) && files.isEmpty()) {
            onText("")
            vm.compactChat(agent.id)
            return
        }
        val body = if (research && typed.isNotEmpty()) "${Prefixes.RESEARCH} $typed" else typed
        val sendingFiles = files.toList()
        haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
        onText("")
        files.clear()
        research = false
        sending = true
        vm.send(agent, body, sendingFiles) { ok ->
            sending = false
            // Keep what was typed and attached if it didn't go through.
            if (!ok) {
                onText(text)
                files.addAll(0, sendingFiles)
            }
        }
    }

    // One line: [+] [research] text [send] in a single pill. Once the text needs a second
    // line, it takes the full width and the buttons move to a row under it (like other chat
    // apps), instead of sitting at the bottom left with an empty block above them. Whether it
    // fits is measured at the one-line width, so the layout never flips back and forth.
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    var rowWidth by remember { mutableIntStateOf(0) }
    var researchWidth by remember { mutableIntStateOf(0) }
    val multi = remember(text, rowWidth, researchWidth, density) {
        text.contains('\n') || (rowWidth > 0 && text.isNotEmpty() && run {
            val fixed = with(density) { (COMPOSER_BTN * 2 + FIELD_PAD * 2).roundToPx() } + researchWidth
            measurer.measure(text, COMPOSER_TEXT, constraints = Constraints(maxWidth = (rowWidth - fixed).coerceAtLeast(1))).lineCount > 1
        })
    }

    @Composable
    fun PlusButton() {
        Box(
            Modifier.size(COMPOSER_BTN).clip(CircleShape).background(Color.White.copy(alpha = 0.06f), CircleShape).press(onClick = ::attach),
            contentAlignment = Alignment.Center,
        ) { Icon(Lucide.Plus, C.muted, 20.dp) }
    }

    @Composable
    fun ResearchButton() {
        // Deep research: an icon, which grows a label while it's on.
        Row(
            Modifier.padding(start = 4.dp).height(COMPOSER_BTN).widthIn(min = COMPOSER_BTN).clip(CircleShape)
                .background(if (research) C.accentSoft else Color.Transparent, CircleShape)
                .press { research = !research; haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove) }
                .onSizeChanged { researchWidth = it.width }
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center,
        ) {
            Icon(Lucide.Telescope, if (research) C.accentHi else C.muted, 20.dp)
            AnimatedVisibility(research, enter = expandHorizontally() + fadeIn(), exit = shrinkHorizontally() + fadeOut()) {
                Txt("Research", 13.sp, C.accentHi, FontWeight.Medium, Modifier.padding(start = 6.dp))
            }
        }
    }

    @Composable
    fun SendButton() {
        val stop = running && text.isBlank() && files.isEmpty() && !sending
        val bg by animateColorAsState(
            if (stop) C.text else if (canSend || sending) C.accent else Color.White.copy(alpha = 0.06f), tween(180), label = "send",
        )
        Box(
            Modifier.size(COMPOSER_BTN).clip(CircleShape).background(bg, CircleShape)
                .press(stop || canSend) { if (stop) vm.stop() else submit() },
            contentAlignment = Alignment.Center,
        ) {
            AnimatedContent(
                if (sending) 2 else if (stop) 1 else 0,
                transitionSpec = { fadeIn(tween(150)) togetherWith fadeOut(tween(100)) },
                label = "sendIcon",
            ) { st ->
                when (st) {
                    2 -> Spinner(18.dp, C.onAccent)
                    1 -> Icon(Lucide.Square, C.bg, 14.dp)
                    else -> Icon(Lucide.ArrowUp, if (canSend) C.onAccent else C.faint, 20.dp)
                }
            }
        }
    }

    Column(
        Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp).fillMaxWidth()
            // A true pill on one line (half of the 56 dp height), soft corners once it grows.
            .background(fill, RoundedCornerShape(28.dp)).padding(6.dp)
            .animateContentSize(Motion.standard()),
    ) {
        // Slash commands: "/" shows what can be typed; a tap runs it.
        val typedCommand = text.trim().lowercase()
        if (typedCommand.startsWith("/") && "/compact".startsWith(typedCommand) && files.isEmpty()) {
            Row(
                Modifier.fillMaxWidth().padding(bottom = 4.dp).clip(RoundedCornerShape(Radius.lg)).press {
                    onText("")
                    vm.compactChat(agent.id)
                }.background(C.hover).padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(Lucide.Archive, C.accentHi, 15.dp)
                Txt("/compact", 13.5.sp, C.text, mono = true)
                Txt("Summarize older messages now", 12.5.sp, C.faint, maxLines = 1, modifier = Modifier.weight(1f))
            }
        }
        if (files.isNotEmpty() || reading > 0) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 4.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                files.forEachIndexed { i, f -> AttachmentChip(f) { files.removeAt(i) } }
                if (reading > 0) {
                    Box(Modifier.size(64.dp).background(C.hover, RoundedCornerShape(Radius.md)), contentAlignment = Alignment.Center) { Spinner(18.dp) }
                }
            }
        }
        Row(Modifier.fillMaxWidth().onSizeChanged { rowWidth = it.width }, verticalAlignment = Alignment.CenterVertically) {
            if (!multi) {
                PlusButton()
                ResearchButton()
            }
            BasicTextField(
                value = text,
                onValueChange = onText,
                enabled = enabled,
                textStyle = COMPOSER_TEXT,
                cursorBrush = SolidColor(C.accentHi),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                maxLines = 8,
                modifier = Modifier.weight(1f).onFocusChanged { focused = it.isFocused },
                decorationBox = { inner ->
                    // Centred on the buttons: the line box is centred on its glyphs, then 1 dp
                    // lower, because capitals and ascenders make an exactly centred line read high.
                    Box(
                        Modifier.fillMaxWidth().heightIn(min = COMPOSER_BTN)
                            .padding(start = FIELD_PAD, end = FIELD_PAD, top = 11.dp, bottom = 9.dp),
                        contentAlignment = Alignment.CenterStart,
                    ) {
                        if (text.isEmpty()) {
                            Txt(
                                when {
                                    !enabled -> "Waiting for the connection…"
                                    files.isNotEmpty() -> "Add a message…"
                                    else -> "Message ${agent.name}…"
                                },
                                16.sp, C.faint, maxLines = 1, lineHeight = 22.sp,
                            )
                        }
                        inner()
                    }
                },
            )
            if (!multi) SendButton()
        }
        if (multi) {
            Row(Modifier.fillMaxWidth().padding(top = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                PlusButton()
                ResearchButton()
                Spacer(Modifier.weight(1f))
                SendButton()
            }
        }
    }
}

private val COMPOSER_BTN = 44.dp
private val FIELD_PAD = 10.dp
private val COMPOSER_TEXT = style(16.sp, C.text, lineHeight = 22.sp).copy(
    lineHeightStyle = androidx.compose.ui.text.style.LineHeightStyle(
        androidx.compose.ui.text.style.LineHeightStyle.Alignment.Center,
        androidx.compose.ui.text.style.LineHeightStyle.Trim.None,
    ),
)

/** A picked file in the message box: a thumbnail for a photo, a chip for anything else. */
@Composable
private fun AttachmentChip(file: Upload, onRemove: () -> Unit) {
    val thumb = remember(file) { file.thumb?.asImageBitmap() }
    Box {
        if (thumb != null) {
            Image(thumb, file.name, Modifier.size(64.dp).clip(RoundedCornerShape(Radius.md)), contentScale = ContentScale.Crop)
        } else {
            Row(
                Modifier.height(64.dp).widthIn(max = 200.dp).background(C.elevated, RoundedCornerShape(Radius.md))
                    .padding(start = 12.dp, end = 26.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(Lucide.FileText, C.accentHi, 20.dp)
                Column {
                    Txt(file.name, 13.sp, C.text, maxLines = 1)
                    Txt(sizeLabel(file.bytes.size), 11.5.sp, C.faint)
                }
            }
        }
        Box(
            Modifier.align(Alignment.TopEnd).offset(4.dp, (-4).dp).size(24.dp).clip(CircleShape)
                .background(C.bg.copy(alpha = 0.85f), CircleShape).border(1.dp, C.lineStrong, CircleShape).press(onClick = onRemove),
            contentAlignment = Alignment.Center,
        ) { Icon(Lucide.X, C.text, 12.dp) }
    }
}

private fun sizeLabel(bytes: Int) = when {
    bytes >= 1024 * 1024 -> "%.1f MB".format(bytes / 1024f / 1024f)
    bytes >= 1024 -> "${bytes / 1024} KB"
    else -> "$bytes B"
}
