package dev.iris.app.data

import android.app.Application
import android.os.Build
import android.provider.Settings
import dev.iris.app.BuildConfig
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.iris.app.link.LinkConnection
import dev.iris.app.link.LinkCredentials
import dev.iris.app.link.LinkState
import dev.iris.app.link.SignIn
import dev.iris.app.link.signInOnce
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonPrimitive

enum class Conn { SignIn, Starting, Online, Reconnecting }

sealed interface Screen {
    data object Home : Screen
    data class Chat(val agentId: String) : Screen
    /** Settings: the list ([page] null) or one section. */
    data class Settings(val page: String? = null) : Screen
    data class AgentEdit(val agentId: String) : Screen
}

/** Matches the engine's markers for notes it posts into chats (see MessageItem.svelte). */
object Prefixes {
    const val UPDATE = "[I.R.I.S. update]"
    const val SETUP = "[I.R.I.S. setup]"
    const val RELEASE = "[I.R.I.S. release]"
    const val RESEARCH = "[Deep research]"
    const val ROUTINE = "[Routine:"
}

class IrisViewModel(app: Application) : AndroidViewModel(app) {
    private val store = CredentialStore(app)

    var conn by mutableStateOf(Conn.Starting)
        private set
    var linkState by mutableStateOf(LinkState.Connecting)
        private set
    var signinError by mutableStateOf("")

    /** The newest release the server knows of (it asks GitHub), and how updating this app is going. */
    var update by mutableStateOf<UpdateStatus?>(null)
        private set

    /** 0..100 while the update downloads, otherwise null. */
    var updateProgress by mutableStateOf<Int?>(null)
        private set

    /** What to tell the user about the update (a problem, or what to do next). */
    var updateNote by mutableStateOf<String?>(null)

    /** A newer version of this app than the one installed, if it can be installed from here. */
    val phoneUpdate: Pair<UpdateRelease, UpdateAsset>?
        get() = AppUpdate.offer(update, BuildConfig.VERSION_NAME)
    var deviceName by mutableStateOf("")
        private set

    /** Where the user is; Back pops it. */
    val stack = mutableStateListOf<Screen>(Screen.Home)
    val screen: Screen get() = stack.last()

    val agents = mutableStateListOf<Agent>()
    /** Replies in progress, by thread. */
    val running = mutableStateMapOf<String, RunInfo>()
    val approvals = mutableStateMapOf<String, PendingApproval>()
    val queues = mutableStateMapOf<String, List<QueuedMessage>>()
    val recent = mutableStateListOf<Thread>()
    /** Short-lived orb expressions: happy, sad, spawn, spin. */
    val moods = mutableStateMapOf<String, String>()
    /** Each agent's latest chat line (the chat list shows it). */
    val previews = mutableStateMapOf<String, ChatPreview>()
    /** Open problems: failed replies and routines (the inbox). */
    val problems = mutableStateListOf<Problem>()
    /** Agents with a reply the user hasn't opened yet: their orbs nudge. */
    val unread = mutableStateMapOf<String, Boolean>()
    /** The user has been away a while: idle orbs doze until they're back. */
    var sleepy by mutableStateOf(false)
        private set

    var thread by mutableStateOf<Thread?>(null)
        private set
    val messages = mutableStateListOf<Message>()
    var loadingThread by mutableStateOf(false)
        private set
    /** The model provider answers (the engine pings it). */
    var providerOnline by mutableStateOf(true)
        private set
    var balance by mutableStateOf<String?>(null)
        private set
    var contextWindow by mutableStateOf<Long?>(null)
        private set
    var defaultModel by mutableStateOf<DefaultModel?>(null)
        private set
    val providers = mutableStateListOf<Provider>()
    var toast by mutableStateOf<String?>(null)

    private var link: LinkConnection? = null
    private var api: IrisApi? = null
    private var linkWatch: Job? = null
    private var pauseJob: Job? = null
    private val events = Channel<JsonObject>(Channel.UNLIMITED)
    private var openSeq = 0
    private var toastJob: Job? = null
    private var previewJob: Job? = null
    private var foreground = true
    private var lastTouch = System.currentTimeMillis()
    private var pausedAt = 0L
    /** A chat to open once connected (a tapped notification). */
    private var pendingOpen: String? = null

