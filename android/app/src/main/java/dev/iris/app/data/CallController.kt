package dev.iris.app.data

import android.content.Context
import android.content.Intent
import dev.iris.app.link.LinkCrypto
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonPrimitive

/** What a call needs from the app (the view model provides it), so the call logic stays apart from the UI. */
interface CallHost {
    /** The engine's live events (deltas, runs, approvals). */
    val events: SharedFlow<JsonObject>

    /** True while the link to the server is up. */
    val online: Flow<Boolean>

    /** The agent's chat thread, once it is loaded. */
    fun threadId(agentId: String): String?

    /** One clip (a WAV, base64) to text. */
    suspend fun transcribe(wavBase64: String, language: String): Result<String>

    /** Posts a message in the agent's chat; true if it went through. */
    suspend fun send(agent: Agent, text: String): Boolean

    /** Stops the agent's reply in progress. */
    fun stopReply()

    /** The question of an `ask_user` step ending the agent's latest reply, if there is one. */
    fun pendingQuestion(agentId: String): String?

    /** The call is over (hung up, or the app is closing). */
    fun onCallEnded(call: CallController)
}

/** The call in progress, if any: the service and the screen find it here. */
object Calls {
    @Volatile
    var active: CallController? = null
}

/**
 * A voice call with one agent (docs/VOICE-PLAN.md, Phase 4). It listens to the microphone, cuts
 * the audio into turns, has the server turn each turn into text, posts it in the agent's chat as
 * "[Voice call] …", and reads the streamed reply aloud sentence by sentence. Tools, approvals and
 * memory work as when typing: the call is the agent's ordinary chat.
 *
 * Interrupting: talking over the agent stops its voice at once (and the rest of that reply is not
 * spoken); its work goes on. Approvals are never given by voice: they show on the call screen.
 */
