package dev.iris.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Bottom sheets drawn inside the app's own window (so they reach under the status and
 * navigation bars), with an animated way in and out. Anything can open one through
 * [LocalOverlays].
 */
class Overlays {
    internal var content by mutableStateOf<(@Composable ColumnScope.(dismiss: () -> Unit) -> Unit)?>(null)
    internal val visible = MutableTransitionState(false)

    fun show(sheet: @Composable ColumnScope.(dismiss: () -> Unit) -> Unit) {
        content = sheet
        visible.targetState = true
    }

    fun dismiss() {
        visible.targetState = false
    }
}

val LocalOverlays = staticCompositionLocalOf { Overlays() }

@Composable
fun OverlayHost(overlays: Overlays, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalOverlays provides overlays) {
        Box(Modifier.fillMaxSize()) {
            content()
            val sheet = overlays.content
            val state = overlays.visible
            // Gone for good once the way out has finished.
            LaunchedEffect(state.currentState, state.isIdle) {
                if (state.isIdle && !state.currentState && !state.targetState) overlays.content = null
            }
            if (sheet != null) {
                BackHandler(state.targetState) { overlays.dismiss() }
                AnimatedVisibility(state, enter = EnterTransition.None, exit = ExitTransition.None) {
                    Box(Modifier.fillMaxSize()) {
                        Box(
                            Modifier.fillMaxSize()
                                .animateEnterExit(enter = fadeIn(tween(180)), exit = fadeOut(tween(220)))
                                .background(Color.Black.copy(alpha = 0.55f))
                                .clickable(remember { MutableInteractionSource() }, null) { overlays.dismiss() },
                        )
                        Column(
                            Modifier.align(Alignment.BottomCenter)
                                .animateEnterExit(
                                    enter = slideInVertically(spring(dampingRatio = 0.92f, stiffness = 480f)) { it },
                                    exit = slideOutVertically(tween(220, easing = Motion.ease)) { it },
                                )
                                .fillMaxWidth()
                                .background(C.elevated, RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
                                .clickable(remember { MutableInteractionSource() }, null) {}
                                .windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.ime))
                                .padding(top = 10.dp, bottom = 10.dp),
                        ) {
                            Box(Modifier.align(Alignment.CenterHorizontally).width(36.dp).height(4.dp).background(C.lineStrong, RoundedCornerShape(2.dp)))
                            Spacer(Modifier.height(6.dp))
                            sheet { overlays.dismiss() }
                        }
                    }
                }
            }
        }
    }
}

class MenuItem(
    val label: String,
    val icon: ImageVector,
    val danger: Boolean = false,
    val enabled: Boolean = true,
    /** Asks for a second tap first (the desktop's "Click again to …"). */
    val confirm: String? = null,
    val action: () -> Unit,
)

/** A sheet of actions: the phone's take on the desktop's right-click menus. */
fun Overlays.menu(title: String?, items: List<MenuItem>) = show { dismiss ->
    var armed by remember { mutableStateOf<MenuItem?>(null) }
    val haptics = LocalHapticFeedback.current
    if (title != null) SheetTitle(title)
    for (item in items) {
        val isArmed = armed === item
        Row(
            Modifier.fillMaxWidth().alpha(if (item.enabled) 1f else 0.4f)
                .press(item.enabled) {
                    if (item.confirm != null && !isArmed) {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        armed = item
                    } else {
                        dismiss()
                        item.action()
                    }
                }
                .heightIn(min = 56.dp)
                .padding(horizontal = 24.dp, vertical = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(18.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(item.icon, if (item.danger) C.danger else C.muted, 21.dp)
            Txt(
                if (isArmed) item.confirm!! else item.label, 16.sp,
                if (item.danger) C.danger else C.text, if (isArmed) FontWeight.SemiBold else FontWeight.Normal,
            )
        }
    }
}

/** A sheet to pick one value from a list (the desktop's Select). */
fun Overlays.picker(
    title: String,
    options: List<Pair<String, String>>,
    selected: String?,
    hint: String? = null,
    onPick: (String) -> Unit,
) = show { dismiss ->
    SheetTitle(title)
    if (hint != null) Txt(hint, 12.5.sp, C.faint, modifier = Modifier.padding(start = 22.dp, end = 22.dp, bottom = 6.dp))
    Column(Modifier.heightIn(max = 460.dp).verticalScroll(rememberScrollState())) {
        for ((value, label) in options) {
            val on = value == selected
            Row(
                Modifier.fillMaxWidth().heightIn(min = 54.dp).press { dismiss(); onPick(value) }.padding(horizontal = 24.dp, vertical = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Txt(label, 15.sp, if (on) C.accentHi else C.text, if (on) FontWeight.Medium else FontWeight.Normal, Modifier.weight(1f))
                if (on) Icon(Lucide.Check, C.accentHi, 17.dp)
            }
        }
    }
}

@Composable
fun SheetTitle(text: String) =
    Txt(text, 13.5.sp, C.faint, FontWeight.Medium, Modifier.padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 8.dp))

/** Tap, plus long-press with a haptic tick (opens an item's menu). */
@OptIn(ExperimentalFoundationApi::class)
fun Modifier.pressLong(onLong: () -> Unit, onClick: () -> Unit): Modifier = composed {
    val haptics = LocalHapticFeedback.current
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) 0.98f else 1f, label = "press")
    graphicsLayer { scaleX = scale; scaleY = scale }
        .combinedClickable(
            interactionSource = source,
            indication = null,
            onLongClick = {
                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                onLong()
            },
            onClick = onClick,
        )
}