    val orderedAgents: List<Agent>
        get() = agents.sortedWith(compareBy<Agent> { if (it.role == "main") 0 else 1 }.thenBy { it.createdAt })

    val pendingApprovals: List<PendingApproval> get() = approvals.values.sortedBy { it.createdAt }

    /** The agent whose chat is loaded (it stays while its editor is open on top). */
    private val chatAgentId: String? get() = (stack.lastOrNull { it is Screen.Chat } as? Screen.Chat)?.agentId

    fun agent(id: String) = agents.firstOrNull { it.id == id }
    fun isWorking(agentId: String) = running.values.any { it.agentId == agentId }
    fun isWaiting(agentId: String) = approvals.values.any { it.agentId == agentId }
    fun isRunning(threadId: String?) = threadId != null && running.containsKey(threadId)
    fun activityOf(agentId: String) = running.values.firstOrNull { it.agentId == agentId }?.activity
    /** The step an agent is on, in words ("Searching the web for “…”"). */
    fun detailOf(agentId: String) = running.values.firstOrNull { it.agentId == agentId }?.detail.orEmpty()

    /** What a busy agent is doing, in plain words ("Searching the web…"). */
    fun statusLine(agentId: String): String {
        val activity = activityOf(agentId)
        return phrases(orbState(activity, working = activity != null), activity).firstOrNull() ?: "Working…"
    }

    /** Any touch: wakes dozing orbs. */
    /** A notification was tapped: open that agent's chat (now, or once connected). */
    fun openFromNotification(agentId: String) {
        if (conn == Conn.Online && agents.any { it.id == agentId }) openAgent(agentId) else pendingOpen = agentId
    }

    private fun loadProblems() {
        viewModelScope.launch {
            // An older engine without the inbox: nothing to show.
            val list = runCatching { api?.problems() }.getOrNull() ?: return@launch
            replaceAll(problems, list)
        }
    }

    fun retryProblem(p: Problem) = launchCatching {
        api?.retryProblem(p.id)
        problems.removeAll { it.id == p.id }
        if (p.kind == "reply" && agents.any { it.id == p.agentId }) openAgent(p.agentId)
        showToast(if (p.kind == "reply") "Trying again…" else "Running the routine again…")
    }

    fun dismissProblem(id: String? = null) = launchCatching {
        if (id != null) api?.dismissProblem(id) else api?.dismissAllProblems()
        if (id != null) problems.removeAll { it.id == id } else problems.clear()
    }

    fun touched() {
        lastTouch = System.currentTimeMillis()
        if (sleepy) sleepy = false
    }

    init {
        // Ten minutes without a touch and idle orbs fall asleep.
        viewModelScope.launch {
            while (true) {
                delay(20_000)
                if (!sleepy && System.currentTimeMillis() - lastTouch > SLEEP_AFTER) sleepy = true
            }
        }
        viewModelScope.launch {
            for (e in events) onEvent(e)
        }
        val saved = store.load()
        if (saved != null) connect(saved) else conn = Conn.SignIn
    }

    // ---- sign-in ------------------------------------------------------------------

    val lastRelay get() = store.lastRelay

    /** A friendly name for this phone in the server's device list. */
    val defaultDeviceName: String
        get() {
            val name = runCatching {
                Settings.Global.getString(getApplication<Application>().contentResolver, Settings.Global.DEVICE_NAME)
            }.getOrNull()?.takeIf { it.isNotBlank() } ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"
            return "$name (I.R.I.S. app)"
        }

    /** Pairs or signs in; throws with a message to show. */
    suspend fun signIn(relay: String, key: String, msg: SignIn) {
        val creds = signInOnce(relay, key, msg)
        store.save(creds)
        store.lastRelay = relay
        signinError = ""
        connect(creds)
    }

    fun signOut() {
        disconnect()
        api = null
        store.forget()
        signinError = ""
        conn = Conn.SignIn
        goHome()
    }

    private fun disconnect() {
        linkWatch?.cancel()
        link?.stop()
        link = null
    }

