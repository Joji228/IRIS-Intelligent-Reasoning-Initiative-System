package dev.iris.app.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.Agent
import dev.iris.app.data.AudioRoute
import dev.iris.app.data.CallPhase
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.PendingApproval
import dev.iris.app.data.Screen
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * The call screen. The whole screen is a slow, living glow in the agent's own color: calm while
 * it listens, brighter and greener while you talk, pulsing in step with its voice while it
 * speaks. In the middle sits its orb; under it, what it last said in big type and what you last
 * said in small; at the bottom Mute, Speaker, Stop and a big red Hang up. Approvals appear here
 * and need a tap; they are never given by voice. The call is the agent's ordinary chat, so the
 * transcript is simply the chat.
 */
@Composable
fun CallScreen(vm: IrisViewModel, agent: Agent) {
    val call = vm.call
    if (call == null || call.agent.id != agent.id) {
        // A leftover screen (the call ended): step back.
        LaunchedEffect(Unit) { vm.back() }
        return
    }
    val phase by call.phase.collectAsState()
    val muted by call.muted.collectAsState()
    val route by call.route.collectAsState()
    val you by call.lastYouSaid.collectAsState()
    val iris by call.lastIrisSaid.collectAsState()
    val trouble by call.trouble.collectAsState()
    val speaking by call.speaker.speaking.collectAsState()
    val working = vm.isWorking(agent.id)
    val waiting = vm.isWaiting(agent.id)
    val haptics = LocalHapticFeedback.current
    val tint = remember(agent.color) { parseColor(agent.color) }

    // The screen stays on while this is open; near the ear the proximity sensor switches it off.
    val view = LocalView.current
    DisposableEffect(Unit) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }

    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            now = System.currentTimeMillis()
            delay(1000)
        }
    }

    val approvals = vm.pendingApprovals.filter { it.agentId == agent.id }
    // A question for you at the end of the agent's last reply (it was also spoken).
    val question = vm.messages.lastOrNull()?.takeIf { it.role == "assistant" && vm.thread?.agentId == agent.id }
        ?.rounds?.lastOrNull()?.tools?.lastOrNull { it.name == "ask_user" && it.status == "done" }

    // The colored glow is drawn by [CallBackdropLayer] behind the whole window, so it runs under the
    // status bar and the navigation bar instead of being cut off at them.
    CallContent(
        name = agent.name,
        tint = tint,
        phase = phase,
        muted = muted,
        route = route,
        you = you,
        iris = iris,
        trouble = trouble,
        working = working,
        waiting = waiting,
        thinkingText = if (working) vm.detailOf(agent.id).ifEmpty { vm.statusLine(agent.id) } else "Thinking…",
        elapsedSeconds = (now - call.startedAt) / 1000,
        canStop = working || speaking,
        orb = { size -> Orb(agent, size, working = working && phase != CallPhase.Speaking, waiting = waiting, activity = vm.activityOf(agent.id)) },
        hasExtra = approvals.isNotEmpty() || (question != null && !working),
        extra = {
            for (a in approvals) CallApproval(vm, a)
            if (question != null && !working) QuestionCard(vm, agent, question, answer = null, live = true)
        },
        onBack = { vm.back() },
        onMute = { haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove); call.toggleMute() },
        onSpeaker = { haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove); call.toggleLoudspeaker() },
        onStop = { haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove); call.stopReply() },
        onHangUp = { haptics.performHapticFeedback(HapticFeedbackType.LongPress); vm.endCall() },
    )
}

/**
 * The call screen's layout, with no app state in it (so the debug preview can show it with made-up
 * data). The orb gets whatever height is left after the header, the status line, the captions and
 * the buttons, and is drawn to fit it: it can never run into the header or be clipped, however long
 * the captions are or whether an approval is showing. The captions and approvals scroll in their
 * own, limited area instead.
 *
 * The colored glow behind is [CallBackdropLayer] (or [CallBackdrop] in the preview).
 */
