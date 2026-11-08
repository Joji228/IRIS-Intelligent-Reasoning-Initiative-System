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
import dev.iris.app.data.REASONING_LEVELS
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
                    .background(C.surface.copy(alpha = 0.92f), RoundedCornerShape(18.dp))
                    .border(1.dp, C.line, RoundedCornerShape(18.dp)).heightIn(max = 170.dp)
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
                            Modifier.background(C.accentSoft, RoundedCornerShape(50)).border(1.dp, C.accent.copy(alpha = 0.5f), RoundedCornerShape(50))
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

        Composer(vm, agent, draft, { draft = it }, running, enabled = vm.conn == Conn.Online)
    }
}

@Composable
private fun Header(vm: IrisViewModel, agent: Agent) {
    val overlays = LocalOverlays.current
    val working = vm.isWorking(agent.id)
    val waiting = vm.isWaiting(agent.id)
    Row(
        Modifier.fillMaxWidth().height(66.dp).padding(start = 0.dp, end = 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(Lucide.ChevronLeft, { vm.back() }, C.muted, 26.dp)
        AgentOrb(vm, agent, 38.dp)
        Column(Modifier.weight(1f).padding(start = 10.dp, end = 6.dp)) {
            Txt(agent.name, 16.sp, C.text, FontWeight.SemiBold, maxLines = 1)
            AnimatedContent(
                when { waiting -> 1; working -> 2; else -> 0 },
                transitionSpec = { (fadeIn(tween(200)) + slideInVertically { it / 2 }) togetherWith fadeOut(tween(120)) },
                label = "status",
            ) { s ->
                when (s) {
                    1 -> Txt("Waiting for your approval", 12.sp, C.warn, FontWeight.Medium, maxLines = 1)
                    // The exact step when there is one ("Searching the web for “…”"), else the plain phrase.
                    2 -> Shimmer(vm.detailOf(agent.id).ifEmpty { vm.statusLine(agent.id) }, 12.sp, FontWeight.Normal)
                    else -> Txt(agent.description.ifEmpty { "Ready" }, 12.sp, C.faint, maxLines = 1)
                }
            }
        }
        // Model, thinking effort and auto mode, like the desktop's chat header.
        Row(
            Modifier.background(C.surface.copy(alpha = 0.6f), RoundedCornerShape(50)).border(1.dp, C.line, RoundedCornerShape(50)).padding(2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(
                Modifier.height(38.dp).clip(RoundedCornerShape(50)).press { modelSheet(vm, overlays, agent) }.padding(horizontal = 11.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Icon(Lucide.Brain, C.muted, 16.dp)
                Txt(reasoningLabel(agent.reasoningEffort), 12.sp, C.muted, FontWeight.Medium)
            }
            val auto = autoMode(agent)
            Box(
                Modifier.size(38.dp)
                    .background(if (auto == "full") C.accent.copy(alpha = 0.22f) else androidx.compose.ui.graphics.Color.Transparent, CircleShape)
                    .press { autoSheet(vm, overlays, agent) },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    when (auto) { "full" -> Lucide.Zap; "careful" -> Lucide.ShieldCheck; else -> Lucide.ShieldOff },
                    if (auto == "off") C.faint else C.accentHi, 17.dp,
                )
            }
        }
        IconButton(Lucide.Ellipsis, { agentMenu(vm, overlays, agent, inChat = true) }, C.muted)
    }
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
                if (vm.providers.size > 1) overlays.picker("Provider", vm.providers.map { it.id to it.name }, start) { choose(it) }
                else if (start.isNotEmpty()) choose(start)
            },
        )
        Label("Thinking")
        Chips(listOf("" to "Auto") + REASONING_LEVELS, a.reasoningEffort) { level ->
            vm.patchAgent(a.id, buildJsonObject { put("reasoning_effort", level) })
        }
        Hint("Higher levels answer more carefully, but are slower and use more tokens. Auto follows Settings → Models.")
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
                        if (i == firstKept) CompactedDivider(vm.thread?.summary.orEmpty())
                        MessageItem(vm, m, agent, i == n - 1)
                    }
                }
            }
            AnimatedVisibility(
                away, Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp),
                enter = fadeIn() + scaleIn(initialScale = 0.8f), exit = fadeOut() + scaleOut(targetScale = 0.8f),
            ) {
                Row(
                    Modifier.background(C.elevated, RoundedCornerShape(50)).border(1.dp, C.lineStrong, RoundedCornerShape(50))
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
            Modifier.background(C.panel, RoundedCornerShape(50)).border(1.dp, C.line, RoundedCornerShape(50))
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
                    .background(C.bubble, RoundedCornerShape(16.dp))
                    .border(1.dp, C.bubbleLine, RoundedCornerShape(16.dp)).press { fill(s) }.padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
    }
}

