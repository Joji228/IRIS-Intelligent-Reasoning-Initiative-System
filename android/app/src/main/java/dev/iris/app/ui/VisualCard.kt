package dev.iris.app.ui

import android.annotation.SuppressLint
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.MotionEvent
import androidx.compose.runtime.staticCompositionLocalOf
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import org.json.JSONObject
import java.io.ByteArrayInputStream
import kotlin.math.roundToInt

// Visual cards (Intelligent UI): a reply can carry a small interactive page, as a
// ```visual block of HTML. It runs in a WebView that can't load anything from the
// network or open links, inside the shared frame (app/public/visual.html, packed into
// the app's assets), which adds the I.R.I.S. style kit and reports the page's height.

/**
 * Sends a card's answer (a `data-send` button or form) to the chat as the user. Provided
 * by the chat screen; elsewhere (release notes, say) cards can't send.
 */
val LocalVisualSend = staticCompositionLocalOf<((String) -> Unit)?> { null }

/** A card may send only this soon after a real touch on it, and not more often than this. */
private const val SEND_AFTER_TOUCH_MS = 2500L
private const val SEND_GAP_MS = 1500L

/** The tallest a card gets in the chat; taller visuals scroll, or open full screen. */
private const val MAX_HEIGHT = 560

/** Heights already measured, so a card scrolled back into view doesn't jump. */
private val heights = HashMap<Int, Int>()

private const val BASE = "https://visual.iris.invalid/"

private var frame: String? = null
private fun frameHtml(context: Context): String =
    frame ?: context.assets.open("visual.html").bufferedReader().use { it.readText() }.also { frame = it }

private fun Color.css() = "rgba(${(red * 255).roundToInt()},${(green * 255).roundToInt()},${(blue * 255).roundToInt()},$alpha)"

private fun theme() = JSONObject().apply {
    put("scheme", "dark")
    put("bg", C.bg.css()); put("surface", C.surface.css()); put("elevated", C.elevated.css()); put("hover", C.hover.css())
    put("line", C.line.css()); put("line-strong", C.lineStrong.css())
    put("text", C.text.css()); put("muted", C.muted.css()); put("faint", C.faint.css())
    put("accent", C.accent.css()); put("accent-hi", C.accentHi.css()); put("accent-soft", C.accentSoft.css())
    put("success", C.success.css()); put("danger", C.danger.css()); put("warn", C.warn.css())
}

/** What the frame says back: it's ready for the visual, the visual's height, an error. */
private class Bridge(private val onMessage: (JSONObject) -> Unit) {
    private val main = Handler(Looper.getMainLooper())

    @JavascriptInterface
    fun post(json: String) {
        val msg = runCatching { JSONObject(json) }.getOrNull() ?: return
        main.post { onMessage(msg) }
    }
}

@SuppressLint("SetJavaScriptEnabled", "ClickableViewAccessibility")
private fun visualView(
    context: Context,
    html: String,
    fontScale: Float,
    onSize: (Int) -> Unit,
    onError: (String) -> Unit,
    onSend: ((String) -> Unit)?,
): WebView {
    val view = WebView(context)
    val payload = JSONObject().put("type", "render").put("html", html).put("theme", theme()).toString()
    var sent = false
    // A visual's own code can ask to send at any time; only a request right after the
    // user's finger left the card counts (touches are seen here, outside the page).
    var lastTouch = 0L
    var lastSend = 0L
    view.setOnTouchListener { _, e ->
        if (e.actionMasked == MotionEvent.ACTION_UP) lastTouch = SystemClock.uptimeMillis()
        false
    }
    view.setBackgroundColor(android.graphics.Color.TRANSPARENT)
    view.isVerticalScrollBarEnabled = false
    view.isHorizontalScrollBarEnabled = false
    view.settings.apply {
        javaScriptEnabled = true
        blockNetworkLoads = true
        allowFileAccess = false
        allowContentAccess = false
        domStorageEnabled = false
        javaScriptCanOpenWindowsAutomatically = false
        setSupportMultipleWindows(false)
        setSupportZoom(false)
        mediaPlaybackRequiresUserGesture = true
        // The same text size as the chat around it.
        textZoom = (fontScale * 100).roundToInt()
    }
    view.webViewClient = object : WebViewClient() {
        // No navigation and no requests: a visual is self-contained. Only the frame
        // page itself (loaded from the app, as data) goes through.
        override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest) = true
        override fun shouldInterceptRequest(v: WebView, request: WebResourceRequest): WebResourceResponse? {
            val url = request.url.toString()
            if (request.isForMainFrame && (url.startsWith("data:") || url == BASE)) return null
            return WebResourceResponse("text/plain", "utf-8", 403, "Blocked", emptyMap(), ByteArrayInputStream(ByteArray(0)))
        }
    }
    view.addJavascriptInterface(
        Bridge { msg ->
            when (msg.optString("type")) {
                "ready" -> if (!sent) {
                    sent = true
                    view.evaluateJavascript("window.postMessage(${JSONObject.quote(payload)}, '*')", null)
                }
                "size" -> onSize(msg.optInt("height").coerceAtLeast(0))
                "error" -> onError(msg.optString("message").take(200))
                "send" -> {
                    val now = SystemClock.uptimeMillis()
                    val text = msg.optString("text").trim().take(1000)
                    if (onSend != null && text.isNotEmpty() && now - lastTouch < SEND_AFTER_TOUCH_MS && now - lastSend > SEND_GAP_MS) {
                        lastSend = now
                        onSend(text)
                    }
                }
            }
        },
        "IrisHost",
    )
    view.loadDataWithBaseURL(BASE, frameHtml(context), "text/html", "utf-8", null)
    return view
}

