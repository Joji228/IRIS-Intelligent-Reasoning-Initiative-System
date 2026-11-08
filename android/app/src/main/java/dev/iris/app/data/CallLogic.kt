package dev.iris.app.data

/**
 * The parts of a voice call that need no Android classes, so they are tested on the JVM
 * (docs/VOICE-PLAN.md, Phase 4): finding where a spoken turn starts and ends, what the call
 * is doing, and what gets said aloud.
 */

/** What happened in the microphone stream. */
sealed interface TurnEvent {
    /** Someone started talking (speech lasted long enough to count). */
    data object Started : TurnEvent

    /**
     * The turn is over (a long enough pause, or the longest turn allowed): the audio of it, and
     * how many of its 20 ms frames the detector took for speech.
     */
    class Ended(val pcm: ShortArray, val speechFrames: Int = 0) : TurnEvent
}

/**
 * Cuts the microphone stream into turns using a [VoiceDetector]. Feed it 20 ms frames, one
 * after another.
 *
 * - A turn **starts** after [startSpeechMs] of speech (a few quiet frames inside it are forgiven
 *   while [tolerant] is on).
 * - It **ends** after [endSilenceMs] without speech, or at [maxTurnMs].
 * - The audio kept starts [preRollMs] before the speech did, so first words aren't cut off.
 *
 * While the agent is talking, the call raises [startSpeechMs] and switches [tolerant] off: its own
 * voice leaking back into the microphone then needs to look like real, steady speech to count.
 */
class TurnDetector(
    private val detector: VoiceDetector,
    var endSilenceMs: Int = 800,
    private val preRollMs: Int = 300,
    private val maxTurnMs: Int = 30_000,
) {
    var startSpeechMs = 250
    var tolerant = true

    private val ring = ArrayDeque<ShortArray>()
    private val turn = ArrayList<ShortArray>()
    private var run = 0
    private var gap = 0
    private var sinceRun = 0
    private var silence = 0
    private var speechFrames = 0

    /** True from [TurnEvent.Started] until [TurnEvent.Ended]. */
    var inTurn = false
        private set

    /** Forgets everything heard so far (the microphone was muted, or the call restarted). */
    fun reset() {
        ring.clear()
        turn.clear()
        run = 0
        gap = 0
        sinceRun = 0
        silence = 0
        speechFrames = 0
        inTurn = false
    }

    fun feed(frame: ShortArray): TurnEvent? {
        val copy = frame.copyOf() // the recorder reuses its buffer
        val voice = detector.isSpeech(frame)
        if (!inTurn) {
            val startFrames = (startSpeechMs + FRAME_MS - 1) / FRAME_MS
            ring.addLast(copy)
            while (ring.size > RING_FRAMES) ring.removeFirst()
            if (voice) {
                run++
                gap = 0
            } else {
                gap++
                if (!tolerant || gap > MAX_GAP) run = 0
            }
            // Frames since the speech began, quiet ones inside it included.
            sinceRun = if (run > 0) sinceRun + 1 else 0
            if (run < startFrames) return null
            // The clip starts [preRollMs] before the speech did.
            val keep = (sinceRun + preRollMs / FRAME_MS).coerceAtMost(ring.size)
            turn.addAll(ring.toList().takeLast(keep))
            ring.clear()
            speechFrames = run
            run = 0
            gap = 0
            sinceRun = 0
            silence = 0
            inTurn = true
            return TurnEvent.Started
        }
        turn.add(copy)
        if (voice) {
            silence = 0
            speechFrames++
        } else {
            silence++
        }
        if (silence * FRAME_MS < endSilenceMs && turn.size * FRAME_MS < maxTurnMs) return null
        // Keep a little of the trailing quiet, not all of it.
        val extra = (silence - KEEP_TAIL_FRAMES).coerceAtLeast(0)
        val kept = turn.subList(0, turn.size - extra)
        val pcm = ShortArray(kept.size * Recorder.FRAME_SAMPLES)
        kept.forEachIndexed { i, f -> f.copyInto(pcm, i * Recorder.FRAME_SAMPLES) }
        val voiced = speechFrames
        reset()
        return TurnEvent.Ended(pcm, voiced)
    }

    companion object {
        const val FRAME_MS = 20

        /** The last two seconds of audio waiting to see whether speech begins. */
        private const val RING_FRAMES = 100

        /** Quiet frames allowed inside the start of speech (60 ms). */
        private const val MAX_GAP = 3

        /** 200 ms of quiet is kept at the end of a turn. */
        private const val KEEP_TAIL_FRAMES = 10
    }
}

