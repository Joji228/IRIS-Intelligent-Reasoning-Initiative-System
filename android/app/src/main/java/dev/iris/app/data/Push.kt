package dev.iris.app.data

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import dev.iris.app.MainActivity
import dev.iris.app.R
import dev.iris.app.link.LinkConnection
import dev.iris.app.link.LinkState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Notifications without any push service: while they're on, [LinkService] keeps its own
 * end-to-end encrypted link to the server open in the background (Android shows a small
 * "connected" notification for it) and turns the engine's events into notifications, the
 * way the engine's push.rs picks them. While the app is on screen it shows everything live,
 * so the service steps aside and its link closes.
 */
object Push {
    /** The user's switch (Settings → Notifications). */
    val enabled = MutableStateFlow(false)

    /** The app is on screen. */
    val foreground = MutableStateFlow(false)

    /** The background link, for the "connected" notification and Settings. */
    val linkState = MutableStateFlow(LinkState.Offline)

    /** What to notify about (Settings → Notifications), kept on this phone. */
    val choices = MutableStateFlow(PushPrefs())

    private const val PREFS = "push"
    private const val KEY_ON = "builtin"

    fun load(context: Context) {
        val p = prefs(context)
        choices.value = PushPrefs(
            p.getBoolean("replies", true), p.getBoolean("questions", true), p.getBoolean("approvals", true), p.getBoolean("problems", true),
        )
        enabled.value = p.getBoolean(KEY_ON, false)
        if (enabled.value) start(context)
    }

    fun enable(context: Context) {
        prefs(context).edit().putBoolean(KEY_ON, true).apply()
        enabled.value = true
        start(context)
    }

    fun disable(context: Context) {
        prefs(context).edit().putBoolean(KEY_ON, false).apply()
        enabled.value = false
        context.stopService(Intent(context, LinkService::class.java))
    }

    fun choose(context: Context, next: PushPrefs) {
        choices.value = next
        prefs(context).edit()
            .putBoolean("replies", next.replies).putBoolean("questions", next.questions)
            .putBoolean("approvals", next.approvals).putBoolean("problems", next.problems)
            .apply()
    }

    /** The background service is alive (shown in Settings). */
    val running = MutableStateFlow(false)

    /** Why the background link couldn't start last time (shown in Settings), or null. */
    val startError = MutableStateFlow<String?>(null)

    internal fun start(context: Context) {
        // Android refuses to start it while the app is in the background (a sleeping phone,
        // a delivery the system held back); the app starts it again when it's on screen.
        runCatching { context.startForegroundService(Intent(context, LinkService::class.java)) }
            .onSuccess { startError.value = null }
            .onFailure { startError.value = it.message ?: it.javaClass.simpleName }
    }

    /** Samsung and others pause background apps unless they're allowed to run unrestricted. */
    fun unrestricted(context: Context) =
        context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // ---- notifications ------------------------------------------------------------------

    internal const val CHANNEL_LINK = "connection"

