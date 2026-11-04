package dev.iris.app.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.Problem
import dev.iris.app.data.Push
import dev.iris.app.data.PushPrefs
import dev.iris.app.link.LinkState
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.compose.animation.AnimatedVisibility
import kotlinx.coroutines.launch

// ---- Problems ---------------------------------------------------------------------------

/** The problems inbox: failed replies and routines, newest first, each with Retry, its chat and Dismiss. */
@Composable
fun ProblemsPage(vm: IrisViewModel, back: () -> Unit) {
    val list = vm.problems.sortedByDescending { it.createdAt }
    Page(
        "Problems", back,
        subtitle = if (list.isEmpty()) null else "${list.size} need a look",
        action = if (list.size > 1) ({ Txt("Dismiss all", 13.sp, C.muted, FontWeight.Medium, Modifier.press { vm.dismissProblem() }.padding(12.dp)) }) else null,
    ) {
        if (list.isEmpty()) {
            item {
                Column(Modifier.fillMaxWidth().padding(top = 60.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Lucide.CircleCheck, C.success, 30.dp)
                    Txt("Nothing needs a look", 15.sp, C.text, FontWeight.Medium)
                    Txt("Failed replies and routines show up here, with a way to try again.", 13.sp, C.faint)
                }
            }
        }
        items(list, key = { it.id }) { p -> Box(Modifier.animateItem()) { ProblemCard(vm, p) } }
    }
}

@Composable
private fun ProblemCard(vm: IrisViewModel, p: Problem) {
    val agent = vm.agent(p.agentId)
    val shape = RoundedCornerShape(18.dp)
    Column(
        Modifier.fillMaxWidth().background(C.dangerSoft.copy(alpha = 0.06f), shape).border(1.dp, C.danger.copy(alpha = 0.25f), shape).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            if (agent != null) Orb(agent, 34.dp, mood = "sad")
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Txt(p.title.ifEmpty { "Something failed" }, 14.sp, C.text, FontWeight.SemiBold, Modifier.weight(1f), maxLines = 2)
                    Txt(relativeTime(p.createdAt), 11.5.sp, C.faint, modifier = Modifier.padding(start = 8.dp))
                }
                Row(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(if (p.kind == "routine") Lucide.Calendar else Lucide.MessageText, C.faint, 12.dp)
                    Txt("${if (p.kind == "routine") "Routine" else "Reply"} · ${agent?.name ?: "removed agent"}", 12.sp, C.faint, maxLines = 1)
                }
            }
        }
        if (p.detail.isNotEmpty()) {
            Txt(
                p.detail, 12.sp, C.muted, mono = true, maxLines = 4, lineHeight = 17.sp,
                modifier = Modifier.fillMaxWidth().background(C.code, RoundedCornerShape(10.dp)).padding(horizontal = 10.dp, vertical = 7.dp),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            var busy by remember { mutableStateOf(false) }
            Button("Retry", { busy = true; vm.retryProblem(p) }, icon = Lucide.RotateCcw, primary = false, enabled = agent != null, busy = busy)
            if (agent != null) Button("Open chat", { vm.openAgent(agent.id) }, icon = Lucide.MessageText, primary = false)
            Box(Modifier.weight(1f))
            IconButton(Lucide.X, { vm.dismissProblem(p.id) }, C.faint, 16.dp)
        }
    }
}

// ---- Notifications ----------------------------------------------------------------------

/** Notifications through the app's own background link, and what gets sent. */
@Composable
fun NotificationsPage(vm: IrisViewModel, back: () -> Unit) {
    val context = LocalContext.current
    val on by Push.enabled.collectAsState()
    val link by Push.linkState.collectAsState()
    val prefs by Push.choices.collectAsState()
    // Re-read when coming back from Android's battery screen.
    var unrestricted by remember { mutableStateOf(Push.unrestricted(context)) }
    LifecycleResumeEffect(Unit) {
        unrestricted = Push.unrestricted(context)
        onPauseOrDispose {}
    }

    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) Push.enable(context) else vm.showToast("Notifications are blocked for I.R.I.S. in Android's settings")
    }
    fun turnOn() {
        val needs = Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        if (needs) permission.launch(Manifest.permission.POST_NOTIFICATIONS) else Push.enable(context)
    }

    Page("Notifications", back) {
        item {
            Card(accent = true) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(42.dp).background(C.accentSoft, RoundedCornerShape(14.dp)), contentAlignment = Alignment.Center) {
                        Icon(Lucide.Bell, C.accentHi, 21.dp)
                    }
                    Column(Modifier.weight(1f)) {
                        Txt("This phone", 16.sp, C.text, FontWeight.SemiBold)
                        Txt(if (on) "Notifications are on" else "Off", 13.sp, if (on) C.success else C.faint)
                    }
                    Switch(on, { if (it) turnOn() else Push.disable(context) })
                }
                Hint(
                    "I.R.I.S. keeps its own encrypted connection to your server while the app is closed, and tells you " +
                        "about replies, questions, approvals and problems. No Google or other push service is involved. " +
                        "Android shows a small \"I.R.I.S. notifications\" entry while it's on; long-press it to hide it.",
                )
                AnimatedVisibility(on) {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Divider()
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Dot(if (link == LinkState.Online) C.success else C.faint)
                            Txt(
                                when (link) {
                                    LinkState.Online -> "Connected in the background"
                                    LinkState.ServerOffline -> "Your server is offline"
                                    LinkState.Connecting -> "Connecting…"
                                    LinkState.Offline -> "Standing by while the app is open"
                                },
                                13.5.sp, C.muted,
                            )
                        }
                        Button("Send a test", {
                            Push.show(context, "replies", "I.R.I.S.", "Notifications work on this phone.", "test", null)
                        }, icon = Lucide.Send, primary = false)
                    }
                }
            }
        }
        if (on && !unrestricted) {
            item {
                Card {
                    CardTitle(
                        "Let it run in the background",
                        "Samsung puts apps to sleep to save battery, which would cut the connection. Allow I.R.I.S. to run unrestricted so notifications arrive on time.",
                    )
                    Button("Allow background use", {
                        runCatching {
                            context.startActivity(
                                Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}")),
                            )
                        }
                    }, icon = Lucide.Zap)
                }
            }
        }
        item {
            val p = prefs
            Card {
                CardTitle("What to send")
                run {
                    fun put(next: PushPrefs) = Push.choose(context, next)
                    SwitchRow("Replies", "An agent finished a reply while you were away.", p.replies, { put(p.copy(replies = it)) })
                    Divider()
                    SwitchRow("Questions", "An agent is asking you something.", p.questions, { put(p.copy(questions = it)) })
                    Divider()
                    SwitchRow("Approvals", "A command waits for your OK.", p.approvals, { put(p.copy(approvals = it)) })
                    Divider()
                    SwitchRow("Problems", "A reply or routine failed.", p.problems, { put(p.copy(problems = it)) })
                }
            }
        }
    }
}
