package dev.iris.app.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableFloatState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.iris.app.data.Agent
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.OrbState
import dev.iris.app.data.orbState
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.random.Random

/**
 * An agent's face, drawn like AgentAvatar.svelte: a flat, single-color orb with eyes and an
 * orbiting ring. It floats, blinks and twinkles, and its face changes with what it's up to:
 * each state (thinking, searching, reading, working, waiting, done, failed, asleep…) has a few
 * faces it morphs between, a body motion, and a little effect (thought dots, sparkles when it
 * finishes, a sweat drop when it fails, z's when it dozes). Every motion is computed from one
 * shared clock in the draw phase, so animating never recomposes.
 */
@Composable
fun Orb(
    agent: Agent?,
    size: Dp,
    modifier: Modifier = Modifier,
    working: Boolean = false,
    waiting: Boolean = false,
    mood: String? = null,
    activity: String? = null,
    unread: Boolean = false,
    sleepy: Boolean = false,
    /** A small chip on the orb's corner with an icon for the step it's on. */
    chip: Boolean = false,
) {
    val state = orbState(activity, mood, working, waiting, unread, sleepy)
    val color = agent?.color.orEmpty()
    val light = remember(color) { parseColor(color) }
    val mid = remember(color) { mix(light, 0.62f, INK) }
    val dark = remember(color) { mix(light, 0.30f, INK) }
    // Different agents float and blink out of step with each other.
    val key = agent?.id ?: agent?.name ?: "iris"
    val seed = remember(key) { key.fold(7L) { h, c -> (h * 31 + c.code) and 0xFFFFFFFFL } }
    val showRing = size >= 26.dp
    val effects = size >= 26.dp

    val clocks = LocalOrbClock.current
    DisposableEffect(clocks) {
        clocks.users.intValue++
        onDispose { clocks.users.intValue-- }
    }
    // Busy orbs move at the full frame rate; idle and dozing ones at half of it.
    val clock = if (state == OrbState.Idle || state == OrbState.Sleeping) clocks.slow else clocks.fast
    // When the state began, for one-off motions (a hop, a sink, a burst of sparkles).
    val since = remember(state) { Snapshot.withoutReadObservation { clocks.fast.floatValue } }

    // Waking up: a stretch and wide eyes when a dozing orb gets going again.
    var waking by remember { mutableStateOf(false) }
    var wokeAt by remember { mutableFloatStateOf(0f) }
    val prev = remember { arrayOf(state) }
    LaunchedEffect(state) {
        val was = prev[0]
        prev[0] = state
        if (was == OrbState.Sleeping && state != OrbState.Sleeping) {
            wokeAt = Snapshot.withoutReadObservation { clocks.fast.floatValue }
            waking = true
            try { delay(1300) } finally { waking = false }
        }
    }

    // The face: cycles through the state's pool, morphing from one to the next.
    val morph = remember { FaceMorph() }
    LaunchedEffect(state, waking) {
        val pool = POOLS.getValue(state)
        morph.go(if (waking) FACES.getValue("wide") else FACES.getValue(pool[0]))
        if (pool.size < 2 || waking || clocks.still) return@LaunchedEffect
        var i = 0
        while (true) {
            delay(((HOLD[state] ?: 2200) * (0.75f + Random.nextFloat() * 0.6f)).toLong())
            // Reading sweeps left-right in order; everything else picks freely.
            i = if (state == OrbState.Reading || state == OrbState.Celebrate) (i + 1) % pool.size
            else Random.nextInt(pool.size).let { if (it == i) (it + 1) % pool.size else it }
            morph.go(FACES.getValue(pool[i]))
        }
    }

    Box(modifier.size(size)) {
        Canvas(Modifier.matchParentSize()) {
            val t = clock.floatValue
            val u = t - since
            val s = this.size.minDimension / 64f
            val ringR = this.size.minDimension / 2 + 2.dp.toPx()

            if (state == OrbState.Waiting) {
                val a = 0.2f + 0.8f * (0.5f + 0.5f * cos(2 * PI.toFloat() * t / 1.4f))
                drawCircle(C.warn.copy(alpha = a), radius = ringR, style = Stroke(2.dp.toPx()))
            } else if (state.busy) {
                rotate(360f * ((t / 1.6f) % 1f)) {
                    drawCircle(RAINBOW_SWEEP, radius = ringR, style = Stroke(2.dp.toPx()))
                }
            }

            scale(s, s, pivot = Offset.Zero) {
                val f = morph.current()
                val body = bodyMotion(state, t, u, if (waking) t - wokeAt else -1f)
                withTransform({
                    translate(body[0], body[1])
                    rotate(body[4], pivot = Offset(32f, 32f))
                    scale(body[2], body[3], pivot = Offset(32f, 34f))
                }) {
                    val sleeping = state == OrbState.Sleeping
                    val floatDy: Float
                    val breathe: Float
                    if (sleeping) {
                        val k = 0.5f - 0.5f * cos(2 * PI.toFloat() * t / 4.6f)
                        floatDy = 0.6f - 0.6f * k
                        breathe = 0.025f * k
                    } else {
                        floatDy = -1.6f * (0.5f - 0.5f * cos(2 * PI.toFloat() * (t + seed % 4000 / 1000f) / 3.8f))
                        breathe = 0f
                    }
                    withTransform({
                        translate(0f, floatDy)
                        scale(1f + breathe, 1f - breathe * 0.6f, pivot = Offset(32f, 34f))
                    }) {
                        val ringAlpha = if (sleeping) 0.25f else 1f
                        if (showRing) ringHalf(light, back = true, ringAlpha)
                        drawCircle(light.copy(alpha = 0.55f), radius = 21.5f, center = Offset(32f, 31f))
                        rotate(-28f, pivot = Offset(34f, 34f)) {
                            drawOval(mid, topLeft = Offset(14f, 19f), size = Size(40f, 30f))
                        }
                        face(state, t, u, seed, dark, f)
                        if (showRing) {
                            ringHalf(light, back = false, ringAlpha)
                            sparkle(state, t, u, seed)
                        }
                    }
                }
                if (effects) effect(state, t, u, light)
            }
        }
        if (chip && state.busy && size >= 34.dp) OrbChip(state, activity, clocks.fast)
    }
}