    private fun connect(creds: LinkCredentials) {
        disconnect()
        deviceName = creds.device
        if (conn != Conn.Online) conn = Conn.Starting
        val l = LinkConnection(
            creds,
            viewModelScope,
            onEvent = { events.trySend(it) },
            onOpen = { viewModelScope.launch { refreshAll() } },
            onSignedOut = { reason ->
                viewModelScope.launch {
                    disconnect()
                    store.forget()
                    signinError = reason
                    conn = Conn.SignIn
                    goHome()
                }
            },
        )
        link = l
        api = IrisApi(l)
        linkWatch = viewModelScope.launch {
            l.state.collect { s ->
                linkState = s
                if (s != LinkState.Online && conn == Conn.Online) conn = Conn.Reconnecting
            }
        }
        l.start()
    }

    /** The app came back to the front: reconnect now instead of waiting out the backoff. */
    fun resume() {
        pauseJob?.cancel()
        foreground = true
        if (pausedAt > 0 && System.currentTimeMillis() - pausedAt > SLEEP_AFTER) sleepy = true
        if (conn == Conn.SignIn) return
        val l = link
        if (l == null) store.load()?.let { connect(it) } else l.nudge()
    }

    /** Reconnects now (the connection bar's tap). */
    fun nudge() = link?.nudge()

    /**
     * The app went to the background. Until notifications arrive (R5) nothing needs the link
     * there, so it closes after a minute instead of pinging the relay all day.
     */
    fun pause() {
        pauseJob?.cancel()
        foreground = false
        pausedAt = System.currentTimeMillis()
        if (link == null) return
        pauseJob = viewModelScope.launch {
            delay(60_000)
            disconnect()
        }
    }

    /** Full refetch: on first connect and after every reconnect. */
    private suspend fun refreshAll() {
        val api = api ?: return
        try {
            val a = viewModelScope.async { api.agents() }
            val s = viewModelScope.async { api.status() }
            val r = viewModelScope.async { api.recentThreads(30) }
            val p = viewModelScope.async { api.approvals() }
            val m = viewModelScope.async { runCatching { api.defaultModel() }.getOrNull() }
            replaceAll(agents, a.await())
            val status = s.await()
            running.clear()
            status.running.forEach { running[it.threadId] = it }
            queues.clear()
            queues.putAll(status.queues)
            providerOnline = status.online
            replaceAll(recent, r.await())
            approvals.clear()
            p.await().forEach { approvals[it.id] = it }
            m.await()?.let { defaultModel = it; contextWindow = it.contextWindow }
            conn = Conn.Online
            refreshBalance()
            refreshPreviews()
            loadProblems()
            checkUpdate()
            pendingOpen?.let { id -> pendingOpen = null; if (agents.any { it.id == id }) openAgent(id) }
            chatAgentId?.let { id -> if (agents.none { it.id == id }) goHome() else loadChat(id) }
        } catch (e: Exception) {
            error(e)
        }
    }

    /** Asks the server for the newest release (quietly: an older server has no such answer). */
    fun checkUpdate(refresh: Boolean = false) {
        val api = api ?: return
        viewModelScope.launch { runCatching { api.update(refresh) }.onSuccess { update = it } }
    }

    /** Downloads the new APK, checks it, and opens Android's installer. */
    fun installUpdate() {
        val (release, asset) = phoneUpdate ?: return
        if (updateProgress != null) return
        val context = getApplication<Application>()
        updateNote = null
        if (!AppUpdate.canInstall(context)) {
            updateNote = "Android first has to allow I.R.I.S. to install apps. Switch it on in the screen that just opened, come back, and press Update again."
            runCatching { context.startActivity(AppUpdate.allowInstallsIntent(context)) }
            return
        }
        viewModelScope.launch {
            updateProgress = 0
            try {
                val apk = AppUpdate.download(context, release, asset) { updateProgress = it }
                if (!AppUpdate.signedLikeThisApp(context, apk)) {
                    apk.delete()
                    updateNote = "This update is signed with a different key than the app on your phone, so Android won't install it over this one. Uninstall I.R.I.S., install the new version from the release page, and pair the phone again."
                } else {
                    context.startActivity(AppUpdate.installIntent(context, apk))
                }
            } catch (e: Exception) {
                updateNote = e.message ?: "The update didn't work."
            } finally {
                updateProgress = null
            }
        }
    }