@Composable
private fun CompactedDivider(summary: String) {
    var open by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Pill(Modifier.press { open = !open }) {
            Icon(Lucide.Archive, C.faint, 12.dp)
            Txt("Earlier messages were compacted · ${if (open) "hide" else "show"} summary", 11.5.sp, C.faint)
        }
        AnimatedVisibility(open) {
            Box(Modifier.fillMaxWidth().background(C.surface, RoundedCornerShape(14.dp)).padding(14.dp)) {
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
    val shape = RoundedCornerShape(28.dp)
    val edge by animateColorAsState(if (focused) C.accent.copy(alpha = 0.45f) else C.lineStrong, tween(200), label = "edge")

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

    Column(
        Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp).fillMaxWidth()
            .background(C.surface, shape).border(1.dp, edge, shape).padding(5.dp)
            .animateContentSize(Motion.standard()),
    ) {
        if (files.isNotEmpty() || reading > 0) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 6.dp, end = 6.dp, top = 6.dp, bottom = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                files.forEachIndexed { i, f -> AttachmentChip(f) { files.removeAt(i) } }
                if (reading > 0) {
                    Box(Modifier.size(64.dp).background(C.elevated, RoundedCornerShape(16.dp)), contentAlignment = Alignment.Center) { Spinner(18.dp) }
                }
            }
        }
        Row(verticalAlignment = Alignment.Bottom) {
            IconButton(Lucide.Plus, ::attach, C.muted, 22.dp, Modifier.size(46.dp))
            // Deep research: an icon, which grows a label while it's on.
            Row(
                Modifier.height(46.dp).widthIn(min = 42.dp).clip(RoundedCornerShape(50))
                    .background(if (research) C.accentSoft else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(50))
                    .press { research = !research; haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove) }
                    .padding(horizontal = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Lucide.Telescope, if (research) C.accentHi else C.muted, 20.dp)
                AnimatedVisibility(research, enter = expandHorizontally() + fadeIn(), exit = shrinkHorizontally() + fadeOut()) {
                    Txt("Research", 12.5.sp, C.accentHi, FontWeight.Medium, Modifier.padding(start = 6.dp))
                }
            }
            BasicTextField(
                value = text,
                onValueChange = onText,
                enabled = enabled,
                textStyle = style(16.sp, C.text, lineHeight = 23.sp),
                cursorBrush = SolidColor(C.accentHi),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                maxLines = 7,
                modifier = Modifier.weight(1f).heightIn(min = 46.dp).onFocusChanged { focused = it.isFocused }
                    // A hair lower than centred: lowercase text reads centred against the round buttons.
                    .padding(start = 4.dp, end = 6.dp, top = 12.dp, bottom = 10.dp),
                decorationBox = { inner ->
                    Box {
                        if (text.isEmpty()) {
                            Txt(
                                when {
                                    !enabled -> "Waiting for the connection…"
                                    files.isNotEmpty() -> "Add a message…"
                                    else -> "Message ${agent.name}…"
                                },
                                16.sp, C.faint, maxLines = 1, lineHeight = 23.sp,
                            )
                        }
                        inner()
                    }
                },
            )
            val stop = running && text.isBlank() && files.isEmpty() && !sending
            val bg by animateColorAsState(if (stop) C.text else if (canSend || sending) C.accent else C.hover, tween(180), label = "send")
            val pop by animateFloatAsState(if (canSend || stop || sending) 1f else 0.92f, spring(dampingRatio = 0.6f, stiffness = 600f), label = "pop")
            Box(
                Modifier.size(46.dp).graphicsLayer { scaleX = pop; scaleY = pop }.clip(CircleShape).background(bg, CircleShape)
                    .press(stop || canSend) { if (stop) vm.stop() else submit() },
                contentAlignment = Alignment.Center,
            ) {
                AnimatedContent(if (sending) 2 else if (stop) 1 else 0, transitionSpec = { (scaleIn() + fadeIn()) togetherWith (scaleOut() + fadeOut()) }, label = "sendIcon") { s ->
                    when (s) {
                        2 -> Spinner(18.dp, C.onAccent)
                        1 -> Icon(Lucide.Square, C.bg, 15.dp)
                        else -> Icon(Lucide.ArrowUp, if (canSend) C.onAccent else C.faint, 22.dp)
                    }
                }
            }
        }
    }
}

/** A picked file in the message box: a thumbnail for a photo, a chip for anything else. */
@Composable
private fun AttachmentChip(file: Upload, onRemove: () -> Unit) {
    val thumb = remember(file) { file.thumb?.asImageBitmap() }
    Box {
        if (thumb != null) {
            Image(thumb, file.name, Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)), contentScale = ContentScale.Crop)
        } else {
            Row(
                Modifier.height(64.dp).widthIn(max = 200.dp).background(C.elevated, RoundedCornerShape(16.dp))
                    .border(1.dp, C.line, RoundedCornerShape(16.dp)).padding(start = 12.dp, end = 26.dp),
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