@Composable
fun CallContent(
    name: String,
    tint: Color,
    phase: CallPhase,
    muted: Boolean,
    route: AudioRoute,
    you: String,
    iris: String,
    trouble: String?,
    working: Boolean,
    waiting: Boolean,
    thinkingText: String,
    elapsedSeconds: Long,
    canStop: Boolean,
    orb: @Composable (androidx.compose.ui.unit.Dp) -> Unit,
    /** Whether [extra] has anything to show (it is only given room then). */
    hasExtra: Boolean,
    extra: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit,
    onBack: () -> Unit,
    onMute: () -> Unit,
    onSpeaker: () -> Unit,
    onStop: () -> Unit,
    onHangUp: () -> Unit,
) {
    Column(Modifier.fillMaxSize().padding(horizontal = 20.dp)) {
        // Top bar: back to the chat (the call goes on), name and timer.
        Row(Modifier.fillMaxWidth().height(64.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(Lucide.ChevronLeft, onBack, C.muted, 26.dp)
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                Txt(name, 18.sp, C.text, FontWeight.SemiBold, maxLines = 1)
                Txt(timer(elapsedSeconds), 13.sp, C.muted, mono = true)
            }
            // Keeps the title centered.
            Box(Modifier.size(48.dp))
        }

        // The orb takes the room that is left and is sized to it.
        androidx.compose.foundation.layout.BoxWithConstraints(
            Modifier.weight(1f).fillMaxWidth().padding(vertical = 6.dp),
            contentAlignment = Alignment.Center,
        ) {
            val size = minOf(maxWidth, maxHeight, 300.dp)
            CallOrb(size, tint, phase, muted, orb)
        }

        Box(Modifier.fillMaxWidth().padding(bottom = 12.dp), contentAlignment = Alignment.Center) {
            StatusChip(
                phase = phase,
                muted = muted,
                waiting = waiting,
                trouble = trouble,
                thinkingText = thinkingText,
                name = name,
            )
        }

        // Anything that needs you (an approval, a question) sits in plain view, above the captions.
        if (hasExtra) {
            Column(
                Modifier.fillMaxWidth().heightIn(max = 300.dp).verticalScroll(rememberScrollState()).padding(bottom = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) { extra() }
        }

        // What was said: scrolls when it is long, and never pushes the orb away.
        Column(
            Modifier.fillMaxWidth().heightIn(max = if (hasExtra) 130.dp else 250.dp).verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Captions(name, iris, you)
        }

        Controls(
            muted = muted,
            route = route,
            canStop = canStop,
            onMute = onMute,
            onSpeaker = onSpeaker,
            onStop = onStop,
            onHangUp = onHangUp,
        )
        Box(Modifier.height(20.dp))
    }
}

/**
 * The call's glow, as a layer behind the whole app window (edge to edge, under the system bars).
 * It fades in when the call screen is on top and out when it leaves. The call screen itself is
 * see-through so the glow shows.
 */
@Composable
fun CallBackdropLayer(vm: IrisViewModel) {
    val call = vm.call
    // The last call stays around while the glow fades out after hanging up.
    val last = remember { arrayOfNulls<dev.iris.app.data.CallController>(1) }
    if (call != null) last[0] = call
    androidx.compose.animation.AnimatedVisibility(
        visible = call != null && vm.screen is Screen.Call,
        enter = fadeIn(tween(450)),
        exit = fadeOut(tween(300)),
    ) {
        val c = last[0] ?: return@AnimatedVisibility
        val phase by c.phase.collectAsState()
        val muted by c.muted.collectAsState()
        val tint = remember(c.agent.color) { parseColor(c.agent.color) }
        CallBackdrop(tint, phase, muted, waiting = vm.isWaiting(c.agent.id))
    }
}

/**
 * Soft blobs of color that drift behind everything: the agent's color high up, violet and blue
 * lower down, and a green bloom while you speak. How bright they are and how fast they beat
 * follows the call's phase. Drawn in the draw phase, so animating it never recomposes.
 */
@Composable
internal fun CallBackdrop(tint: Color, phase: CallPhase, muted: Boolean, waiting: Boolean) {
    val clock = rememberInfiniteTransition("backdrop")
    val drift by clock.animateFloat(0f, (2 * PI).toFloat(), infiniteRepeatable(tween(22_000, easing = LinearEasing)), "drift")
    val beatMs = when (phase) {
        CallPhase.Speaking -> 820
        CallPhase.YouSpeaking -> 620
        CallPhase.Thinking -> 1500
        CallPhase.Listening -> 2800
    }
    val beat by clock.animateFloat(0f, 1f, infiniteRepeatable(tween(beatMs, easing = FastOutSlowInEasing), RepeatMode.Reverse), "beat")
    val energy by animateFloatAsState(
        when {
            muted -> 0.35f
            phase == CallPhase.Speaking -> 1f
            phase == CallPhase.YouSpeaking -> 0.8f
            phase == CallPhase.Thinking -> 0.65f
            else -> 0.55f
        },
        tween(800), label = "energy",
    )
    val you by animateFloatAsState(if (phase == CallPhase.YouSpeaking && !muted) 1f else 0f, tween(500), label = "you")
    val warn by animateFloatAsState(if (waiting) 1f else 0f, tween(500), label = "warn")

    Canvas(Modifier.fillMaxSize().background(C.bg)) {
        fun blob(cx: Float, cy: Float, radius: Float, color: Color, alpha: Float) {
            if (alpha <= 0.01f) return
            drawCircle(
                brush = Brush.radialGradient(listOf(color.copy(alpha = alpha.coerceAtMost(1f)), Color.Transparent), center = Offset(cx, cy), radius = radius),
                radius = radius, center = Offset(cx, cy),
            )
        }
        val w = size.width
        val h = size.height
        val d = drift
        // The agent's color, behind the orb.
        blob(w * (0.5f + 0.16f * cos(d)), h * (0.34f + 0.05f * sin(d * 1.3f)), w * (0.95f + 0.14f * beat * energy), tint, 0.34f * energy)
        // Violet, low left; blue, low right: the app's own palette.
        blob(w * (0.15f + 0.14f * sin(d * 0.8f)), h * (0.78f + 0.04f * cos(d)), w * 0.9f, C.accent, 0.22f * energy)
        blob(w * (0.9f + 0.08f * cos(d * 1.1f)), h * (0.66f + 0.05f * sin(d * 0.9f)), w * 0.8f, Color(0xFF60A5FA), 0.15f * energy)
        // Green while you speak, amber while it waits for your OK.
        blob(w * 0.5f, h * 0.36f, w * (0.85f + 0.12f * beat), C.success, 0.26f * you)
        blob(w * 0.5f, h * 0.36f, w * 0.9f, C.warn, 0.18f * warn)
        // A darker floor under the buttons keeps them readable.
        drawRect(Brush.verticalGradient(listOf(Color.Transparent, C.bg.copy(alpha = 0.85f)), startY = h * 0.62f, endY = h))
    }
}

/**
 * The orb with its soft pulsing rings, drawn to fit a square of [size]: the outer ring at its biggest
 * (22% more than at rest) still stays inside it, so nothing is cut off at the edges of its space.
 */
@Composable
private fun CallOrb(size: androidx.compose.ui.unit.Dp, tint: Color, phase: CallPhase, muted: Boolean, orb: @Composable (androidx.compose.ui.unit.Dp) -> Unit) {
    // Three looks, each with its own steady pulse: calm (slow breathing), speaking (quicker) and you
    // talking (green, quickest). They all run all the time and only cross-fade, so a change of
    // phase never makes a ring jump mid-pulse.
    val pulse = rememberInfiniteTransition("call-pulse")
    val tCalm by pulse.animateFloat(0f, 1f, infiniteRepeatable(tween(2600, easing = FastOutSlowInEasing), RepeatMode.Reverse), "calm")
    val tSpeak by pulse.animateFloat(0f, 1f, infiniteRepeatable(tween(850, easing = FastOutSlowInEasing), RepeatMode.Reverse), "speak")
    val tYou by pulse.animateFloat(0f, 1f, infiniteRepeatable(tween(650, easing = FastOutSlowInEasing), RepeatMode.Reverse), "you")
    val fade = tween<Float>(600)
    val calm by animateFloatAsState(
        when {
            phase == CallPhase.Speaking || phase == CallPhase.YouSpeaking -> if (muted) 0.45f else 0f
            muted -> 0.45f
            else -> 1f
        },
        fade, label = "calm-a",
    )
    val speak by animateFloatAsState(if (phase == CallPhase.Speaking && !muted) 1f else 0f, fade, label = "speak-a")
    val you by animateFloatAsState(if (phase == CallPhase.YouSpeaking && !muted) 1f else 0f, fade, label = "you-a")

    // Outer ring 0.80 of the space (x1.22 at its biggest = 0.98), inner ring 0.68, the orb 0.57.
    @Composable
    fun Rings(color: Color, t: Float, amount: Float, a: Float) {
        if (a < 0.01f) return
        Box(Modifier.size(size * 0.80f).scale(1f + amount * 2.2f * t).alpha((0.07f + 0.10f * t) * a).background(color, CircleShape))
        Box(Modifier.size(size * 0.68f).scale(1f + amount * t).alpha(0.16f * a).background(color, CircleShape))
    }
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        Rings(tint, tCalm, 0.04f, calm)
        Rings(tint, tSpeak, 0.08f, speak)
        Rings(C.success, tYou, 0.10f, you)
        orb(size * 0.57f)
    }
}