/** What the call looks like to the person on it (and to the call screen). */
enum class CallPhase { Listening, YouSpeaking, Thinking, Speaking }

/** One phase from the pieces of the call that are going on. Speaking wins, then you talking, then thinking. */
fun callPhase(speaking: Boolean, userSpeaking: Boolean, busy: Boolean): CallPhase = when {
    speaking -> CallPhase.Speaking
    userSpeaking -> CallPhase.YouSpeaking
    busy -> CallPhase.Thinking
    else -> CallPhase.Listening
}

/** Empty results, and ones of one or two characters, are noise: back to listening. */
fun isUsableTranscript(text: String): Boolean = text.count { it.isLetterOrDigit() } >= 3

/**
 * What Whisper tends to "hear" in silence and noise: a polite closing line or a subtitle credit.
 * The server drops these too (voice.rs); the phone checks as well, so it holds even with an older
 * server. Lowercase, no punctuation.
 */
private val PHANTOM_PHRASES = listOf(
    "you", "thank you", "thanks", "thank you very much", "thank you so much", "thanks for watching",
    "thank you for watching", "thank you so much for watching", "thanks for listening", "thank you for listening",
    "please subscribe", "bye", "bye bye", "subtitles by the amara org community",
    "ευχαριστώ", "ευχαριστώ πολύ", "ευχαριστούμε", "ευχαριστούμε πολύ", "ευχαριστώ που παρακολουθήσατε",
    "ευχαριστούμε που παρακολουθήσατε", "ευχαριστώ για την προσοχή σας", "τα λέμε στο επόμενο βίντεο",
    "υπότιτλοι authorwave", "υπότιτλοι amara org", "υπότιτλοι από την κοινότητα του amara org",
).sortedByDescending { it.length }

/** True if the text is only such phrases, once or repeated ("Thank you. Thank you."). A real sentence that starts with one is not. */
fun isPhantomPhrase(text: String): Boolean {
    var rest = plainText(text)
    if (rest.isEmpty()) return false
    while (rest.isNotEmpty()) {
        val hit = PHANTOM_PHRASES.firstOrNull { p -> rest == p || rest.startsWith("$p ") } ?: return false
        rest = rest.removePrefix(hit).trim()
    }
    return true
}

/** How loud the speech in a turn is: the average level (RMS, of 32768) of its [speechFrames] loudest 20 ms frames. */
fun speechLevel(pcm: ShortArray, speechFrames: Int): Double {
    val size = Recorder.FRAME_SAMPLES
    val levels = (0 until pcm.size / size).map { f ->
        var sum = 0.0
        for (i in f * size until (f + 1) * size) sum += pcm[i].toDouble() * pcm[i]
        Math.sqrt(sum / size)
    }.sortedDescending()
    if (levels.isEmpty()) return 0.0
    return levels.take(speechFrames.coerceIn(1, levels.size)).average()
}

/**
 * Whether a recorded turn is worth sending to the speech service at all. Whisper invents words
 * ("Thank you.") for a faint noise or a click, so a turn has to be real speech: at least 0.3 s of
 * it, making up a fair part of the clip, and not just a whisper-quiet hum.
 */
fun isWorthTranscribing(pcm: ShortArray, speechFrames: Int, minLevel: Double = MIN_SPEECH_LEVEL): Boolean {
    if (pcm.isEmpty() || speechFrames * TurnDetector.FRAME_MS < 300) return false
    if (speechFrames.toDouble() * Recorder.FRAME_SAMPLES / pcm.size < 0.25) return false
    return speechLevel(pcm, speechFrames) >= minLevel
}

/** About -41 dBFS: below this, a "voice" is a hum, a far-off sound, or what is left of an echo. */
const val MIN_SPEECH_LEVEL = 300.0

/** Lowercase letters and digits with single spaces: what two texts are compared by. */
fun plainText(s: String): String =
    s.lowercase().map { if (it.isLetterOrDigit()) it else ' ' }.joinToString("").trim().replace(Regex("\\s+"), " ")

/**
 * Whether what the speech service heard is just the agent's own voice coming back through the
 * microphone: it is part of a sentence the phone said a moment ago (or repeats one whole).
 */
fun looksLikeEcho(heard: String, recentlySpoken: List<String>): Boolean {
    val h = plainText(heard)
    if (h.length < 3) return false
    return recentlySpoken.map(::plainText).any { s -> s.contains(h) || (s.length >= 12 && h.contains(s)) }
}