/** A visual card in a reply. While the reply is still streaming the block, a placeholder. */
@Composable
fun VisualCard(title: String, html: String, closed: Boolean) {
    val key = html.hashCode()
    var showCode by remember { mutableStateOf(false) }
    var full by remember { mutableStateOf(false) }
    var height by remember(key) { mutableIntStateOf(heights[key] ?: 0) }
    var error by remember(key) { mutableStateOf<String?>(null) }
    val fontScale = LocalDensity.current.fontScale
    val send = LocalVisualSend.current
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(Radius.md)).background(C.surface)) {
        Row(Modifier.fillMaxWidth().padding(start = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Lucide.Sparkles, C.accentHi, 16.dp)
            Txt(title.ifBlank { "Visual" }, 13.sp, C.muted, FontWeight.Medium, Modifier.weight(1f).padding(start = 8.dp), maxLines = 1)
            if (closed) {
                IconButton(Lucide.Code, { showCode = !showCode }, tint = if (showCode) C.accentHi else C.faint, size = 16.dp)
                IconButton(Lucide.Maximize, { full = true }, size = 16.dp)
            } else {
                Box(Modifier.height(48.dp))
            }
        }
        when {
            !closed -> Box(Modifier.fillMaxWidth().height(120.dp), contentAlignment = Alignment.Center) { Shimmer("Building visual…") }
            showCode -> BasicText(
                html,
                style = style(12.5.sp, C.muted, lineHeight = 19.sp, mono = true),
                softWrap = false,
                modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 14.dp, end = 14.dp, bottom = 14.dp),
            )
            else -> Box(Modifier.fillMaxWidth().height(if (height > 0) height.coerceAtMost(MAX_HEIGHT).dp else 120.dp)) {
                AndroidView(
                    factory = { ctx ->
                        visualView(ctx, html, fontScale, onSize = { if (it > 0) { height = it; heights[key] = it } }, onError = { error = it }, onSend = send)
                    },
                    modifier = Modifier.fillMaxSize(),
                    onRelease = { it.destroy() },
                )
                if (height == 0 && error == null) Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Shimmer("Building visual…") }
            }
        }
        if (!showCode && closed) error?.let {
            Txt("This visual hit an error: $it", 12.sp, C.danger, modifier = Modifier.padding(start = 14.dp, end = 14.dp, bottom = 12.dp), maxLines = 2)
        }
    }
    if (full) VisualFullScreen(title, html, send?.let { s -> { text: String -> full = false; s(text) } }) { full = false }
}

/** A visual on its own screen, scrolling if it's tall. */
@Composable
private fun VisualFullScreen(title: String, html: String, onSend: ((String) -> Unit)?, onClose: () -> Unit) {
    val fontScale = LocalDensity.current.fontScale
    Dialog(onClose, DialogProperties(usePlatformDefaultWidth = false)) {
        Column(Modifier.fillMaxSize().background(C.bg).systemBarsPadding()) {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Lucide.Sparkles, C.accentHi, 20.dp)
                Txt(title.ifBlank { "Visual" }, 16.sp, C.text, FontWeight.SemiBold, Modifier.weight(1f).padding(start = 10.dp), maxLines = 1)
                IconButton(Lucide.X, onClose, tint = C.muted, size = 24.dp)
            }
            AndroidView(
                factory = { ctx -> visualView(ctx, html, fontScale, onSize = {}, onError = {}, onSend = onSend) },
                modifier = Modifier.fillMaxSize(),
                onRelease = { it.destroy() },
            )
        }
    }
}
