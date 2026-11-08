package dev.iris.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import dev.iris.app.data.Conn
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.Push
import dev.iris.app.data.Screen
import dev.iris.app.ui.AgentEditScreen
import dev.iris.app.ui.ApprovalCard
import dev.iris.app.ui.CallBackdropLayer
import dev.iris.app.ui.CallScreen
import dev.iris.app.ui.OnCallBar
import dev.iris.app.ui.C
import dev.iris.app.ui.ChatScreen
import dev.iris.app.ui.ConnectionBar
import dev.iris.app.ui.HomeScreen
import dev.iris.app.ui.IrisTheme
import dev.iris.app.ui.Motion
import dev.iris.app.ui.OverlayHost
import dev.iris.app.ui.ProvideOrbClock
import dev.iris.app.ui.Overlays
import dev.iris.app.ui.SettingsScreen
import dev.iris.app.ui.SignInScreen
import dev.iris.app.ui.Spinner
import dev.iris.app.ui.Toast

class MainActivity : ComponentActivity() {
    private val vm: IrisViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        Push.load(this)
        if (savedInstanceState == null) openFrom(intent)
        setContent { IrisTheme { ProvideOrbClock { App(vm) } } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        openFrom(intent)
    }

    /** A tapped notification carries the agent whose chat to open. */
    private fun openFrom(intent: Intent?) {
        intent?.getStringExtra(EXTRA_AGENT)?.takeIf { it.isNotEmpty() }?.let(vm::openFromNotification)
    }

    override fun onStart() {
        super.onStart()
        Push.foreground.value = true
        // On screen, Android always lets the background link start: make sure it's running.
        if (Push.enabled.value) Push.start(this)
        // Phones pause sockets in the background: reconnect as soon as the app is back.
        vm.resume()
    }

    override fun onStop() {
        super.onStop()
        Push.foreground.value = false
        if (!isChangingConfigurations) vm.pause()
    }

    companion object {
        const val EXTRA_AGENT = "agent_id"
    }
}

@Composable
private fun App(vm: IrisViewModel) {
    val overlays = remember { Overlays() }
    OverlayHost(overlays) {
        // Any touch wakes dozing orbs.
        Box(Modifier.fillMaxSize().background(C.bg).pointerInput(Unit) {
            awaitPointerEventScope { while (true) { awaitPointerEvent(PointerEventPass.Initial); vm.touched() } }
        }) {
            // The call's colored glow lives behind everything, so it reaches the screen's edges.
            CallBackdropLayer(vm)
            AnimatedContent(
                vm.conn == Conn.SignIn,
                transitionSpec = { fadeIn(tween(300)) togetherWith fadeOut(tween(200)) },
                label = "signin",
            ) { signIn ->
                if (signIn) SignInScreen(vm) else Main(vm)
            }
            AnimatedVisibility(
                vm.toast != null,
                Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.systemBars.union(WindowInsets.ime)).padding(bottom = 96.dp),
                enter = fadeIn() + slideInVertically { it / 2 },
                exit = fadeOut() + slideOutVertically { it / 2 },
            ) { vm.toast?.let { Toast(it, Modifier.fillMaxWidth(0.9f)) } }
        }
    }
}

@Composable
private fun Main(vm: IrisViewModel) {
    BackHandler(vm.stack.size > 1) { vm.back() }
    Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars.union(WindowInsets.ime))) {
        ConnectionBar(vm.linkState, vm.providerOnline) { vm.nudge() }
        OnCallBar(vm)
        Box(Modifier.weight(1f)) {
            if (vm.conn == Conn.Starting && vm.agents.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Spinner(24.dp, C.faint) }
            } else {
                // Each screen with how deep it is, so the slide goes the right way.
                AnimatedContent(
                    vm.screen to vm.stack.size,
                    transitionSpec = {
                        val forward = targetState.second > initialState.second ||
                            (targetState.second == initialState.second && targetState.first != initialState.first && initialState.first is Screen.Home)
                        // The new screen glides in a short way while the old one steps back and fades.
                        val move = tween<androidx.compose.ui.unit.IntOffset>(300, easing = Motion.ease)
                        if (forward) {
                            slideInHorizontally(move) { it / 5 } + fadeIn(tween(220, 40)) togetherWith
                                slideOutHorizontally(move) { -it / 12 } + fadeOut(tween(160))
                        } else {
                            slideInHorizontally(move) { -it / 12 } + fadeIn(tween(220, 40)) togetherWith
                                slideOutHorizontally(move) { it / 5 } + fadeOut(tween(160))
                        }
                    },
                    contentKey = { it.first },
                    label = "screens",
                ) { (screen, _) ->
                    // The call screen is see-through: the glow behind the window shows through it.
                    Box(Modifier.fillMaxSize().background(if (screen is Screen.Call) androidx.compose.ui.graphics.Color.Transparent else C.bg)) {
                        when (screen) {
                            Screen.Home -> HomeScreen(vm)
                            is Screen.Chat -> vm.agent(screen.agentId)?.let { ChatScreen(vm, it) }
                            is Screen.Settings -> SettingsScreen(vm, screen.page)
                            is Screen.AgentEdit -> vm.agent(screen.agentId)?.let { AgentEditScreen(vm, it) }
                            is Screen.Call -> vm.agent(screen.agentId)?.let { CallScreen(vm, it) }
                        }
                    }
                }
            }
            ApprovalBanner(vm, Modifier.align(Alignment.TopCenter))
        }
    }
}

/** An approval from a chat that isn't open pops in at the top. */
@Composable
private fun ApprovalBanner(vm: IrisViewModel, modifier: Modifier) {
    val current = (vm.screen as? Screen.Chat)?.agentId
    val elsewhere = vm.pendingApprovals.firstOrNull { current != null && it.agentId != current }
    AnimatedVisibility(
        elsewhere != null,
        modifier.padding(10.dp),
        enter = slideInVertically() + fadeIn(),
        exit = slideOutVertically() + fadeOut(),
    ) {
        elsewhere?.let { Box(Modifier.background(C.bg, RoundedCornerShape(18.dp))) { ApprovalCard(vm, it) } }
    }
}