/** An agent's orb with everything the app knows about it (run, mood, unread, away). */
@Composable
fun AgentOrb(vm: IrisViewModel, agent: Agent, size: Dp, modifier: Modifier = Modifier, chip: Boolean = false) = Orb(
    agent, size, modifier,
    working = vm.isWorking(agent.id),
    waiting = vm.isWaiting(agent.id),
    mood = vm.moods[agent.id],
    activity = vm.activityOf(agent.id),
    unread = vm.unread[agent.id] == true,
    sleepy = vm.sleepy,
    chip = chip,
)

// ---- faces -------------------------------------------------------------------------------

/*
 * A face as numbers, so two can be blended: each eye's [scaleX, scaleY, rotate°, dx, dy]
 * (0–4 left, 5–9 right), the brows' left and right rotation and lift (10–12) and opacity (13),
 * and how much of each eye style shows: open pills (14), smiling arcs (15), closed lines (16).
 */
private const val N = 17

private fun face(
    eyes: String = "open",
    l: FloatArray = eye(),
    r: FloatArray = eye(),
    brows: FloatArray? = null,
) = FloatArray(N).also {
    l.copyInto(it, 0)
    r.copyInto(it, 5)
    it[10] = brows?.get(0) ?: 0f
    it[11] = brows?.get(1) ?: 0f
    it[12] = brows?.get(2) ?: 1.5f
    it[13] = if (brows != null) 1f else 0f
    it[14] = if (eyes == "open") 1f else 0f
    it[15] = if (eyes == "happy") 1f else 0f
    it[16] = if (eyes == "closed") 1f else 0f
}

private fun eye(sx: Float = 1f, sy: Float = 1f, rot: Float = 0f, dx: Float = 0f, dy: Float = 0f) = floatArrayOf(sx, sy, rot, dx, dy)
private fun brows(l: Float, r: Float, lift: Float) = floatArrayOf(l, r, lift)