/** A glass pill saying what the call is doing, with a little live indicator. */
@Composable
private fun StatusChip(phase: CallPhase, muted: Boolean, waiting: Boolean, trouble: String?, thinkingText: String, name: String) {
    val target = when {
        trouble != null -> C.danger
        waiting -> C.warn
        muted -> C.faint
        phase == CallPhase.YouSpeaking -> C.success
        phase == CallPhase.Speaking -> C.accentHi
        else -> C.muted
    }
    // The chip's color glides to the new phase instead of switching.
    val color by androidx.compose.animation.animateColorAsState(target, tween(400), label = "chip")
    Row(
        Modifier.background(Color.White.copy(alpha = 0.07f), RoundedCornerShape(50))
            .border(1.dp, if (target == C.muted || target == C.faint) Color.White.copy(alpha = 0.10f) else color.copy(alpha = 0.32f), RoundedCornerShape(50))
            .padding(horizontal = 16.dp, vertical = 9.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when {
            trouble != null -> Icon(Lucide.CircleAlert, color, 15.dp)
            waiting -> Icon(Lucide.ShieldAlert, color, 15.dp)
            muted -> Icon(Lucide.MicOff, color, 15.dp)
            else -> Bars(color, fast = phase == CallPhase.YouSpeaking || phase == CallPhase.Speaking, still = phase == CallPhase.Thinking)
        }
        AnimatedContent(
            targetState = when {
                trouble != null -> trouble
                waiting -> "Waiting for your OK"
                muted -> "Muted"
                phase == CallPhase.YouSpeaking -> "I'm listening"
                phase == CallPhase.Thinking -> thinkingText
                phase == CallPhase.Speaking -> "$name is speaking"
                else -> "Listening"
            }.orEmpty(),
            transitionSpec = { fadeIn(tween(220)) togetherWith fadeOut(tween(140)) },
            label = "status",
        ) { text ->
            if (phase == CallPhase.Thinking && !muted && trouble == null && !waiting) Shimmer(text, 14.5.sp)
            else Txt(text, 14.5.sp, color, FontWeight.Medium, maxLines = 2)
        }
    }
}

/** Three little bars that dance, like a sound level. */
@Composable
internal fun Bars(color: Color, fast: Boolean, still: Boolean) {
    val t = rememberInfiniteTransition("bars")
    val a by t.animateFloat(0.25f, 1f, infiniteRepeatable(tween(if (fast) 380 else 760, easing = FastOutSlowInEasing), RepeatMode.Reverse), "a")
    val b by t.animateFloat(1f, 0.3f, infiniteRepeatable(tween(if (fast) 470 else 900, easing = FastOutSlowInEasing), RepeatMode.Reverse), "b")
    val c by t.animateFloat(0.4f, 0.9f, infiniteRepeatable(tween(if (fast) 560 else 1000, easing = FastOutSlowInEasing), RepeatMode.Reverse), "c")
    Row(Modifier.height(16.dp), horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
        for (level in listOf(a, b, c)) {
            Box(Modifier.width(3.dp).height((16f * (if (still) 0.35f else level)).dp).background(color, CircleShape))
        }
    }
}

/**
 * What you last said, small, and what it said back, big: in the order they happened. They fade into
 * each other as they change.
 */
@Composable
private fun Captions(name: String, iris: String, you: String) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 6.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        AnimatedContent(
            targetState = you,
            transitionSpec = { fadeIn(tween(260)) togetherWith fadeOut(tween(160)) },
            label = "you-caption",
        ) { text ->
            if (text.isNotEmpty()) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Txt("You said", 11.5.sp, C.faint, align = TextAlign.Center)
                    Txt("“$text”", 15.sp, C.muted, maxLines = 3, lineHeight = 21.sp, align = TextAlign.Center)
                }
            } else Box(Modifier.height(2.dp))
        }
        AnimatedContent(
            targetState = iris,
            transitionSpec = { fadeIn(tween(260)) togetherWith fadeOut(tween(160)) },
            label = "iris-caption",
        ) { text ->
            // No line limit: a long reply scrolls in its own area instead of being cut off with "…".
            if (text.isNotEmpty()) Txt(text, 20.sp, C.text, FontWeight.Medium, lineHeight = 28.sp, align = TextAlign.Center)
            else Box(Modifier.height(2.dp))
        }
    }
}

