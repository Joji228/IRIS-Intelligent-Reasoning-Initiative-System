package dev.iris.app.ui

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import dev.iris.app.R
import dev.iris.app.data.IrisViewModel
import dev.iris.app.link.SignIn
import dev.iris.app.link.linkHttp
import dev.iris.app.link.parsePairingLink
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Request

private enum class Mode { Code, Password }

@Composable
fun SignInScreen(vm: IrisViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var mode by rememberSaveable { mutableStateOf(Mode.Code) }
    var relay by rememberSaveable { mutableStateOf(vm.lastRelay) }
    var code by rememberSaveable { mutableStateOf("") }
    var user by rememberSaveable { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf(vm.defaultDeviceName) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf(vm.signinError) }

    fun run(block: suspend () -> Unit) {
        busy = true
        error = ""
        scope.launch {
            try {
                block()
            } catch (e: Exception) {
                error = e.message ?: "That didn't work"
            } finally {
                busy = false
            }
        }
    }

    /** A pairing link (from the QR or pasted): pair right away. */
    fun pairWith(text: String): Boolean {
        val link = parsePairingLink(text) ?: return false
        relay = link.relay
        code = link.code
        run { vm.signIn(link.relay, link.key, SignIn.Pair(link.code, name.trim())) }
        return true
    }

    fun scan() {
        val options = GmsBarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build()
        GmsBarcodeScanning.getClient(context, options).startScan()
            .addOnSuccessListener { b ->
                if (!pairWith(b.rawValue.orEmpty())) error = "That QR code isn't an I.R.I.S. pairing code."
            }
            .addOnFailureListener { error = "The scanner isn't available on this phone: paste the pairing link instead." }
    }

    fun paste() {
        val clip = (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip
        val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
        if (!pairWith(text)) error = "Copy the pairing link first (Settings → Remote access on the server)."
    }

    suspend fun engineKey(): String {
        val base = normalizeRelay(relay)
        return withContext(Dispatchers.IO) {
            val res = runCatching { linkHttp.newCall(Request.Builder().url("$base/link/key").build()).execute() }.getOrNull()
                ?: throw Exception("Can't reach the relay. Check the address and your internet connection.")
            res.use {
                val key = if (it.isSuccessful) {
                    runCatching { Json.parseToJsonElement(it.body.string()).jsonObject["key"]?.jsonPrimitive?.content }.getOrNull()
                } else null
                key ?: throw Exception("Your server hasn't connected to this relay yet. Turn on Remote access in I.R.I.S. settings on the server.")
            }
        }
    }

    Box(Modifier.fillMaxSize().background(C.bg).systemBarsPadding().imePadding(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 420.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 36.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Image(painterResource(R.drawable.iris_logo), null, Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)))
            Spacer(Modifier.height(18.dp))
            Txt("I.R.I.S.", 24.sp, C.text, FontWeight.SemiBold)
            Txt("Pair this phone to reach your agents from anywhere", 13.sp, C.faint, modifier = Modifier.padding(top = 4.dp))

            Spacer(Modifier.height(28.dp))
            Button("Scan the pairing QR code", ::scan, Modifier.fillMaxWidth(), Lucide.ScanLine, busy = busy, padding = PaddingValues14)
            Spacer(Modifier.height(10.dp))
            Button("Paste a pairing link", ::paste, Modifier.fillMaxWidth(), primary = false, enabled = !busy, padding = PaddingValues14)
            Txt(
                "On the server: Settings → Remote access → Pair a device.",
                12.sp, C.faint, modifier = Modifier.padding(top = 8.dp),
            )

            Spacer(Modifier.height(26.dp))
            Column(
                Modifier.fillMaxWidth().background(C.surface, RoundedCornerShape(18.dp)).border(1.dp, C.line, RoundedCornerShape(18.dp)).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(Modifier.fillMaxWidth().background(C.elevated, RoundedCornerShape(12.dp)).padding(4.dp)) {
                    for ((m, label, icon) in listOf(Triple(Mode.Code, "Pairing code", Lucide.QrCode), Triple(Mode.Password, "Password", Lucide.KeyRound))) {
                        val on = mode == m
                        Row(
                            Modifier.weight(1f).background(if (on) C.surface else C.elevated, RoundedCornerShape(9.dp))
                                .press { mode = m }.padding(vertical = 9.dp),
                            horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(icon, if (on) C.text else C.muted, 14.dp)
                            Txt(label, 13.sp, if (on) C.text else C.muted, if (on) FontWeight.Medium else FontWeight.Normal)
                        }
                    }
                }
                Column {
                    Label("Relay address")
                    Field(relay, { relay = it }, placeholder = "https://iris-you.workers.dev",
                        keyboard = KeyboardOptions(keyboardType = KeyboardType.Uri, autoCorrectEnabled = false))
                    Txt("Shown on the server in Settings → Remote access.", 12.sp, C.faint, modifier = Modifier.padding(top = 6.dp))
                }
                if (mode == Mode.Code) {
                    Column {
                        Label("Pairing code")
                        Field(code, { code = it.uppercase() }, placeholder = "ABCD2345", mono = true,
                            keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, autoCorrectEnabled = false))
                    }
                } else {
                    Column {
                        Label("Username")
                        Field(user, { user = it }, keyboard = KeyboardOptions(autoCorrectEnabled = false))
                    }
                    Column {
                        Label("Password")
                        Field(password, { password = it }, password = true, keyboard = KeyboardOptions(keyboardType = KeyboardType.Password))
                    }
                }
                Column {
                    Label("This device")
                    Field(name, { name = it })
                }
                if (error.isNotEmpty()) Txt(error, 13.sp, C.danger)
                // Says what's missing instead of greying the button out with no reason.
                val missing = when {
                    relay.isBlank() -> "Enter the relay address first."
                    mode == Mode.Code && code.trim().length < 6 -> "Enter the pairing code shown on the server."
                    mode == Mode.Password && user.isBlank() -> "Enter your username."
                    mode == Mode.Password && password.isEmpty() -> "Enter your password."
                    name.isBlank() -> "Give this device a name."
                    else -> null
                }
                Button(
                    if (mode == Mode.Code) "Pair" else "Sign in",
                    {
                        if (missing != null) error = missing
                        else run {
                            val base = normalizeRelay(relay)
                            val key = engineKey()
                            vm.signIn(
                                base, key,
                                if (mode == Mode.Code) SignIn.Pair(code.trim(), name.trim())
                                else SignIn.Login(user.trim(), password, name.trim()),
                            )
                        }
                    },
                    Modifier.fillMaxWidth(),
                    enabled = !busy,
                    busy = busy,
                    padding = PaddingValues14,
                )
            }
            Row(Modifier.padding(top = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Lucide.ShieldCheck, C.faint, 13.dp)
                Txt("End-to-end encrypted between this phone and your server.", 11.5.sp, C.faint)
            }
        }
    }
}

private val PaddingValues14 = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp, vertical = 13.dp)

/** The relay is always reached over https, even when the address is typed with http://. */
private fun normalizeRelay(s: String): String {
    val t = s.trim().trimEnd('/').removePrefix("https://").removePrefix("http://")
    return "https://$t"
}
