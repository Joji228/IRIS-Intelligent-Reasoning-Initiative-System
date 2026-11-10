package dev.iris.app.data

import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.speech.tts.Voice
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.util.Locale

/** One text-to-speech voice the phone has, as Settings → Voice & calls lists it. */
data class VoiceChoice(
    val name: String,
    val label: String,
    /** Works without a connection: it starts faster and survives bad signal. */
    val offline: Boolean,
    /** Its data is on the phone (otherwise Android would have to download it). */
    val installed: Boolean,
)

/** Whether a language can be spoken right now. */
enum class VoiceAvailability { Checking, Ready, NeedsInstall, Unsupported }

/**
 * Best voices first: installed before not installed, working offline before needing a
 * connection, then by name so the order is stable.
 */
fun sortVoices(voices: List<VoiceChoice>): List<VoiceChoice> =
    voices.sortedWith(compareByDescending<VoiceChoice> { it.installed }.thenByDescending { it.offline }.thenBy { it.name })

/**
 * The phone's own text-to-speech (the Google or Samsung engine). Nothing is downloaded by the
 * app; Android manages the voices. The audio is marked as voice-communication audio, so the
 * phone's echo canceller knows what the speaker is playing during a call.
 */
class Speaker(
    context: Context,
    /**
     * How the audio is classed. Calls use voice-communication audio (the phone's echo canceller needs to
     * know it); the test buttons in Settings use media audio, which plays on the loudspeaker, not the earpiece.
     */
    private val usage: Int = AudioAttributes.USAGE_VOICE_COMMUNICATION,
) {
    private val appContext = context.applicationContext
    private var tts: TextToSpeech? = null
    private val ready = MutableStateFlow(false)

    /** True once Android's engine has started (it takes a moment). */
    val isReady: StateFlow<Boolean> = ready

    init {
        tts = TextToSpeech(appContext) { status ->
            val engine = tts
            if (status == TextToSpeech.SUCCESS && engine != null) {
                engine.setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(usage)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build(),
                )
                listen(engine)
                ready.value = true
            }
        }
    }

    /** The voices for one language ("en" or "el"), best first. Empty until the engine is ready. */
    fun voices(lang: String): List<VoiceChoice> {
        val engine = tts?.takeIf { ready.value } ?: return emptyList()
        val all = runCatching { engine.voices }.getOrNull().orEmpty()
        return sortVoices(
            all.filter { it.locale.language == lang }.map { v ->
                VoiceChoice(
                    name = v.name,
                    label = voiceLabel(v),
                    offline = !v.isNetworkConnectionRequired,
                    installed = !v.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED),
                )
            },
        )
    }

    /** Whether the language can be spoken: a voice with its data on the phone is what counts. */
    fun availability(lang: String): VoiceAvailability {
        val engine = tts?.takeIf { ready.value } ?: return VoiceAvailability.Checking
        val support = engine.isLanguageAvailable(Locale.forLanguageTag(lang))
        if (support == TextToSpeech.LANG_NOT_SUPPORTED) return VoiceAvailability.Unsupported
        val installed = voices(lang).any { it.installed }
        return if (installed || support >= TextToSpeech.LANG_AVAILABLE) VoiceAvailability.Ready else VoiceAvailability.NeedsInstall
    }

    // ---- speaking: a queue of sentences ----

    /** Bumped by [stop], so callbacks about sentences that were cut off are ignored. */
    private val generation = AtomicInteger(0)
    private val counter = AtomicInteger(0)
    private val waiting = AtomicInteger(0)
    private val saying = ConcurrentHashMap<String, String>()
    private val speakingNow = MutableStateFlow(false)
    private val nowSayingText = MutableStateFlow("")

    /** True while any sentence is being spoken or waits in the queue. */
    val speaking: StateFlow<Boolean> = speakingNow

    /** The sentence being spoken right now (for the call screen's caption); stays on the last one. */
    val nowSaying: StateFlow<String> = nowSayingText

    private fun listen(engine: TextToSpeech) {
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String) {
                if (current(utteranceId)) saying[utteranceId]?.let { nowSayingText.value = it }
            }

            override fun onDone(utteranceId: String) = finished(utteranceId)

            @Deprecated("Replaced by onError(String, Int)")
            override fun onError(utteranceId: String) = finished(utteranceId)

            override fun onError(utteranceId: String, errorCode: Int) = finished(utteranceId)

            override fun onStop(utteranceId: String, interrupted: Boolean) = finished(utteranceId)
        })
    }

    private fun current(id: String) = id.startsWith("iris-${generation.get()}-")

    private fun finished(id: String) {
        saying.remove(id)
        if (!current(id)) return
        if (waiting.decrementAndGet() <= 0) {
            waiting.set(0)
            speakingNow.value = false
        }
    }

    /**
     * Adds one sentence to the queue, in the language it is written in ([languageOf]) with the
     * voice and speed chosen in Settings → Voice & calls. [flush] drops whatever is queued first.
     */
    fun say(text: String, settings: VoiceSettings, flush: Boolean = false) {
        val engine = tts?.takeIf { ready.value } ?: return
        if (text.isBlank()) return
        if (flush) stop()
        val lang = languageOf(text)
        val voiceName = settings.voiceFor(lang)
        val voice = voiceName.takeIf { it.isNotEmpty() }?.let { n -> runCatching { engine.voices }.getOrNull()?.firstOrNull { it.name == n } }
        if (voice != null) engine.voice = voice else engine.language = Locale.forLanguageTag(lang)
        engine.setSpeechRate(settings.speechRate)
        val id = "iris-${generation.get()}-${counter.incrementAndGet()}"
        saying[id] = text
        waiting.incrementAndGet()
        speakingNow.value = true
        if (engine.speak(text, TextToSpeech.QUEUE_ADD, Bundle.EMPTY, id) != TextToSpeech.SUCCESS) finished(id)
    }

    fun sayAll(sentences: List<String>, settings: VoiceSettings) = sentences.forEach { say(it, settings) }

    /** Stops talking at once and forgets the queue (an interruption, or hanging up). */
    fun stop() {
        generation.incrementAndGet()
        waiting.set(0)
        saying.clear()
        speakingNow.value = false
        runCatching { tts?.stop() }
    }

    fun shutdown() {
        stop()
        runCatching { tts?.shutdown() }
        tts = null
        ready.value = false
    }

    /** Opens Android's screen for downloading a voice's data. */
    fun installIntent(): Intent = Intent(TextToSpeech.Engine.ACTION_INSTALL_TTS_DATA).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    private fun voiceLabel(v: Voice): String {
        val where = v.locale.getDisplayCountry(Locale.ENGLISH).takeIf { it.isNotBlank() }
        val kind = if (v.isNetworkConnectionRequired) "online" else "on this phone"
        // The engine's own name (el-gr-x-ell-local) is ugly but unique, so it ends the label.
        return listOfNotNull(where, kind, v.name).joinToString(" · ")
    }
}