/** An approval from the call's agent: shown here and decided by a tap (never by voice). */
@Composable
private fun CallApproval(vm: IrisViewModel, a: PendingApproval) =
    CallApprovalCard(a.summary, { vm.decide(a.id, true) }, { vm.decide(a.id, false) })

@Composable
internal fun CallApprovalCard(summary: String, onApprove: () -> Unit, onDeny: () -> Unit) {
    Column(
        Modifier.fillMaxWidth()
            .background(C.surface.copy(alpha = 0.85f), RoundedCornerShape(24.dp))
            .border(1.dp, C.warn.copy(alpha = 0.45f), RoundedCornerShape(24.dp))
            .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Lucide.ShieldAlert, C.warn, 17.dp)
            Txt("Needs your OK", 14.5.sp, C.warn, FontWeight.SemiBold)
        }
        Txt(summary, 14.sp, C.text, mono = true, maxLines = 6)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button("Approve", onApprove, icon = Lucide.Check)
            Button("Deny", onDeny, icon = Lucide.X, primary = false)
        }
        Txt("Approvals are tapped, never spoken.", 12.sp, C.faint)
    }
}

@Composable
private fun Controls(
    muted: Boolean,
    route: AudioRoute,
    canStop: Boolean,
    onMute: () -> Unit,
    onSpeaker: () -> Unit,
    onStop: () -> Unit,
    onHangUp: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().padding(top = 10.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.Top) {
        RoundButton(if (muted) Lucide.MicOff else Lucide.Mic, if (muted) "Unmute" else "Mute", on = muted, onClick = onMute)
        RoundButton(
            Lucide.Volume,
            when (route) { AudioRoute.Speaker -> "Speaker"; AudioRoute.Bluetooth -> "Headset"; AudioRoute.Earpiece -> "Earpiece" },
            on = route == AudioRoute.Speaker,
            onClick = onSpeaker,
        )
        RoundButton(Lucide.Square, "Stop", enabled = canStop, onClick = onStop)
        RoundButton(Lucide.Phone, "Hang up", danger = true, iconRotation = 135f, onClick = onHangUp)
    }
}