    private fun refreshBalance() {
        val api = api ?: return
        viewModelScope.launch {
            runCatching {
                val provider = api.providers().firstOrNull { it.id.contains("deepseek", true) && it.hasKey } ?: return@launch
                val b = api.balance(provider.id)
                // Same format as the desktop's CreditBadge.
                val symbol = mapOf("USD" to "$", "CNY" to "¥", "EUR" to "€")[b.currency]
                val value = b.total.toDoubleOrNull()?.let { "%.2f".format(it) } ?: b.total
                balance = if (b.total.isEmpty()) null else if (symbol != null) "$symbol$value" else "$value ${b.currency}"
            }
        }
    }

    // ---- navigation ------------------------------------------------------------------

    /** Back to Home. The chat's messages stay until another chat opens, so it can animate out. */
    fun goHome() {
        openSeq++
        stack.clear()
        stack.add(Screen.Home)
    }

    fun open(s: Screen) {
        if (screen != s) stack.add(s)
    }

    /** Back one screen; false when already Home. */
    fun back(): Boolean {
        if (stack.size <= 1) return false
        stack.removeAt(stack.size - 1)
        if (stack.none { it is Screen.Chat }) openSeq++
        return true
    }

    /** Opens an agent's one chat: the switch is immediate, the messages follow. */
    fun openAgent(agentId: String) {
        val seq = ++openSeq
        val same = thread?.agentId == agentId
        // From a chat, another chat replaces it; from anywhere else it goes on top.
        val target = Screen.Chat(agentId)
        if (screen is Screen.Chat) stack[stack.size - 1] = target else if (screen != target) stack.add(target)
        unread.remove(agentId)
        if (!same) {
            thread = null
            messages.clear()
        }
        viewModelScope.launch { loadChat(agentId, seq) }
    }

    private suspend fun loadChat(agentId: String, seq: Int = openSeq) {
        val api = api ?: return
        loadingThread = true
        try {
            val detail = api.chat(agentId)
            if (seq != openSeq || chatAgentId != agentId) return
            thread = detail.thread
            replaceAll(messages, detail.messages)
        } catch (e: ApiException) {
            if (e.status == 404) {
                if (seq == openSeq) goHome()
            } else error(e)
        } finally {
            if (seq == openSeq) loadingThread = false
        }
    }

    // ---- actions ---------------------------------------------------------------------

    fun send(agent: Agent, text: String, files: List<Upload> = emptyList(), onDone: (Boolean) -> Unit = {}) {
        val api = api ?: return
        if (text.isBlank() && files.isEmpty()) return
        viewModelScope.launch {
            try {
                var tid = thread?.takeIf { it.agentId == agent.id }?.id
                if (tid == null) {
                    loadChat(agent.id)
                    tid = thread?.id ?: return@launch onDone(false)
                }
                // Files go up one by one first, then the message that carries them.
                val attachments = files.map { api.uploadAttachment(tid, it) }
                val posted = api.postMessage(tid, text, attachments)
                val line = text.trim().ifEmpty {
                    when {
                        files.size > 1 -> "Sent ${files.size} files"
                        files.first().isImage -> "Sent a photo"
                        else -> "Sent ${files.first().name}"
                    }
                }
                previews[agent.id] = ChatPreview(agent.id, "user", line.take(160), System.currentTimeMillis())
                posted.userMessage?.let(::upsertMessage)
                posted.assistantMessage?.let(::upsertMessage)
                posted.queued?.let { q ->
                    val list = queues[tid].orEmpty()
                    if (list.none { it.id == q.id }) queues[tid] = list + q
                }
                onDone(true)
            } catch (e: Exception) {
                error(e)
                onDone(false)
            }
        }
    }

    fun stop() = thread?.id?.let { tid -> launchCatching { api?.stop(tid) } }

    fun regenerate() = thread?.id?.let { tid ->
        launchCatching { api?.regenerate(tid)?.assistantMessage?.let(::upsertMessage) }
    }

    fun decide(approvalId: String, approve: Boolean) {
        val api = api ?: return
        // Optimistic: the reply resumes right away; events confirm it.
        approvals.remove(approvalId)
        viewModelScope.launch {
            try {
                api.decide(approvalId, approve)
            } catch (e: Exception) {
                error(e)
                refreshAll()
            }
        }
    }

    fun removeQueued(threadId: String, id: String) = launchCatching { api?.removeQueued(threadId, id) }
    fun steerQueued(threadId: String, id: String) = launchCatching { api?.steerQueued(threadId, id) }