private val FACES = mapOf(
    "neutral" to face(),
    "soft" to face(l = eye(1f, 0.86f, 0f, 0f, 0.4f), r = eye(1f, 0.86f, 0f, 0f, 0.4f)),
    "glanceLeft" to face(l = eye(1f, 1f, 0f, -2f, 0f), r = eye(0.94f, 1f, 0f, -2f, 0f)),
    "glanceRight" to face(l = eye(0.94f, 1f, 0f, 2f, 0f), r = eye(1f, 1f, 0f, 2f, 0f)),
    "curious" to face(l = eye(1.08f, 1.1f, 0f, 0.6f, -0.4f), r = eye(0.96f, 0.78f, 0f, 0.6f, 0.4f), brows = brows(-6f, 10f, -1.4f)),
    "wide" to face(l = eye(1.12f, 1.12f), r = eye(1.12f, 1.12f), brows = brows(-8f, 8f, -2.2f)),
    "pondering" to face(l = eye(0.95f, 0.9f, 0f, 1.8f, -2.4f), r = eye(0.95f, 0.9f, 0f, 1.8f, -2.4f), brows = brows(-10f, 0f, -1.6f)),
    "ponderLeft" to face(l = eye(0.95f, 0.9f, 0f, -1.8f, -2.2f), r = eye(0.95f, 0.9f, 0f, -1.8f, -2.2f), brows = brows(0f, 10f, -1.6f)),
    "focused" to face(l = eye(1f, 0.8f, 0f, 0f, 0.4f), r = eye(1f, 0.8f, 0f, 0f, 0.4f), brows = brows(14f, -14f, 0.6f)),
    "determined" to face(l = eye(1.04f, 0.66f, -6f, 0f, 0.8f), r = eye(1.04f, 0.66f, 6f, 0f, 0.8f), brows = brows(20f, -20f, 1.2f)),
    "readingL" to face(l = eye(1f, 0.72f, 0f, -1.6f, 1.2f), r = eye(1f, 0.72f, 0f, -1.6f, 1.2f), brows = brows(6f, -6f, 0.6f)),
    "readingR" to face(l = eye(1f, 0.72f, 0f, 1.6f, 1.2f), r = eye(1f, 0.72f, 0f, 1.6f, 1.2f), brows = brows(6f, -6f, 0.6f)),
    "suspicious" to face(l = eye(1.04f, 0.42f, 0f, 0.8f, 0.6f), r = eye(1.04f, 0.5f, 0f, 0.8f, 0.6f), brows = brows(4f, -12f, 0.8f)),
    "lookUp" to face(l = eye(1f, 1f, 0f, 0.6f, -2.2f), r = eye(1f, 1f, 0f, 0.6f, -2.2f), brows = brows(-6f, 6f, -1.6f)),
    "worried" to face(l = eye(0.92f, 0.9f, 0f, 0f, -0.6f), r = eye(0.92f, 0.9f, 0f, 0f, -0.6f), brows = brows(-16f, 16f, -1.2f)),
    "happy" to face("happy"),
    "proud" to face("happy", eye(1.06f, 1f, -6f, 0f, -0.6f), eye(1.06f, 1f, 6f, 0f, -0.6f), brows(-8f, 8f, -1.6f)),
    "wink" to face(l = eye(1.06f, 1.06f), r = eye(1f, 0.14f, 0f, 0f, 0.8f), brows = brows(-6f, 6f, -1.2f)),
    "sad" to face(l = eye(1f, 0.6f, 14f, 0f, 1.4f), r = eye(1f, 0.6f, -14f, 0f, 1.4f), brows = brows(-18f, 18f, -0.6f)),
    "asleep" to face("closed"),
)

/** Faces each state cycles through (the first is shown first). */
private val POOLS = mapOf(
    OrbState.Idle to listOf("neutral", "soft", "glanceLeft", "neutral", "glanceRight", "curious"),
    OrbState.Sleeping to listOf("asleep"),
    OrbState.Thinking to listOf("pondering", "ponderLeft", "soft", "pondering"),
    OrbState.Searching to listOf("curious", "glanceLeft", "wide", "glanceRight", "focused"),
    OrbState.Reading to listOf("readingL", "readingR"),
    OrbState.Working to listOf("focused", "determined", "focused", "glanceRight"),
    OrbState.Team to listOf("neutral", "happy", "glanceLeft"),
    OrbState.Reviewing to listOf("suspicious", "focused"),
    OrbState.Waiting to listOf("lookUp", "worried", "lookUp", "soft"),
    OrbState.Designing to listOf("curious", "wide", "happy", "pondering"),
    OrbState.Celebrate to listOf("happy", "proud", "wink"),
    OrbState.Failed to listOf("sad", "worried"),
    OrbState.Unread to listOf("wide", "happy", "curious"),
    OrbState.Spawn to listOf("wide", "happy"),
    OrbState.Spin to listOf("happy"),
)

/** How long each face holds in a state (ms, before a little random jitter). */
private val HOLD = mapOf(
    OrbState.Idle to 3600, OrbState.Reading to 900, OrbState.Searching to 1300, OrbState.Working to 1900,
    OrbState.Thinking to 2400, OrbState.Celebrate to 900, OrbState.Waiting to 2600,
)