/** A round glass button with a label. [on] fills it with the accent; [danger] is the red hang-up. */
@Composable
private fun RoundButton(
    icon: ImageVector,
    label: String,
    on: Boolean = false,
    enabled: Boolean = true,
    danger: Boolean = false,
    iconRotation: Float = 0f,
    onClick: () -> Unit,
) {
    // A springy press (it squeezes and bounces back), the accent fill fading in and out when it's
    // switched on, the icon cross-fading when it changes, and a ring that bursts out on each tap.
    val source = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val squeeze by animateFloatAsState(
        if (pressed && enabled) 0.88f else 1f,
        androidx.compose.animation.core.spring(dampingRatio = 0.42f, stiffness = 520f), label = "squeeze",
    )
    val fill by animateFloatAsState(if (on) 1f else 0f, tween(300), label = "fill")
    val dim by animateFloatAsState(if (enabled) 1f else 0.38f, tween(250), label = "dim")
    val burst = remember { androidx.compose.animation.core.Animatable(1f) }
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val tint by androidx.compose.animation.animateColorAsState(if (danger || on) Color.White else C.text, tween(300), label = "tint")
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.size(70.dp), contentAlignment = Alignment.Center) {
            // The burst ring.
            Box(
                Modifier.size(70.dp).scale(1f + 0.45f * burst.value).alpha(0.55f * (1f - burst.value))
                    .border(2.dp, if (danger) C.danger else if (on) C.accentHi else C.text, CircleShape),
            )
            Box(
                Modifier.size(70.dp).scale(squeeze).alpha(dim)
                    .background(
                        if (danger) Brush.linearGradient(listOf(Color(0xFFFB7185), Color(0xFFEF4444)))
                        else Brush.linearGradient(listOf(Color.White.copy(alpha = 0.12f), Color.White.copy(alpha = 0.06f))),
                        CircleShape,
                    )
                    .background(Brush.linearGradient(listOf(C.accentHi.copy(alpha = fill), C.accent.copy(alpha = fill))), CircleShape)
                    .border(1.dp, if (danger) Color.Transparent else Color.White.copy(alpha = 0.12f * (1f - fill)), CircleShape)
                    .clickable(interactionSource = source, indication = null, enabled = enabled, role = androidx.compose.ui.semantics.Role.Button) {
                        scope.launch {
                            burst.snapTo(0f)
                            burst.animateTo(1f, tween(450))
                        }
                        onClick()
                    },
                contentAlignment = Alignment.Center,
            ) {
                AnimatedContent(
                    targetState = icon,
                    transitionSpec = {
                        (fadeIn(tween(200)) + androidx.compose.animation.scaleIn(tween(260), initialScale = 0.6f)) togetherWith fadeOut(tween(120))
                    },
                    label = "icon",
                ) { i -> Icon(i, tint, 28.dp, Modifier.rotate(iconRotation)) }
            }
        }
        AnimatedContent(targetState = label, transitionSpec = { fadeIn(tween(200)) togetherWith fadeOut(tween(120)) }, label = "label") { l ->
            Txt(l, 12.5.sp, if (danger) C.danger else C.muted, FontWeight.Medium)
        }
    }
}