    fun summonAgent() = launchCatching {
        val agent = api?.summonAgent() ?: return@launchCatching
        upsertAgent(agent)
        openAgent(agent.id)
    }

    fun clearChat(agentId: String) = launchCatching {
        val detail = api?.clearChat(agentId) ?: return@launchCatching
        if (thread?.agentId == agentId) {
            thread = detail.thread
            messages.clear()
        }
        showToast("${agent(agentId)?.name ?: "The"} chat cleared")
    }

    fun compactChat(agentId: String) = launchCatching {
        showToast("Compacting…")
        val detail = api?.compactChat(agentId) ?: return@launchCatching
        if (thread?.agentId == agentId) {
            thread = detail.thread
            replaceAll(messages, detail.messages)
        }
        showToast("Chat compacted: older messages are summarized")
    }

    fun deleteAgent(agentId: String) = launchCatching {
        api?.deleteAgent(agentId)
        removeAgent(agentId)
    }

    suspend fun attachment(threadId: String, id: String): ByteArray? = api?.attachment(threadId, id)

    // ---- settings ------------------------------------------------------------------------

    /** Runs a settings call; shows its error and returns null when it fails. */
    suspend fun <T> attempt(block: suspend IrisApi.() -> T): T? {
        val api = api ?: return null
        return try {
            api.block()
        } catch (e: Exception) {
            error(e)
            null
        }
    }

    /** An agent changed in its editor (the event confirms it too). */
    fun agentSaved(a: Agent) = upsertAgent(a)

    /** Changes a few of an agent's settings (the chat header's model and auto pickers). */
    fun patchAgent(agentId: String, patch: JsonObject, done: String? = null) = launchCatching {
        api?.patchAgent(agentId, patch)?.let {
            upsertAgent(it)
            done?.let(::showToast)
        }
    }

    suspend fun loadModelSettings() {
        attempt { defaultModel() }?.let { defaultModel = it; contextWindow = it.contextWindow }
        attempt { providers() }?.let { replaceAll(providers, it) }
    }

    suspend fun setDefaultModel(provider: String, model: String) {
        attempt { putDefaultModel(provider, model) }?.let {
            defaultModel = it
            showToast("Every agent now uses ${modelName(it.model)}")
        }
    }

    // ---- live events -----------------------------------------------------------------

