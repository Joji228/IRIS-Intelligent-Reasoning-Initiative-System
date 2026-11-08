package dev.iris.app.data

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** What Settings → Voice remembers on this phone (docs/VOICE-PLAN.md). */
data class VoiceSettings(
    /** What you speak: "" lets the speech service detect it per clip, or "el" / "en". */
    val language: String = "",
    /** The chosen text-to-speech voice per language; empty means the phone's default. */
    val voiceEn: String = "",
    val voiceEl: String = "",
    /** "slow", "normal" or "fast". */
    val speed: String = "normal",
    /** Talking over the agent stops it speaking. */
    val interrupt: Boolean = true,
    /** The agent says what it is doing ("Searching the web…") while it works. */
    val sayProgress: Boolean = true,
    /** How long a pause ends your turn: "short" (0.5 s), "normal" (0.8 s) or "long" (1.2 s). */
    val wait: String = "normal",
    /** Calls start on the loudspeaker instead of the earpiece. */
    val loudspeaker: Boolean = false,
) {
    /** The speech rate to give Android's TextToSpeech. */
    val speechRate: Float get() = SPEEDS[speed] ?: 1.0f

    /** The silence, in milliseconds, that ends a spoken turn. */
    val endOfTurnMs: Int get() = WAITS[wait] ?: 800

    fun voiceFor(lang: String): String = if (lang == "el") voiceEl else voiceEn

    companion object {
        val SPEEDS = mapOf("slow" to 0.85f, "normal" to 1.0f, "fast" to 1.2f)
        val WAITS = mapOf("short" to 500, "normal" to 800, "long" to 1200)
    }
}

/** "Fills up again in 5 h 12 min", or that it is full already, for the allowance bar. */
fun describeReset(l: VoiceLimits, nowSec: Long): String {
    if (l.remaining >= l.limit) return "The full allowance is available."
    val left = l.resetsAt - nowSec
    if (left <= 0) return "The full allowance is available."
    val minutes = (left + 59) / 60
    val text = when {
        minutes < 2 -> "about a minute"
        minutes < 60 -> "$minutes min"
        minutes < 24 * 60 -> "${minutes / 60} h" + if (minutes % 60 > 0) " ${minutes % 60} min" else ""
        else -> "${minutes / (24 * 60)} d ${minutes / 60 % 24} h"
    }
    return "Fills up again in $text."
}

/** Keeps [VoiceSettings] in the app's SharedPreferences file ("iris"), like the other phone-only choices. */
object VoicePrefs {
    private const val FILE = "iris"
    private val state = MutableStateFlow(VoiceSettings())
    val settings: StateFlow<VoiceSettings> = state
    private var loaded = false

    private fun prefs(context: Context) = context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** Reads the saved choices once (safe to call from every screen that needs them). */
    @Synchronized
    fun load(context: Context) {
        if (loaded) return
        val p = prefs(context)
        state.value = VoiceSettings(
            language = p.getString("voice.language", "") ?: "",
            voiceEn = p.getString("voice.voice_en", "") ?: "",
            voiceEl = p.getString("voice.voice_el", "") ?: "",
            speed = (p.getString("voice.speed", "normal") ?: "normal").takeIf { it in VoiceSettings.SPEEDS } ?: "normal",
            interrupt = p.getBoolean("voice.interrupt", true),
            sayProgress = p.getBoolean("voice.say_progress", true),
            wait = (p.getString("voice.wait", "normal") ?: "normal").takeIf { it in VoiceSettings.WAITS } ?: "normal",
            loudspeaker = p.getBoolean("voice.loudspeaker", false),
        )
        loaded = true
    }

    @Synchronized
    fun update(context: Context, change: (VoiceSettings) -> VoiceSettings) {
        load(context)
        val next = change(state.value)
        state.value = next
        prefs(context).edit()
            .putString("voice.language", next.language)
            .putString("voice.voice_en", next.voiceEn)
            .putString("voice.voice_el", next.voiceEl)
            .putString("voice.speed", next.speed)
            .putBoolean("voice.interrupt", next.interrupt)
            .putBoolean("voice.say_progress", next.sayProgress)
            .putString("voice.wait", next.wait)
            .putBoolean("voice.loudspeaker", next.loudspeaker)
            .apply()
    }
}
