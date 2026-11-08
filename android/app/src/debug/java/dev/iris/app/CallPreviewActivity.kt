package dev.iris.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.ui.Modifier
import dev.iris.app.data.AudioRoute
import dev.iris.app.data.CallPhase
import dev.iris.app.ui.C
import dev.iris.app.ui.CallApprovalCard
import dev.iris.app.ui.CallBackdrop
import dev.iris.app.ui.CallContent
import dev.iris.app.ui.IrisTheme
import dev.iris.app.ui.Orb
import dev.iris.app.ui.ProvideOrbClock
import dev.iris.app.ui.parseColor

/**
 * Debug builds only: shows the call screen with made-up data, so its layout can be checked on an
 * emulator without a paired server or a real call. Not part of the release app.
 *
 *   adb shell am start -n dev.iris.app/.CallPreviewActivity --es phase speaking --ez long true --ez approval true
 *
 * Extras: phase (listening | you | thinking | speaking), muted, long (a long reply caption), approval, trouble.
 */
class CallPreviewActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        val phase = when (intent.getStringExtra("phase")) {
            "you" -> CallPhase.YouSpeaking
            "thinking" -> CallPhase.Thinking
            "speaking" -> CallPhase.Speaking
            else -> CallPhase.Listening
        }
        val muted = intent.getBooleanExtra("muted", false)
        val long = intent.getBooleanExtra("long", false)
        val approval = intent.getBooleanExtra("approval", false)
        val trouble = intent.getStringExtra("trouble")
        val agent = dev.iris.app.data.Agent(id = "iris", name = "Iris", color = "#c4b5fd", role = "main")
        val tint = parseColor(agent.color)
        val said = if (long) {
            "Άκουσα καθαρά «Thank you» στο προηγούμενο μήνυμά σου — αν δεν το είπες, ήταν λάθος της αναγνώρισης φωνής, δεν υπέθεσα τίποτα. I can also check it again if you like, or look at the logs for that call."
        } else "The weather in London is cloudy, around 14 degrees."
        setContent {
            IrisTheme {
                ProvideOrbClock {
                    Box(Modifier.fillMaxSize().background(C.bg)) {
                        // The glow reaches the screen's edges, like in the app.
                        CallBackdrop(tint, phase, muted, waiting = approval)
                        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars)) {
                            CallContent(
                                name = agent.name, tint = tint, phase = phase, muted = muted, route = AudioRoute.Earpiece,
                                you = "What's that bug?", iris = said, trouble = trouble,
                                working = phase == CallPhase.Thinking, waiting = approval,
                                thinkingText = "Searching the web", elapsedSeconds = 346, canStop = phase != CallPhase.Listening,
                                orb = { size -> Orb(agent, size, working = phase == CallPhase.Thinking, waiting = approval) },
                                hasExtra = approval,
                                extra = {
                                    if (approval) CallApprovalCard("run_command: Get-ChildItem logs | Sort-Object LastWriteTime", {}, {})
                                },
                                onBack = {}, onMute = {}, onSpeaker = {}, onStop = {}, onHangUp = { finish() },
                            )
                        }
                    }
                }
            }
        }
    }
}