    private fun onEvent(e: JsonObject) {
        when (e.type) {
            "agent_updated" -> e.field<Agent>("agent")?.let { a ->
                val isNew = agents.none { it.id == a.id }
                upsertAgent(a)
                if (isNew) setMood(a.id, "spawn", 1600)
            }
            "agent_deleted" -> e.str("agent_id")?.let(::removeAgent)
            "thread_updated" -> e.field<Thread>("thread")?.let(::upsertThread)
            "thread_deleted" -> e.str("thread_id")?.let { tid ->
                recent.removeAll { it.id == tid }
                running.remove(tid)
                if (thread?.id == tid) chatAgentId?.let { openAgent(it) }
            }
            "message_created", "message_completed" -> e.field<Message>("message")?.let(::upsertMessage)
            "message_deleted" -> e.str("message_id")?.let { id -> messages.removeAll { it.id == id } }
            "message_delta" -> {
                if (thread?.id != e.str("thread_id")) return
                val i = messages.indexOfFirst { it.id == e.str("message_id") }
                if (i < 0) return
                val round = e["round"]?.jsonPrimitive?.int ?: 0
                val content = e.str("content").orEmpty()
                val reasoning = e.str("reasoning").orEmpty()
                val m = messages[i]
                val rounds = m.rounds.toMutableList()
                while (rounds.size <= round) rounds.add(Round())
                val r = rounds[round]
                rounds[round] = r.copy(content = r.content + content, reasoning = r.reasoning + reasoning)
                messages[i] = m.copy(rounds = rounds, content = m.content + content, reasoning = m.reasoning + reasoning)
            }
            "message_progress" -> {
                if (thread?.id != e.str("thread_id")) return
                val i = messages.indexOfFirst { it.id == e.str("message_id") }
                val rounds = e.field<List<Round>>("rounds")
                if (i >= 0 && rounds != null) messages[i] = messages[i].copy(rounds = rounds)
            }
            "approval_requested" -> e.field<PendingApproval>("approval")?.let { approvals[it.id] = it }
            "approval_resolved" -> e.str("approval_id")?.let { approvals.remove(it) }
            "run_started" -> {
                val tid = e.str("thread_id") ?: return
                running[tid] = RunInfo(e.str("agent_id").orEmpty(), tid, e.str("message_id").orEmpty(), "thinking")
            }
            "run_activity" -> {
                val tid = e.str("thread_id") ?: return
                val activity = e.str("activity")
                running[tid]?.let { running[tid] = it.copy(activity = activity, detail = e.str("detail").orEmpty()) }
                if (activity == "delegate_task" || activity == "create_agent") setMood(e.str("agent_id").orEmpty(), "spin", 1100)
            }
            "run_finished" -> {
                e.str("thread_id")?.let { running.remove(it) }
                val agentId = e.str("agent_id").orEmpty()
                when (e.str("status")) {
                    "complete" -> {
                        setMood(agentId, "happy", 2600)
                        // A reply in a chat that isn't on screen: the orb nudges until it's read.
                        if (!(foreground && (screen as? Screen.Chat)?.agentId == agentId)) unread[agentId] = true
                    }
                    "error" -> setMood(agentId, "sad", 4500)
                }
                refreshPreviews(600)
                viewModelScope.launch {
                    delay(2500)
                    refreshBalance()
                }
            }
            "providers_changed" -> viewModelScope.launch { loadModelSettings() }
            "connectivity" -> providerOnline = e["online"]?.jsonPrimitive?.content == "true"
            "queue_changed" -> {
                val tid = e.str("thread_id") ?: return
                val q = e.field<List<QueuedMessage>>("queue").orEmpty()
                if (q.isEmpty()) queues.remove(tid) else queues[tid] = q
            }
            "problems_changed" -> loadProblems()
            "lagged" -> viewModelScope.launch { refreshAll() }
        }
    }

    private fun upsertAgent(a: Agent) {
        val i = agents.indexOfFirst { it.id == a.id }
        if (i >= 0) agents[i] = a else agents.add(a)
    }

    private fun removeAgent(id: String) {
        agents.removeAll { it.id == id }
        recent.removeAll { it.agentId == id }
        if (stack.any { (it as? Screen.Chat)?.agentId == id || (it as? Screen.AgentEdit)?.agentId == id }) goHome()
    }

    private fun upsertThread(t: Thread) {
        val i = recent.indexOfFirst { it.id == t.id }
        if (i >= 0) recent[i] = t else recent.add(0, t)
        recent.sortByDescending { it.updatedAt }
        if (thread?.id == t.id) thread = t
    }

    private fun upsertMessage(m: Message) {
        if (thread?.id != m.threadId) return
        val i = messages.indexOfFirst { it.id == m.id }
        if (i >= 0) messages[i] = m else messages.add(m)
    }

    /** Reloads the chat list's last lines (soon, after a burst of replies). */
    private fun refreshPreviews(delayMs: Long = 0) {
        previewJob?.cancel()
        previewJob = viewModelScope.launch {
            delay(delayMs)
            // An older engine without previews: the list shows descriptions.
            val list = runCatching { api?.previews() }.getOrNull() ?: return@launch
            previews.clear()
            list.forEach { previews[it.agentId] = it }
        }
    }

    private fun setMood(agentId: String, mood: String, ms: Long) {
        if (agentId.isEmpty()) return
        moods[agentId] = mood
        viewModelScope.launch {
            delay(ms)
            if (moods[agentId] == mood) moods.remove(agentId)
        }
    }

    // ---- helpers -----------------------------------------------------------------------

    private fun launchCatching(block: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                block()
            } catch (e: Exception) {
                error(e)
            }
        }
    }

    private fun error(e: Exception) {
        showToast(e.message ?: "Something went wrong")
    }

    fun showToast(text: String) {
        toast = text
        toastJob?.cancel()
        toastJob = viewModelScope.launch {
            delay(3200)
            toast = null
        }
    }

    override fun onCleared() {
        link?.stop()
    }

    private fun <T> replaceAll(list: MutableList<T>, items: List<T>) {
        list.clear()
        list.addAll(items)
    }
}