/** Glides from one face to the next with a little overshoot (the web's .32s spring curve). */
private class FaceMorph {
    private var from = FACES.getValue("neutral")
    private var to = from
    private val k = Animatable(1f)

    fun current(): FloatArray {
        val p = k.value
        return FloatArray(N) { i -> from[i] + (to[i] - from[i]) * p }
    }

    suspend fun go(target: FloatArray) {
        if (target === to) return
        from = Snapshot.withoutReadObservation { current() }
        to = target
        k.snapTo(0f)
        k.animateTo(1f, tween(320, easing = MORPH))
    }
}

private val MORPH = CubicBezierEasing(0.34f, 1.4f, 0.5f, 1f)

// ---- motion ------------------------------------------------------------------------------

/** The whole orb's motion for its state: [dx, dy, scaleX, scaleY, rotate°]. */
private fun bodyMotion(state: OrbState, t: Float, u: Float, wake: Float): FloatArray {
    if (wake >= 0f && wake < 1.2f) {
        val p = wake / 1.2f
        return floatArrayOf(
            0f, keys(p, 0f to 0f, 0.3f to -3f, 0.6f to 0f, 1f to 0f),
            keys(p, 0f to 1f, 0.3f to 0.92f, 0.6f to 1.05f, 1f to 1f),
            keys(p, 0f to 1f, 0.3f to 1.1f, 0.6f to 0.95f, 1f to 1f), 0f,
        )
    }
    return when (state) {
        OrbState.Working -> {
            val k = 0.5f - 0.5f * cos(2 * PI.toFloat() * t / 0.9f)
            floatArrayOf(0f, 0f, 1f + 0.02f * k, 1f - 0.02f * k, 0f)
        }
        OrbState.Waiting -> floatArrayOf(0f, keys((t / 2.4f) % 1f, 0f to 0f, 0.45f to -2.5f, 1f to 0f), 1f, 1f, 0f)
        OrbState.Spin -> if (u < 1.6f) floatArrayOf(0f, keys(u / 1.6f, 0f to 0f, 0.45f to -2.5f, 1f to 0f), 1f, 1f, 0f) else REST
        OrbState.Designing -> hop((t / 1.1f) % 1f)
        OrbState.Celebrate -> if (u < 1.7f) hop((u / 0.85f) % 1f) else REST
        OrbState.Failed -> {
            val k = min(u / 0.6f, 1f).let { 1 - (1 - it) * (1 - it) }
            floatArrayOf(0f, 2f * k, 1f - 0.03f * k, 1f - 0.03f * k, 0f)
        }
        OrbState.Unread -> {
            val p = (t / 3.2f) % 1f
            floatArrayOf(
                0f, keys(p, 0f to 0f, 0.62f to 0f, 0.68f to 1f, 0.76f to -4f, 0.84f to 0f, 1f to 0f),
                keys(p, 0f to 1f, 0.62f to 1f, 0.68f to 1.05f, 0.76f to 1f, 1f to 1f),
                keys(p, 0f to 1f, 0.62f to 1f, 0.68f to 0.95f, 0.76f to 1f, 1f to 1f),
                keys(p, 0f to 0f, 0.62f to 0f, 0.68f to 0f, 0.76f to -6f, 0.84f to 5f, 0.91f to -2f, 1f to 0f),
            )
        }
        OrbState.Spawn -> if (u < 1.1f) {
            val p = u / 1.1f
            val sc = keys(p, 0f to 0f, 0.55f to 1.15f, 0.75f to 0.97f, 1f to 1f)
            floatArrayOf(0f, keys(p, 0f to 0f, 0.55f to 0f, 0.75f to -4f, 1f to 0f), sc, sc, keys(p, 0f to -90f, 0.55f to 8f, 0.75f to 0f, 1f to 0f))
        } else REST
        else -> REST
    }
}

private val REST = floatArrayOf(0f, 0f, 1f, 1f, 0f)

private fun hop(p: Float) = floatArrayOf(
    0f, keys(p, 0f to 0f, 0.2f to 1f, 0.5f to -6f, 0.8f to 0f, 1f to 0f),
    keys(p, 0f to 1f, 0.2f to 1.06f, 0.5f to 0.96f, 0.8f to 1.03f, 1f to 1f),
    keys(p, 0f to 1f, 0.2f to 0.94f, 0.5f to 1.04f, 0.8f to 0.97f, 1f to 1f), 0f,
)