private fun isLatin(c: Char) = c in 'A'..'Z' || c in 'a'..'z' || c in 'À'..'ɏ'
private fun isGreek(c: Char) = c in 'Ͱ'..'Ͽ' || c in 'ἀ'..'῿'

/**
 * Whether text is in a script other than Latin or Greek (Cyrillic, Chinese, Arabic…): calls are
 * English and Greek only, so such text is a mistake, never something to say aloud.
 */
fun hasForeignScript(text: String): Boolean {
    val letters = text.filter { it.isLetter() }
    if (letters.isEmpty()) return false
    return letters.count { !isLatin(it) && !isGreek(it) } * 2 > letters.length
}

/** What a failed step of a call is told as, and how the call goes on. */
enum class CallProblem(val spoken: String) {
    /** Groq's free speech limit: say so, and try the same clip again after a pause. */
    SpeechLimit("I've hit the free speech limit; give me a minute."),

    /** The link to the server dropped: say so, keep the call, try again. */
    Offline("I lost the connection to the server."),

    /** Anything else (the clip couldn't be turned into text). */
    Other("Sorry, I couldn't make that out."),
}

fun classifyProblem(e: Throwable): CallProblem {
    val status = (e as? ApiException)?.status
    return when {
        status == 429 -> CallProblem.SpeechLimit
        status == 0 || e.message?.startsWith("Not connected") == true -> CallProblem.Offline
        else -> CallProblem.Other
    }
}

/** Where the call's sound goes. */
enum class AudioRoute { Earpiece, Speaker, Bluetooth }

/** The loudspeaker switch wins; otherwise a connected Bluetooth headset; otherwise the earpiece. */
fun chooseRoute(loudspeaker: Boolean, bluetooth: Boolean): AudioRoute = when {
    loudspeaker -> AudioRoute.Speaker
    bluetooth -> AudioRoute.Bluetooth
    else -> AudioRoute.Earpiece
}

/**
 * Decides when to say what the agent is doing ("Searching the web"): at most one phrase every
 * [gapMs], never over a sentence being spoken, never the same phrase twice in a row, and only
 * for a real step (not plain thinking or waiting for an approval).
 */
class ProgressGate(private val gapMs: Long = 8_000) {
    private var lastAt = Long.MIN_VALUE / 2
    private var lastPhrase = ""

    /** The phrase to say now for [activity], or null to stay quiet. */
    fun next(now: Long, activity: String?, enabled: Boolean, speakingNow: Boolean): String? {
        if (!enabled || speakingNow || activity.isNullOrEmpty()) return null
        if (activity in QUIET_STEPS) return null
        if (now - lastAt < gapMs) return null
        val phrase = progressPhrase(activity) ?: return null
        if (phrase == lastPhrase) return null
        lastAt = now
        lastPhrase = phrase
        return phrase
    }

    /** A new reply starts: the next step may be announced again. */
    fun reset() {
        lastPhrase = ""
    }

    private companion object {
        val QUIET_STEPS = setOf("thinking", "waiting", "compacting", "reviewing", "ask_user")
    }
}

/** The orb's short phrase for a step, as speech: "Searching the web…" becomes "Searching the web". */
fun progressPhrase(activity: String): String? =
    phrases(orbState(activity, working = true), activity).firstOrNull()?.trimEnd('…', '.', ' ')?.takeIf { it.isNotEmpty() }

/**
 * Turns the `message_delta` events of a call's replies into sentences to say. Reasoning never
 * gets here (only `content`). A new round (the agent used a tool and went on) says what is left
 * of the earlier one first, since it may have had no full stop ("On it, give me a minute").
 */
class ReplySpeech {
    private var messageId: String? = null
    private var round = 0
    private var stream = SpeechStream()

    /** Set when the user talked over this reply: the rest of it is not spoken. */
    @Volatile
    private var silenced = false

    fun onDelta(messageId: String, round: Int, content: String): List<String> {
        if (content.isEmpty()) return emptyList()
        val out = ArrayList<String>()
        if (messageId != this.messageId) {
            out += finishQuietly()
            this.messageId = messageId
            this.round = round
            stream = SpeechStream()
            silenced = false
        } else if (round != this.round) {
            out += stream.flush()
            this.round = round
        }
        out += stream.push(content)
        return if (silenced) emptyList() else out
    }

    /** The reply is over: whatever is left. */
    fun onEnd(): List<String> = finishQuietly()

    /** The user talked over the reply: drop what is queued in it, and stay quiet until the next reply. */
    fun silence() {
        silenced = true
    }

    private fun finishQuietly(): List<String> {
        val rest = stream.flush()
        return if (silenced) emptyList() else rest
    }
}
