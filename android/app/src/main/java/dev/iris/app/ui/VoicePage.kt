package dev.iris.app.ui

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import dev.iris.app.data.VoiceLimits
import dev.iris.app.data.describeReset
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.iris.app.data.ApiException
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.Recorder
import dev.iris.app.data.Speaker
import dev.iris.app.data.SpeechStream
import dev.iris.app.data.VoiceAvailability
import dev.iris.app.data.VoicePrefs
import dev.iris.app.data.VoiceStatus
import dev.iris.app.data.Wav
import dev.iris.app.link.LinkCrypto
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/** The longest "Try it" clip, and the shortest one worth sending (about a third of a second). */
private const val TRY_MAX_MS = 15_000L
private const val TRY_MIN_BYTES = 44 + Recorder.SAMPLE_RATE * 2 / 3

private val LANGUAGES = listOf("en" to "English", "el" to "Ελληνικά")
private val SAMPLES = mapOf(
    "en" to "Hello, this is Iris. Can you hear me?",
    "el" to "Γεια σου, είμαι η Ίρις. Με ακούς;",
)

private const val MIXED_SAMPLE =
    "Γεια σου! Σήμερα έχει 21,5 βαθμούς στην Αθήνα. The weather in London is cloudy, around 14.5 degrees. " +
        "Θες να σε ενημερώσω κάθε πρωί; Just say the word.\n[details]\nThis part is only shown in the chat and is not read aloud."

/**
 * A test button that shows what it is doing: while its sound plays it turns violet with a dancing
 * level indicator and says it can be tapped to stop; at rest it's the usual outlined button.
 */
@Composable
private fun TestButton(idle: String, active: Boolean, enabled: Boolean = true, onClick: () -> Unit) {
    val shape = RoundedCornerShape(16.dp)
    val bg by animateColorAsState(if (active) C.accent else Color.Transparent, label = "test-bg")
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp)
            .alpha(if (enabled) 1f else 0.5f)
            .background(bg, shape)
            .border(1.dp, if (active) Color.Transparent else C.lineStrong, shape)
            .press(enabled, onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (active) {
            Bars(Color.White, fast = true, still = false)
            Txt("Speaking… tap to stop", 14.5.sp, Color.White, FontWeight.Medium)
        } else {
            Icon(Lucide.Volume, C.text, 17.dp)
            Txt(idle, 14.5.sp, C.text, FontWeight.Medium)
        }
    }
}

/** How many spoken turns are left today, as a bar (green to amber to red as it runs low) and in words. */
@Composable
private fun AllowanceMeter(l: VoiceLimits) {
    val share = (l.remaining.toFloat() / l.limit).coerceIn(0f, 1f)
    val color = when {
        share < 0.10f -> C.danger
        share < 0.30f -> C.warn
        else -> C.success
    }
    val anim by animateFloatAsState(share, tween(600), label = "allowance")
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Txt("Spoken turns left today", 14.sp, C.text, FontWeight.Medium, Modifier.weight(1f))
            Txt("%,d of %,d".format(l.remaining, l.limit), 13.sp, C.muted)
        }
        Box(Modifier.fillMaxWidth().height(8.dp).background(C.hover, CircleShape)) {
            Box(Modifier.fillMaxWidth(anim).fillMaxHeight().background(color, CircleShape))
        }
        Txt(describeReset(l, System.currentTimeMillis() / 1000), 12.sp, C.faint)
    }
}