/** The dark face with its eyes and brows: it turns, scans, nods or ponders by state. */
private fun DrawScope.face(state: OrbState, t: Float, u: Float, seed: Long, dark: Color, f: FloatArray) {
    var hx = 0f
    var hy = 0f
    var hsx = 1f
    var hrot = 0f
    var alpha = 1f
    var gx = 0f
    var gy = 0f
    when (state) {
        OrbState.Thinking -> {
            val k = 0.5f - 0.5f * cos(2 * PI.toFloat() * t / 3.6f)
            hx = 0.6f + k; hy = -0.6f - 0.6f * k
        }
        OrbState.Searching -> {
            hx = -2.4f + 4.8f * (0.5f - 0.5f * cos(2 * PI.toFloat() * t / 2.6f)); hsx = 0.93f
        }
        OrbState.Reading -> hy = 0.4f + (0.5f - 0.5f * cos(2 * PI.toFloat() * t / 1.8f))
        OrbState.Working -> {
            val p = (t / 1.9f) % 1f
            hx = keys(p, 0f to 0f, 0.3f to 1.4f, 0.6f to -1.4f, 1f to 0f)
            hsx = keys(p, 0f to 1f, 0.3f to 0.97f, 0.6f to 0.97f, 1f to 1f)
        }
        OrbState.Team, OrbState.Spin -> {
            // The face slides off one side (the back of the head shows), then comes back round.
            val p = (t / 3.4f) % 1f
            hx = keys(p, 0f to 0f, 0.22f to 11f, 0.225f to -11f, 0.47f to -11f, 0.7f to 1.5f, 0.8f to 0f, 1f to 0f)
            hsx = keys(p, 0f to 1f, 0.22f to 0.25f, 0.47f to 0.25f, 0.7f to 1.03f, 0.8f to 1f, 1f to 1f)
            alpha = keys(p, 0f to 1f, 0.22f to 0f, 0.47f to 0f, 0.7f to 1f, 1f to 1f)
        }
        OrbState.Reviewing -> {
            val k = 0.5f - 0.5f * cos(2 * PI.toFloat() * t / 2.4f)
            hx = 1f + k; hrot = 4f + 3f * k
        }
        OrbState.Sleeping -> hy = 1f
        OrbState.Failed -> if (u < 1f) hx = keys((u / 0.5f) % 1f, 0f to 0f, 0.25f to -0.8f, 0.75f to 0.8f, 1f to 0f)
        OrbState.Idle, OrbState.Unread, OrbState.Spawn -> {
            // Now and then it looks around, and turns its head to one side.
            val hp = ((t + seed % 17000 / 1000f) / 17f) % 1f
            hx = keys(hp, 0f to 0f, 0.62f to 0f, 0.68f to -3f, 0.78f to -3f, 0.84f to 1.2f, 0.88f to 0f, 1f to 0f)
            hsx = keys(hp, 0f to 1f, 0.62f to 1f, 0.68f to 0.92f, 0.78f to 0.92f, 0.84f to 0.97f, 0.88f to 1f, 1f to 1f)
            val gp = ((t + seed % 11000 / 1000f) / 11f) % 1f
            gx = keys(gp, 0f to 0f, 0.3f to 0f, 0.34f to -2.2f, 0.44f to -2.2f, 0.5f to 2f, 0.58f to 2f, 0.63f to 0f, 1f to 0f)
            gy = keys(gp, 0f to 0f, 0.3f to 0f, 0.34f to 0.3f, 0.44f to 0.3f, 0.5f to -1.2f, 0.58f to -1.2f, 0.63f to 0f, 1f to 0f)
        }
        else -> {}
    }
    if (alpha <= 0.01f) return
    withTransform({
        translate(hx, hy)
        rotate(hrot, pivot = Offset(30.5f, 33f))
        scale(hsx, 1f, pivot = Offset(30.5f, 33f))
    }) {
        drawCircle(dark.copy(alpha = alpha), radius = 14.5f, center = Offset(30.5f, 33f))
        val blink = if (state == OrbState.Sleeping) 1f else {
            val bp = ((t + seed % 6100 / 1000f) / 6.1f) % 1f
            keys(bp, 0f to 1f, 0.93f to 1f, 0.95f to 0.12f, 0.97f to 1f, 1f to 1f)
        }
        val open = f[14].coerceIn(0f, 1f) * alpha
        val happy = f[15].coerceIn(0f, 1f) * alpha
        val closed = f[16].coerceIn(0f, 1f) * alpha
        translate(gx, gy) {
            for (side in 0..1) {
                val o = side * 5
                val cx = if (side == 0) 26.2f else 36f
                val pivot = Offset(cx, 31.3f)
                withTransform({
                    translate(f[o + 3], f[o + 4])
                    rotate(f[o + 2], pivot = pivot)
                    scale(f[o], f[o + 1], pivot = pivot)
                }) {
                    if (open > 0.01f) scale(1f, blink, pivot = pivot) {
                        drawRoundRect(Color.White.copy(alpha = 0.22f * open), Offset(cx - 3.6f, 25.8f), Size(7.2f, 11f), CornerRadius(3.6f))
                        drawRoundRect(Color.White.copy(alpha = open), Offset(cx - 2.8f, 26.6f), Size(5.6f, 9.4f), CornerRadius(2.8f))
                    }
                    if (happy > 0.01f) {
                        val p = if (side == 0) HAPPY_L else HAPPY_R
                        drawPath(p, Color.White.copy(alpha = 0.22f * happy), style = Stroke(4f, cap = StrokeCap.Round))
                        drawPath(p, Color.White.copy(alpha = happy), style = Stroke(2.6f, cap = StrokeCap.Round))
                    }
                    if (closed > 0.01f) {
                        drawPath(if (side == 0) CLOSED_L else CLOSED_R, Color.White.copy(alpha = closed), style = Stroke(2.2f, cap = StrokeCap.Round))
                    }
                }
            }
        }
        val browAlpha = f[13].coerceIn(0f, 1f) * alpha
        if (browAlpha > 0.01f) {
            val white = Color.White.copy(alpha = browAlpha)
            withTransform({ translate(0f, f[12]); rotate(f[10], pivot = Offset(25.9f, 23.8f)) }) {
                drawLine(white, Offset(23.2f, 23.8f), Offset(28.6f, 23.8f), 1.8f, StrokeCap.Round)
            }
            withTransform({ translate(0f, f[12]); rotate(f[11], pivot = Offset(36.3f, 23.8f)) }) {
                drawLine(white, Offset(33.6f, 23.8f), Offset(39f, 23.8f), 1.8f, StrokeCap.Round)
            }
        }
    }
}