class CallController(
    context: Context,
    private val host: CallHost,
    val agent: Agent,
    scope: CoroutineScope,
) {
    private val appContext = context.applicationContext
    private val job = SupervisorJob(scope.coroutineContext[Job])
    private val cs = CoroutineScope(scope.coroutineContext + job)

    val speaker = Speaker(appContext)
    private val audio = CallAudio(appContext)
    private val reply = ReplySpeech()
    private val progress = ProgressGate()
    private val settings get() = VoicePrefs.settings.value

    /** When the call started (the timer counts from here). */
    val startedAt = System.currentTimeMillis()

    private val mutedNow = MutableStateFlow(false)
    private val userSpeaking = MutableStateFlow(false)
    private val busyTurns = MutableStateFlow(0)
    private val working = MutableStateFlow(false)
    private val endedNow = MutableStateFlow(false)
    private val youSaid = MutableStateFlow("")
    private val problem = MutableStateFlow<String?>(null)

    val muted: StateFlow<Boolean> = mutedNow
    val ended: StateFlow<Boolean> = endedNow

    /** The last thing you said, as the server heard it. */
    val lastYouSaid: StateFlow<String> = youSaid

    /** The sentence being spoken, or the last one. */
    val lastIrisSaid: StateFlow<String> get() = speaker.nowSaying

    /** Something is wrong that the person should see (the microphone, the connection); null when all is well. */
    val trouble: StateFlow<String?> = problem

    /** Where the sound goes (earpiece, loudspeaker, headset). */
    val route: StateFlow<AudioRoute> get() = audio.route

    val phase: StateFlow<CallPhase> = combine(speaker.speaking, userSpeaking, busyTurns, working) { s, u, b, w ->
        callPhase(speaking = s, userSpeaking = u, busy = b > 0 || w)
    }.stateIn(cs, SharingStarted.Eagerly, CallPhase.Listening)

    /** Spoken sentences wait here, in order, until the voice engine is ready and nobody is talking over them. */
    private val sayQueue = Channel<String>(Channel.UNLIMITED)

    /** How many sentences are queued or being handed to the voice. */
    private val sayPending = AtomicInteger(0)

    /** A recorded turn, and whether it happened while (or just after) the agent was talking, so it may be its own echo. */
    private class Clip(val pcm: ShortArray, val echoRisk: Boolean)

    /** Recorded turns wait here for the speech service, in order. */
    private val clips = Channel<Clip>(8, BufferOverflow.DROP_OLDEST)

    /** The last few sentences the phone said, to tell its own echo from you. */
    private val recentlySpoken = ArrayDeque<String>()
    private val toldAt = HashMap<String, Long>()
    private val announced = HashSet<String>()

    /** Starts listening. False if the microphone isn't allowed. */
    fun start(): Boolean {
        if (!Recorder.hasPermission(appContext)) return false
        VoicePrefs.load(appContext)
        Calls.active = this
        Push.inCall.value = true
        audio.start(settings.loudspeaker)
        cs.launch(Dispatchers.IO) { listen() }
        cs.launch { transcribeClips() }
        cs.launch { speakQueued() }
        cs.launch { watchEvents() }
        cs.launch { watchLink() }
        return true
    }

    fun toggleMute() {
        mutedNow.update { !it }
    }

    fun toggleLoudspeaker() = audio.setLoudspeaker(audio.route.value != AudioRoute.Speaker)

    /** The Stop button: stops the agent's work and its voice. */
    fun stopReply() {
        host.stopReply()
        silenceNow()
    }

    fun hangUp() {
        if (endedNow.value) return
        endedNow.value = true
        job.cancel()
        speaker.shutdown()
        audio.stop()
        Push.inCall.value = false
        if (Calls.active === this) Calls.active = null
        appContext.stopService(Intent(appContext, CallService::class.java))
        host.onCallEnded(this)
    }

    // ---- the microphone: turns ----------------------------------------------------------

    private suspend fun listen() {
        val recorder = Recorder()
        if (!recorder.open() || !recorder.start()) {
            problem.value = "Couldn't open the microphone. Another app may be using it."
            recorder.close()
            return
        }
        val detector = WebRtcVoiceDetector()
        val turns = TurnDetector(detector)
        val frame = ShortArray(Recorder.FRAME_SAMPLES)
        // The speaker's own voice leaks into the microphone a moment after it stops.
        var echoUntil = 0L
        var turnEchoRisk = false
        try {
            while (cs.isActive) {
                if (recorder.read(frame) < 0) {
                    problem.value = "The microphone stopped working."
                    return
                }
                val now = System.currentTimeMillis()
                if (mutedNow.value) {
                    if (turns.inTurn) turns.reset()
                    userSpeaking.value = false
                    continue
                }
                val s = settings
                if (speaker.speaking.value) echoUntil = now + ECHO_TAIL_MS
                val echoRisk = now < echoUntil
                // Interrupting is switched off: nothing is heard while the agent talks.
                if (echoRisk && !s.interrupt && !turns.inTurn) continue
                turns.endSilenceMs = s.endOfTurnMs
                // Against echo leftovers the rule is stricter: longer, steady speech.
                turns.startSpeechMs = if (echoRisk) 400 else 250
                turns.tolerant = !echoRisk
                when (val event = turns.feed(frame)) {
                    TurnEvent.Started -> {
                        userSpeaking.value = true
                        turnEchoRisk = echoRisk
                        if (echoRisk) silenceNow()
                    }
                    is TurnEvent.Ended -> {
                        userSpeaking.value = false
                        // A faint hum or a click isn't speech: Whisper would invent words ("Thank you.") for it.
                        if (isWorthTranscribing(event.pcm, event.speechFrames)) {
                            clips.trySend(Clip(event.pcm, turnEchoRisk || speaker.speaking.value))
                        }
                    }
                    null -> {}
                }
            }
        } finally {
            userSpeaking.value = false
            detector.close()
            recorder.close()
        }
    }

    /** Stops the voice now and drops what was queued; the rest of this reply stays unspoken. */
    private fun silenceNow() {
        reply.silence()
        while (sayQueue.tryReceive().isSuccess) sayPending.decrementAndGet()
        speaker.stop()
    }

    // ---- from a turn to a message ---------------------------------------------------------

    private suspend fun transcribeClips() {
        for (clip in clips) {
            busyTurns.update { it + 1 }
            try {
                handle(clip)
            } finally {
                busyTurns.update { it - 1 }
            }
        }
    }

    private suspend fun handle(clip: Clip) {
        val data = LinkCrypto.b64(Wav.encode(clip.pcm))
        var tries = 0
        while (cs.isActive) {
            val result = host.transcribe(data, settings.language)
            val text = result.getOrNull()
            if (text != null) {
                if (!isUsableTranscript(text) || isPhantomPhrase(text)) return
                // English and Greek only: another script is a mistake of the speech service.
                if (hasForeignScript(text)) return
                // The agent's own voice, heard back through the microphone, is not you.
                if (clip.echoRisk && looksLikeEcho(text, recentlySpoken.toList())) return
                youSaid.value = text.trim()
                post(text.trim())
                return
            }
            val error = result.exceptionOrNull() ?: return
            tries++
            when (classifyProblem(error)) {
                CallProblem.SpeechLimit -> {
                    // The free limit is per minute and per day: say so, wait, and send the same clip again.
                    tellOnce(CallProblem.SpeechLimit.spoken, 60_000)
                    if (tries >= 4) return
                    delay(30_000)
                }
                CallProblem.Offline -> {
                    tellOnce(CallProblem.Offline.spoken, 30_000)
                    if (tries >= 12) return
                    delay(5_000)
                }
                CallProblem.Other -> {
                    // The server's own words are short and speakable ("Groq refused the key; check it on the server").
                    tellOnce(error.message?.takeIf { it.length in 3..120 } ?: CallProblem.Other.spoken, 30_000)
                    return
                }
            }
        }
    }

    private suspend fun post(text: String) {
        repeat(3) { attempt ->
            if (host.send(agent, "${Prefixes.VOICE} $text")) return
            if (attempt == 0) tellOnce(CallProblem.Offline.spoken, 30_000)
            delay(3_000)
        }
    }

    /** Says [text] unless it was said within the last [withinMs]. */
    private fun tellOnce(text: String, withinMs: Long) {
        val now = System.currentTimeMillis()
        if (now - (toldAt[text] ?: 0L) < withinMs) return
        toldAt[text] = now
        say(text)
    }

    private fun say(text: String) {
        sayPending.incrementAndGet()
        sayQueue.trySend(text)
    }

    // ---- the reply: events from the server ---------------------------------------------------

    private suspend fun watchEvents() {
        host.events.collect { e ->
            when (e.type) {
                "run_started" -> if (e.str("agent_id") == agent.id) {
                    working.value = true
                    progress.reset()
                }
                "run_activity" -> if (e.str("agent_id") == agent.id) onActivity(e.str("activity"))
                "message_delta" -> {
                    if (e.str("thread_id") != host.threadId(agent.id)) return@collect
                    val id = e.str("message_id") ?: return@collect
                    // Only the reply's own words, never its thinking.
                    val said = reply.onDelta(id, e["round"]?.jsonPrimitive?.int ?: 0, e.str("content").orEmpty())
                    said.forEach(::say)
                }
                "approval_requested" -> {
                    val approval = e.obj("approval")
                    val id = approval?.str("id").orEmpty()
                    if (approval?.str("agent_id") == agent.id && announced.add(id)) say(APPROVAL_LINE)
                }
                "run_finished" -> if (e.str("agent_id") == agent.id) onFinished(e.str("status"))
            }
        }
    }

    private fun onActivity(activity: String?) {
        val phrase = progress.next(System.currentTimeMillis(), activity, settings.sayProgress, speaker.speaking.value || sayPending.get() > 0)
        if (phrase != null) say(phrase)
    }

    private suspend fun onFinished(status: String?) {
        working.value = false
        reply.onEnd().forEach(::say)
        when (status) {
            "error" -> say("Something went wrong with that reply. It's in your inbox.")
            "complete" -> {
                // A question for you (ask_user) lives in the tool step, not the text: say it too.
                delay(500)
                host.pendingQuestion(agent.id)?.let { say(speakable(it)) }
            }
        }
    }

    private suspend fun watchLink() {
        var wasOnline = true
        host.online.distinctUntilChanged().collect { online ->
            if (!online && wasOnline) {
                // A blip of a couple of seconds isn't worth a sentence.
                delay(3_000)
                if (!host.online.first()) tellOnce(CallProblem.Offline.spoken, 30_000)
            }
            wasOnline = online
        }
    }

    // ---- speaking, one sentence at a time ----------------------------------------------------

    private suspend fun speakQueued() {
        for (sentence in sayQueue) {
            // The voice engine takes a moment to start; nothing is spoken over you.
            withTimeoutOrNull(5_000) { speaker.isReady.first { it } }
            while (userSpeaking.value && cs.isActive) delay(50)
            speaker.say(sentence, settings)
            recentlySpoken.addLast(sentence)
            while (recentlySpoken.size > 8) recentlySpoken.removeFirst()
            sayPending.decrementAndGet()
        }
    }

    private companion object {
        const val ECHO_TAIL_MS = 300L
        const val APPROVAL_LINE = "I need your OK on the screen."
    }
}