/** Settings → Voice & calls: speech to text (on the server, with Groq), the phone's voices, call options, and a test. */
@Composable
fun VoicePage(vm: IrisViewModel, back: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    LaunchedEffect(Unit) { VoicePrefs.load(context) }
    val settings by VoicePrefs.settings.collectAsState()
    fun change(f: (dev.iris.app.data.VoiceSettings) -> dev.iris.app.data.VoiceSettings) = VoicePrefs.update(context, f)

    // ---- speech to text status, from the server ----
    var status by remember { mutableStateOf<VoiceStatus?>(null) }
    var statusFailed by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        vm.quiet { voice() }.onSuccess { status = it }.onFailure { statusFailed = true }
    }

    // ---- the phone's voices ----
    // The test sounds come out of the loudspeaker (media audio); a call uses the call audio route instead.
    val speaker = remember { Speaker(context, android.media.AudioAttributes.USAGE_MEDIA) }
    DisposableEffect(Unit) { onDispose { speaker.shutdown() } }
    val engineReady by speaker.isReady.collectAsState()
    val saying by speaker.nowSaying.collectAsState()
    // Which test is playing: "en", "el" or "mixed". The buttons show it and turn into Stop.
    var testing by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(testing) {
        if (testing != null) {
            kotlinx.coroutines.delay(300)
            speaker.speaking.first { !it }
            testing = null
        }
    }
    // Looked again at when coming back from Android's voice installer.
    var refresh by remember { mutableIntStateOf(0) }
    LifecycleResumeEffect(Unit) {
        refresh++
        onPauseOrDispose {}
    }

    // ---- "Try it" ----
    var held by remember { mutableStateOf(false) }
    var sending by remember { mutableStateOf(false) }
    var heard by remember { mutableStateOf<String?>(null) }
    var tryNote by remember { mutableStateOf<String?>(null) }
    var tryFailed by remember { mutableStateOf(false) }
    val micPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) vm.showToast("Microphone allowed. Hold the button and speak.")
        else vm.showToast("The microphone is blocked for I.R.I.S. in Android's settings")
    }

    fun startTry() {
        if (held || sending) return
        if (!Recorder.hasPermission(context)) {
            micPermission.launch(Manifest.permission.RECORD_AUDIO)
            return
        }
        held = true
        heard = null
        tryNote = null
        tryFailed = false
        scope.launch {
            val wav = Recorder.recordClip(TRY_MAX_MS) { !held }
            held = false
            when {
                wav == null -> { tryNote = "Couldn't open the microphone (another app may be using it)"; tryFailed = true }
                wav.size < TRY_MIN_BYTES -> tryNote = "That was too short. Hold the button while you speak."
                else -> {
                    sending = true
                    val started = System.currentTimeMillis()
                    val result = vm.quiet { transcribe(LinkCrypto.b64(wav), Wav.MIME, settings.language) }
                    val seconds = (System.currentTimeMillis() - started) / 1000.0
                    result.onSuccess {
                        heard = it.text.ifBlank { null }
                        tryNote = (if (it.text.isBlank()) "Only silence or noise was heard. " else "") + "Phone → server → Groq and back: ${"%.1f".format(seconds)} s"
                        // That turn used some of the free allowance: show the new count.
                        vm.quiet { voice() }.onSuccess { s -> status = s }
                    }.onFailure {
                        tryFailed = true
                        tryNote = when ((it as? ApiException)?.status) {
                            0, null -> "Couldn't reach the server"
                            else -> it.message ?: "Something went wrong"
                        }
                    }
                    sending = false
                }
            }
        }
    }

    Page("Voice & calls", back, subtitle = "Speech, voices, calls") {
        item {
            Card(accent = true) {
                CardTitle("Speech to text", "Your voice becomes text on your server, using Groq's free Whisper.")
                val s = status
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    when {
                        s == null && !statusFailed -> { Spinner(14.dp, C.faint); Txt("Asking the server…", 13.5.sp, C.muted) }
                        s == null -> { Dot(C.danger); Txt("Couldn't reach the server", 13.5.sp, C.danger) }
                        s.available -> {
                            Dot(C.success)
                            Txt("${s.provider ?: "Speech service"} · ${s.model ?: "?"} · ready", 13.5.sp, C.success)
                        }
                        else -> {
                            Dot(C.warn)
                            Txt("Add a Groq key on the server (Settings → AI providers on the desktop app)", 13.5.sp, C.warn, lineHeight = 18.sp)
                        }
                    }
                }
                if (s?.available == true) {
                    val l = s.limits
                    if (l != null && l.limit > 0) AllowanceMeter(l)
                    else Hint("How many spoken turns you have left today shows here after your first one. Try the button at the bottom.")
                }
                Hint("Your voice clips go through your server to Groq to be turned into text. Groq's free plan allows up to 2,000 spoken turns a day.")
                Column {
                    Label("What you speak")
                    Chips(listOf("" to "English + Greek", "el" to "Ελληνικά", "en" to "English"), settings.language) { v -> change { it.copy(language = v) } }
                    Box(Modifier.padding(top = 8.dp)) {
                        Hint("Calls understand English and Greek only. \"English + Greek\" picks the right one for each thing you say; pick one language if it ever gets confused.")
                    }
                }
            }
        }

        item {
            Card {
                CardTitle("Voices", "The phone speaks the replies with its own voices. Nothing is downloaded by I.R.I.S.")
                for ((lang, name) in LANGUAGES) {
                    val availability = remember(engineReady, refresh) { speaker.availability(lang) }
                    val voices = remember(engineReady, refresh) { speaker.voices(lang) }
                    val chosen = settings.voiceFor(lang)
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Txt(name, 15.sp, C.text, FontWeight.Medium, Modifier.weight(1f))
                            when (availability) {
                                VoiceAvailability.Checking -> Spinner(14.dp, C.faint)
                                VoiceAvailability.Ready -> Badge("Installed", C.success, Lucide.Check)
                                VoiceAvailability.NeedsInstall -> Badge("Not installed", C.warn)
                                VoiceAvailability.Unsupported -> Badge("Not supported", C.danger)
                            }
                        }
                        if (availability == VoiceAvailability.NeedsInstall) {
                            Button(
                                "Install the $name voice",
                                {
                                    runCatching { context.startActivity(speaker.installIntent()) }
                                        .onFailure { vm.showToast("This phone has no voice installer. Check Android's text-to-speech settings.") }
                                },
                                primary = false,
                            )
                        }
                        if (voices.isNotEmpty()) {
                            val current = voices.firstOrNull { it.name == chosen }
                            SelectField(
                                null,
                                current?.label ?: "Phone default",
                                onClick = {
                                    overlays.picker(
                                        "$name voice",
                                        listOf("" to "Phone default") + voices.map { v ->
                                            v.name to (v.label + if (!v.installed) " (not installed)" else "")
                                        },
                                        chosen,
                                        hint = "Voices that work on the phone are listed first: they start faster and keep working on a bad connection.",
                                    ) { picked -> change { if (lang == "el") it.copy(voiceEl = picked) else it.copy(voiceEn = picked) } }
                                },
                            )
                        }
                        TestButton(
                            idle = "Test $name",
                            active = testing == lang,
                            enabled = availability != VoiceAvailability.Unsupported && engineReady,
                        ) {
                            if (testing == lang) {
                                speaker.stop()
                                testing = null
                            } else {
                                testing = lang
                                speaker.say(SAMPLES.getValue(lang), settings, flush = true)
                            }
                        }
                    }
                    if (lang != LANGUAGES.last().first) Divider()
                }
                Column {
                    Label("Speed")
                    Chips(listOf("slow" to "Slower", "normal" to "Normal", "fast" to "Faster"), settings.speed) { v -> change { it.copy(speed = v) } }
                }
                Divider()
                Hint("A reply is read sentence by sentence, each in the right language. This test streams a mixed Greek and English paragraph the way a reply arrives; the part after [details] stays silent.")
                TestButton(idle = "Test a mixed paragraph", active = testing == "mixed", enabled = engineReady) {
                    if (testing == "mixed") {
                        speaker.stop()
                        testing = null
                    } else {
                        testing = "mixed"
                        // Fed in small pieces, like message_delta events.
                        val stream = SpeechStream()
                        MIXED_SAMPLE.chunked(6).forEach { speaker.sayAll(stream.push(it), settings) }
                        speaker.sayAll(stream.flush(), settings)
                    }
                }
                // What is being said right now, so the test is not just a sound from nowhere.
                if (testing != null && saying.isNotEmpty()) {
                    Txt("“$saying”", 14.sp, C.muted, lineHeight = 20.sp)
                }
            }
        }

        item {
            Card {
                CardTitle("Calls")
                SwitchRow("Interrupt by talking", "Start talking and Iris stops to listen.", settings.interrupt, onChange = { v -> change { it.copy(interrupt = v) } })
                SwitchRow("Say what I'm doing", "Iris says a short line while she searches or works.", settings.sayProgress, onChange = { v -> change { it.copy(sayProgress = v) } })
                SwitchRow("Start on loudspeaker", "Otherwise calls start on the earpiece.", settings.loudspeaker, onChange = { v -> change { it.copy(loudspeaker = v) } })
                Column {
                    Label("Wait before answering")
                    Chips(
                        listOf("short" to "Short · 0.5 s", "normal" to "Normal · 0.8 s", "long" to "Long · 1.2 s"),
                        settings.wait,
                    ) { v -> change { it.copy(wait = v) } }
                    Box(Modifier.padding(top = 8.dp)) { Hint("How long you stay quiet before your turn counts as finished.") }
                }
            }
        }

        item {
            Card {
                CardTitle("Try it", "Hold the button and speak, up to 15 seconds. It tests the whole way: phone, relay, server and Groq.")
                val ready = status?.available == true
                val bg by animateColorAsState(if (held) C.danger else if (ready) C.accent else C.lineStrong, label = "mic")
                Box(
                    Modifier.fillMaxWidth().heightIn(min = 64.dp)
                        .background(bg, RoundedCornerShape(20.dp))
                        .pointerInput(ready, sending) {
                            if (!ready || sending) return@pointerInput
                            detectTapGestures(onPress = {
                                startTry()
                                tryAwaitRelease()
                                held = false
                            })
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                        if (sending) Spinner(20.dp, C.onAccent) else Icon(Lucide.Mic, C.onAccent, 22.dp)
                        Txt(
                            when {
                                sending -> "Sending…"
                                held -> "Listening… release to send"
                                !ready -> "Speech to text isn't ready"
                                else -> "Hold to talk"
                            },
                            16.sp, C.onAccent, FontWeight.Medium,
                        )
                    }
                }
                heard?.let {
                    Column(
                        Modifier.fillMaxWidth().background(C.bg.copy(alpha = 0.4f), RoundedCornerShape(14.dp))
                            .border(1.dp, C.lineStrong, RoundedCornerShape(14.dp)).padding(14.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Txt("I heard", 12.sp, C.faint)
                        androidx.compose.foundation.text.selection.SelectionContainer { Txt(it, 16.sp, C.text, lineHeight = 22.sp) }
                    }
                }
                tryNote?.let { Hint(it, if (tryFailed) C.danger else C.faint) }
            }
        }
    }
}