private fun DrawScope.ringHalf(color: Color, back: Boolean, alpha: Float) {
    rotate(-14f, pivot = Offset(32f, 37f)) {
        drawArc(
            color = color.copy(alpha = (if (back) 0.5f else 1f) * alpha),
            startAngle = if (back) 180f else 0f,
            sweepAngle = 180f,
            useCenter = false,
            topLeft = Offset(4f, 28.5f),
            size = Size(56f, 17f),
            style = Stroke(2.4f, cap = StrokeCap.Round),
        )
    }
}

private fun DrawScope.sparkle(state: OrbState, t: Float, u: Float, seed: Long) {
    val spin = when (state) {
        OrbState.Working, OrbState.Searching, OrbState.Designing -> (t / 1.6f) % 1f
        OrbState.Celebrate -> if (u < 1.8f) (u / 0.6f) % 1f else -1f
        OrbState.Spawn -> if (u < 1.4f) (u / 0.7f) % 1f else -1f
        else -> -1f
    }
    rotate(-14f, pivot = Offset(32f, 37f)) {
        val pivot = Offset(57f, 32f)
        if (state == OrbState.Sleeping) {
            drawPath(STAR, Color.White.copy(alpha = 0.35f))
        } else if (spin >= 0f) {
            withTransform({ rotate(360f * spin, pivot = pivot); scale(1.1f, 1.1f, pivot = pivot) }) { drawPath(STAR, Color.White) }
        } else {
            val k = 0.5f - 0.5f * cos(2 * PI.toFloat() * (t + seed % 4000 / 1000f) / 2.8f)
            scale(0.75f + 0.4f * k, 0.75f + 0.4f * k, pivot = pivot) { drawPath(STAR, Color.White.copy(alpha = 0.7f + 0.3f * k)) }
        }
    }
}