/**
 * The phone button in the chat header. With a call going it returns to the call screen. Otherwise
 * it starts one: if the server can't turn speech into text yet it opens Settings → Voice instead,
 * and the first time it asks for the microphone.
 */
@Composable
fun CallButton(vm: IrisViewModel, agent: Agent) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val onThisCall = vm.call?.agent?.id == agent.id
    val permission = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) vm.startCall(agent) else vm.showToast("Calls need the microphone. Allow it for I.R.I.S. in Android's settings.")
    }
    // A soft violet disc, so the call button is easy to find; green with a beating dot on a call.
    val beat by rememberInfiniteTransition("call-dot").animateFloat(0.35f, 1f, infiniteRepeatable(tween(900), RepeatMode.Reverse), "dot")
    Box(
        Modifier.padding(horizontal = 2.dp).size(40.dp)
            .background(if (onThisCall) C.success.copy(alpha = 0.16f) else C.accentSoft, CircleShape)
            .border(1.dp, if (onThisCall) C.success.copy(alpha = 0.5f) else C.accent.copy(alpha = 0.35f), CircleShape)
            .press {
                when {
                    onThisCall -> vm.open(Screen.Call(agent.id))
                    vm.call != null -> vm.showToast("You're already on a call with ${vm.call?.agent?.name}")
                    else -> scope.launch {
                        val status = vm.quiet { voice() }.getOrNull()
                        when {
                            status?.available != true -> {
                                vm.showToast("Add a Groq key on the server to make calls")
                                vm.open(Screen.Settings("voice"))
                            }
                            dev.iris.app.data.Recorder.hasPermission(context) -> vm.startCall(agent)
                            else -> permission.launch(android.Manifest.permission.RECORD_AUDIO)
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Lucide.Phone, if (onThisCall) C.success else C.accentHi, 19.dp)
        if (onThisCall) {
            Box(Modifier.align(Alignment.TopEnd).padding(7.dp).size(8.dp).alpha(beat).background(C.success, CircleShape))
        }
    }
}

/**
 * A green bar at the top of every other screen while a call is going: who, for how long, and a
 * tap takes you back to it. Without it, leaving the call screen would hide that you are still live.
 */
@Composable
fun OnCallBar(vm: IrisViewModel) {
    val call = vm.call ?: return
    if (vm.screen is Screen.Call) return
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            now = System.currentTimeMillis()
            delay(1000)
        }
    }
    val beat by rememberInfiniteTransition("bar-dot").animateFloat(0.3f, 1f, infiniteRepeatable(tween(900), RepeatMode.Reverse), "dot")
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)
            .background(C.success.copy(alpha = 0.14f), RoundedCornerShape(50))
            .border(1.dp, C.success.copy(alpha = 0.4f), RoundedCornerShape(50))
            .press { vm.open(Screen.Call(call.agent.id)) }
            .padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(9.dp).alpha(beat).background(C.success, CircleShape))
        Txt("On a call with ${call.agent.name}", 14.sp, C.text, FontWeight.Medium, Modifier.weight(1f), maxLines = 1)
        Txt(timer((now - call.startedAt) / 1000), 13.sp, C.success, mono = true)
        Box(
            Modifier.size(34.dp).background(C.danger, CircleShape).press { vm.endCall() },
            contentAlignment = Alignment.Center,
        ) { Icon(Lucide.Phone, Color.White, 16.dp, Modifier.rotate(135f)) }
    }
}

private fun timer(seconds: Long): String {
    val s = seconds.coerceAtLeast(0)
    return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
}