    /** One channel per kind, so each can be silenced in Android's settings. */
    internal fun channels(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_LINK, "Connection", NotificationManager.IMPORTANCE_MIN).apply {
                description = "The small notification that keeps I.R.I.S. connected in the background"
                setShowBadge(false)
            },
        )
        for ((id, name, importance) in listOf(
            Triple("replies", "Replies", NotificationManager.IMPORTANCE_DEFAULT),
            Triple("questions", "Questions", NotificationManager.IMPORTANCE_HIGH),
            Triple("approvals", "Approvals", NotificationManager.IMPORTANCE_HIGH),
            Triple("problems", "Problems", NotificationManager.IMPORTANCE_DEFAULT),
        )) nm.createNotificationChannel(NotificationChannel(id, name, importance))
    }

    internal fun openApp(context: Context, agentId: String?) = PendingIntent.getActivity(
        context, agentId.hashCode(),
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .apply { if (!agentId.isNullOrEmpty()) putExtra(MainActivity.EXTRA_AGENT, agentId) },
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    /** Shows one notification; the same tag replaces the earlier one (one per chat or approval). */
    fun show(context: Context, channel: String, title: String, body: String, tag: String, agentId: String?) {
        channels(context)
        // On a locked phone with a PIN, pattern or password, Android shows this plainer version
        // instead of the text of the reply (if the phone is set to hide private notification content,
        // which is the phone's own choice; unlocked, or with the content allowed, nothing changes).
        val plain = Notification.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notify)
            .setColor(0xFF8B5CF6.toInt())
            .setContentTitle("I.R.I.S.")
            .setContentText("New notification")
            .build()
        val notification = Notification.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notify)
            .setColor(0xFF8B5CF6.toInt())
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(Notification.BigTextStyle().bigText(body))
            .setContentIntent(openApp(context, agentId))
            .setAutoCancel(true)
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(plain)
            .build()
        runCatching { context.getSystemService(NotificationManager::class.java).notify(tag, 0, notification) }
    }
}