/** The little extras: thought dots, z's, a sweat drop, a burst of sparkles, a reply ping. */
private fun DrawScope.effect(state: OrbState, t: Float, u: Float, light: Color) {
    when (state) {
        OrbState.Thinking -> for ((i, d) in THINK_DOTS.withIndex()) {
            val p = ((t - i * 0.25f) / 1.8f).mod(1f)
            val a = keys(p, 0f to 0f, 0.35f to 0.95f, 0.7f to 0.95f, 1f to 0f)
            val dy = keys(p, 0f to 2f, 0.35f to 0f, 0.7f to 0f, 1f to -2f)
            val sc = keys(p, 0f to 0.6f, 0.35f to 1f, 0.7f to 1f, 1f to 0.9f)
            drawCircle(Color.White.copy(alpha = a), d.z * sc, Offset(d.x, d.y + dy))
        }
        OrbState.Sleeping -> for (i in 0..2) {
            val p = ((t - i * 1.2f) / 3.6f).mod(1f)
            val a = keys(p, 0f to 0f, 0.25f to 0.9f, 1f to 0f)
            val sc = 0.6f + 0.65f * p
            withTransform({ translate(9f * p, -14f * p); scale(sc, sc, pivot = Offset(48f, 13f)) }) {
                drawPath(ZED, Color.White.copy(alpha = a), style = Stroke(1.5f, cap = StrokeCap.Round))
            }
        }
        OrbState.Failed -> {
            val p = (t / 1.9f) % 1f
            translate(0f, keys(p, 0f to -2f, 0.2f to -1f, 0.7f to 2f, 1f to 5f)) {
                drawPath(DROP, SWEAT.copy(alpha = keys(p, 0f to 0f, 0.2f to 0.95f, 0.7f to 0.95f, 1f to 0f)))
            }
        }
        OrbState.Celebrate, OrbState.Spawn -> for ((i, d) in BURST.withIndex()) {
            val p = ((u - i * 0.04f) / 0.9f).coerceIn(0f, 1f)
            if (p <= 0f || p >= 1f) continue
            val e = 1 - (1 - p) * (1 - p) * (1 - p)
            val a = if (p < 0.7f) 1f else 1f - (p - 0.7f) / 0.3f
            withTransform({
                translate(d.x * 26f * e, d.y * 24f * e)
                rotate(90f * e, pivot = Offset(32f, 32f))
                scale(0.3f + 0.7f * e, 0.3f + 0.7f * e, pivot = Offset(32f, 32f))
            }) { drawPath(MINI_STAR, (if (i % 2 == 1) light else Color.White).copy(alpha = a)) }
        }
        OrbState.Unread -> {
            val p = (t / 1.6f) % 1f
            drawCircle(light, 4.2f, Offset(53f, 11f))
            drawCircle(light.copy(alpha = 0.9f * (1 - p)), 4.2f * (1 + 1.2f * p), Offset(53f, 11f), style = Stroke(1.6f))
        }
        else -> {}
    }
}

/** The step's icon on the orb's corner (thinking shows bouncing dots). */
@Composable
private fun BoxScope.OrbChip(state: OrbState, activity: String?, clock: MutableFloatState) {
    val warn = state == OrbState.Waiting
    val icon = when {
        state == OrbState.Thinking -> null
        warn -> Lucide.Hourglass
        state == OrbState.Team -> Lucide.Users
        state == OrbState.Designing -> Lucide.Sparkles
        activity == "run_command" -> Lucide.Terminal
        activity?.contains("file") == true || state == OrbState.Reading -> Lucide.FileText
        activity == "web_fetch" || activity?.startsWith("browser_") == true -> Lucide.Globe
        else -> Lucide.Search
    }
    val shown = remember { MutableTransitionState(false).apply { targetState = true } }
    AnimatedVisibility(
        shown, Modifier.align(Alignment.BottomEnd).offset(3.dp, 3.dp),
        enter = scaleIn(spring(dampingRatio = 0.5f, stiffness = 600f)), exit = scaleOut(),
    ) {
        Box(
            Modifier.size(17.dp).border(2.dp, C.bg, CircleShape).padding(2.dp)
                .background(if (warn) Color(0xFF4A3A12) else Color(0xFF2D2D3D), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (icon != null) Icon(icon, if (warn) C.warn else Color(0xFFD4D4E0), 9.dp)
            else Canvas(Modifier.size(11.dp, 5.dp)) {
                val t = clock.floatValue
                for (i in 0..2) {
                    val p = ((t - i * 0.15f) / 1.1f).mod(1f)
                    val k = keys(p, 0f to 0f, 0.4f to 1f, 1f to 0f)
                    drawCircle(
                        Color(0xFFD4D4E0).copy(alpha = 0.35f + 0.65f * k), 1.3.dp.toPx(),
                        Offset(this.size.width / 2 + (i - 1) * 3.6.dp.toPx(), this.size.height / 2 - 1.5.dp.toPx() * k),
                    )
                }
            }
        }
    }
}

// ---- shapes ------------------------------------------------------------------------------

private val INK = Color(0xFF0B0B14)
private val SWEAT = Color(0xFF9FD8FF)

private val RAINBOW_SWEEP = Brush.sweepGradient(
    0f to Color.Transparent, 0.19f to Color.Transparent, 0.39f to C.rainbow[0],
    0.56f to C.rainbow[1], 0.72f to C.rainbow[2], 0.86f to C.rainbow[3], 1f to C.rainbow[4],
)

private val STAR = Path().apply {
    moveTo(57f, 26.5f); lineTo(58.4f, 30.6f); lineTo(62.5f, 32f); lineTo(58.4f, 33.4f)
    lineTo(57f, 37.5f); lineTo(55.6f, 33.4f); lineTo(51.5f, 32f); lineTo(55.6f, 30.6f); close()
}

private val MINI_STAR = Path().apply {
    moveTo(32f, 28.5f); lineTo(33f, 31f); lineTo(35.5f, 32f); lineTo(33f, 33f)
    lineTo(32f, 35.5f); lineTo(31f, 33f); lineTo(28.5f, 32f); lineTo(31f, 31f); close()
}

private fun arc(cx: Float, y0: Float, yc: Float, w: Float) = Path().apply { moveTo(cx - w, y0); quadraticTo(cx, yc, cx + w, y0) }
private val HAPPY_L = arc(26.2f, 32.6f, 27.4f, 3f)
private val HAPPY_R = arc(36f, 32.6f, 27.4f, 3f)
private val CLOSED_L = arc(26.2f, 32f, 34.6f, 2.9f)
private val CLOSED_R = arc(36f, 32f, 34.6f, 2.9f)

private val DROP = Path().apply { moveTo(47f, 14f); quadraticTo(44f, 19.5f, 47f, 21f); quadraticTo(50f, 19.5f, 47f, 14f); close() }
private val ZED = Path().apply { moveTo(46f, 10.5f); lineTo(50.5f, 10.5f); lineTo(46f, 16f); lineTo(50.5f, 16f) }

private class Dot3(val x: Float, val y: Float, val z: Float)
private val THINK_DOTS = listOf(Dot3(47f, 15f, 1.5f), Dot3(51.5f, 10f, 2f), Dot3(57f, 4.5f, 2.6f))
private val BURST = listOf(Offset(-1f, -1f), Offset(1f, -1.2f), Offset(1.3f, 0.4f), Offset(-1.3f, 0.3f), Offset(0f, -1.5f), Offset(0.6f, 1.2f))

// ---- the shared clock ---------------------------------------------------------------------

/** The orbs' shared clocks in seconds: [fast] ticks every frame, [slow] at about 30 fps. */
class OrbClock(val fast: MutableFloatState, val slow: MutableFloatState, val still: Boolean = false) {
    /** Orbs on screen; with none (Settings) the frame loop rests. */
    val users = mutableIntStateOf(0)
}

val LocalOrbClock = staticCompositionLocalOf { OrbClock(mutableFloatStateOf(0f), mutableFloatStateOf(0f)) }

/**
 * One frame loop for every orb on screen (instead of an animation each). It stands still when
 * the phone's "Remove animations" setting is on.
 */
@Composable
fun ProvideOrbClock(content: @Composable () -> Unit) {
    val context = LocalContext.current
    val still = remember {
        android.provider.Settings.Global.getFloat(context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
    val clock = remember { OrbClock(mutableFloatStateOf(0f), mutableFloatStateOf(0f), still) }
    if (!still) {
        LaunchedEffect(Unit) {
            val start = System.nanoTime()
            var lastSlow = 0L
            snapshotFlow { clock.users.intValue > 0 }.collectLatest { active ->
                while (active) {
                    withFrameNanos { now ->
                        val t = ((System.nanoTime() - start) / 1_000_000_000.0 % 3600.0).toFloat()
                        clock.fast.floatValue = t
                        if (now - lastSlow >= 32_000_000L) {
                            lastSlow = now
                            clock.slow.floatValue = t
                        }
                    }
                }
            }
        }
    }
    CompositionLocalProvider(LocalOrbClock provides clock, content = content)
}

/** Keyframe interpolation with ease-in-out between keys (like the CSS animations). */
private fun keys(p: Float, vararg k: Pair<Float, Float>): Float {
    for (i in 0 until k.size - 1) {
        val (p0, v0) = k[i]
        val (p1, v1) = k[i + 1]
        if (p in p0..p1) {
            if (p1 <= p0) return v1
            val x = (p - p0) / (p1 - p0)
            return v0 + (v1 - v0) * (x * x * (3 - 2 * x))
        }
    }
    return k.last().second
}