/** The background link that turns the server's events into notifications. */
class LinkService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var link: LinkConnection? = null
    private var watch: Job? = null

    private val names = HashMap<String, String>()
    private val seenProblems = HashSet<String>()
    private var problemsLoaded = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Push.running.value = true
        Push.channels(this)
        val first = status(LinkState.Connecting)
        if (Build.VERSION.SDK_INT >= 34) startForeground(1, first, ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING)
        else startForeground(1, first)
        // Connected only while the app is off screen (it has its own link then).
        watch = scope.launch {
            combine(Push.foreground, Push.enabled) { front, on -> on && !front }.collect { needed ->
                if (needed) connect() else drop()
            }
        }
        // The "connected" notification follows the link.
        scope.launch {
            combine(Push.linkState, Push.foreground) { state, _ -> state }.collect {
                runCatching { getSystemService(NotificationManager::class.java).notify(1, status(it)) }
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (!Push.enabled.value) Push.load(this)
        if (!Push.enabled.value) stopSelf()
        return START_STICKY
    }

    override fun onDestroy() {
        Push.running.value = false
        drop()
        scope.cancel()
        super.onDestroy()
    }

    private fun status(state: LinkState): Notification {
        val text = when {
            Push.foreground.value -> "On · the app is open"
            state == LinkState.Online -> "On · you'll get replies, questions and approvals"
            state == LinkState.ServerOffline -> "Your server is offline · waiting for it"
            else -> "Connecting to your server…"
        }
        return Notification.Builder(this, Push.CHANNEL_LINK)
            .setSmallIcon(R.drawable.ic_notify)
            .setColor(0xFF8B5CF6.toInt())
            .setContentTitle("I.R.I.S. notifications")
            .setContentText(text)
            .setContentIntent(Push.openApp(this, null))
            .setOngoing(true)
            .build()
    }

    private fun connect() {
        if (link != null) return
        val creds = CredentialStore(this).load() ?: return stopSelf()
        lateinit var l: LinkConnection
        l = LinkConnection(
            creds, scope,
            onEvent = { e -> scope.launch { runCatching { onEvent(l, e) } } },
            onOpen = { scope.launch { runCatching { refresh(l) } } },
            onSignedOut = { reason ->
                // Say so, instead of notifications quietly stopping.
                Push.show(
                    this, "problems", "This phone was signed out of I.R.I.S.",
                    "${reason.replaceFirstChar { it.uppercase() }.trimEnd('.')}. Open the app to sign in again.", "signed-out", null,
                )
                Push.disable(this)
            },
            // Less often than on screen: easier on the battery, still inside the relay's 45 s.
            pingEvery = 30_000,
        )
        link = l
        scope.launch { l.state.collect { Push.linkState.value = it } }
        l.start()
    }

    private fun drop() {
        link?.stop()
        link = null
        Push.linkState.value = LinkState.Offline
    }

    private suspend fun get(l: LinkConnection, path: String) =
        l.request("GET", path).takeIf { it.status in 200..299 }?.body?.let { irisJson.parseToJsonElement(it) }

    /** After every (re)connect: the agents' names, which problems are old news. */
    private suspend fun refresh(l: LinkConnection) {
        get(l, "/v1/agents")?.let { list -> irisJson.decodeFromJsonElement<List<Agent>>(list).forEach { names[it.id] = it.name } }
        loadProblems(l, quiet = !problemsLoaded)
        problemsLoaded = true
    }

    private fun name(agentId: String) = names[agentId] ?: "An agent"

    private suspend fun onEvent(l: LinkConnection, e: JsonObject) {
        if (Push.foreground.value) return
        when (e["type"]?.jsonPrimitive?.content) {
            "approval_requested" -> {
                if (!Push.choices.value.approvals) return
                val a = irisJson.decodeFromJsonElement<PendingApproval>(e["approval"] ?: return)
                Push.show(this, "approvals", "${name(a.agentId)} needs your OK", a.summary, "approval-${a.agentId}", a.agentId)
            }
            "message_completed" -> {
                val m = irisJson.decodeFromJsonElement<Message>(e["message"] ?: return)
                if (m.role == "assistant" && m.status == "complete") afterReply(l, m)
            }
            "problems_changed" -> if (Push.choices.value.problems) loadProblems(l, quiet = false)
            "agent_updated" ->
                e["agent"]?.let { irisJson.decodeFromJsonElement<Agent>(it) }?.let { names[it.id] = it.name }
        }
    }

    /** A finished reply: a question gets its own kind; replies to Iris's hand-offs stay quiet. */
    private suspend fun afterReply(l: LinkConnection, m: Message) {
        val detail = get(l, "/v1/threads/${m.threadId}")?.let { irisJson.decodeFromJsonElement<ThreadDetail>(it) } ?: return
        val agentId = detail.thread.agentId
        val tag = "reply-$agentId"
        val step = m.rounds.lastOrNull()?.tools?.lastOrNull { it.name == "ask_user" }
        val question = step?.let {
            runCatching { irisJson.parseToJsonElement(it.arguments).jsonObject["question"]?.jsonPrimitive?.content }.getOrNull()
        }
        if (question != null) {
            if (Push.choices.value.questions) Push.show(this, "questions", "${name(agentId)} asks", question, tag, agentId)
            return
        }
        if (!Push.choices.value.replies) return
        val i = detail.messages.indexOfFirst { it.id == m.id }
        val trigger = detail.messages.take(if (i < 0) detail.messages.size else i).lastOrNull { it.role == "user" }
        if (trigger?.content?.startsWith("[Task from ") == true) return
        val body = snippet(m.content).ifEmpty { return }
        Push.show(this, "replies", name(agentId), body, tag, agentId)
    }

    private suspend fun loadProblems(l: LinkConnection, quiet: Boolean) {
        val list = get(l, "/v1/problems")?.let { irisJson.decodeFromJsonElement<List<Problem>>(it) } ?: return
        for (p in list) {
            if (!seenProblems.add(p.id) || quiet || !Push.choices.value.problems) continue
            Push.show(this, "problems", p.title.ifEmpty { "Something failed" }, p.detail.ifEmpty { name(p.agentId) }, "problem-${p.id}", p.agentId)
        }
    }

    /** A reply as one plain line (no Markdown), like the engine's snippet. */
    private fun snippet(text: String): String {
        val plain = text.lines()
            .map { it.trim().trimStart('#', '>', '-', '*', ' ').trim() }
            .filter { it.isNotEmpty() && !it.startsWith("```") }
            .joinToString(" ")
            .replace("**", "").replace("`", "")
        return if (plain.length <= 180) plain else plain.take(179).trimEnd() + "…"
    }
}

/** After a restart or an app update: back on if notifications were on. */
class PushBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Push.load(context)
    }
}
