package dev.iris.app.link

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** What this phone remembers about its pairing. */
@Serializable
data class LinkCredentials(
    /** The relay's address (https://…). */
    val relay: String,
    /** The engine's public key (base64url), pinned at pairing. */
    val key: String,
    /** This device's token. */
    val token: String,
    val device: String,
)

data class LinkResponse(val status: Int, val body: String? = null, val b64: String? = null, val ct: String? = null)

enum class LinkState { Connecting, Online, ServerOffline, Offline }

sealed interface SignIn {
    data class Auth(val token: String) : SignIn
    data class Pair(val code: String, val name: String) : SignIn
    data class Login(val user: String, val password: String, val name: String) : SignIn
}

class LinkException(val kind: Kind, message: String) : Exception(message) {
    enum class Kind { RelayUnreachable, ServerOffline, Refused }
}

private val json = Json { ignoreUnknownKeys = true }

/** One sealed session (a fresh one after every reconnect). */
class LinkSession private constructor(
    private val ws: WebSocket,
    private val sendKey: ByteArray,
    private val recvKey: ByteArray,
) {
    private var sent = 0L
    private var received = 0L
    private val waiters = ConcurrentHashMap<Int, CompletableDeferred<LinkResponse>>()
    private val nextId = AtomicInteger()
    @Volatile private var pendingSignIn: CompletableDeferred<JsonObject>? = null
    @Volatile var lastPong = System.currentTimeMillis()
    @Volatile var onEvent: (JsonObject) -> Unit = {}

    /** Completes when the socket is gone. */
    val closed = CompletableDeferred<Unit>()
    val isOpen get() = !closed.isCompleted

    /** Frames are sealed one at a time, so counters go out in order. */
    @Synchronized
    fun send(msg: JsonObject) {
        val n = ++sent
        val sealed = LinkCrypto.seal(sendKey, n, "c2s", msg.toString().toByteArray())
        ws.send(buildJsonObject { put("s", n); put("b", LinkCrypto.b64(sealed)) }.toString())
    }

    /** OkHttp delivers messages one after another, so counters are checked in arrival order. */
    private fun receive(frame: String) {
        val msg = try {
            val f = json.parseToJsonElement(frame).jsonObject
            val n = f["s"]?.jsonPrimitive?.long ?: return
            if (n <= received) return
            val plain = LinkCrypto.open(recvKey, n, "s2c", LinkCrypto.unb64(f["b"]!!.jsonPrimitive.content))
            received = n
            json.parseToJsonElement(plain.decodeToString()).jsonObject
        } catch (_: Exception) {
            close()
            return
        }
        when (msg["t"]?.jsonPrimitive?.content) {
            "res" -> {
                val id = msg["id"]?.jsonPrimitive?.int ?: return
                waiters.remove(id)?.complete(
                    LinkResponse(
                        status = msg["status"]?.jsonPrimitive?.int ?: 0,
                        body = msg["body"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content,
                        b64 = msg["b64"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content,
                        ct = msg["ct"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content,
                    ),
                )
            }
            "ev" -> msg["e"]?.jsonObject?.let { onEvent(it) }
            "pong" -> lastPong = System.currentTimeMillis()
            // This device was removed on the server: hang up now. The reconnect signs in
            // again, is refused, and sends the app back to its sign-in screen with the reason.
            "revoked" -> close()
            "signed_in", "error" -> pendingSignIn?.complete(msg).also { pendingSignIn = null }
        }
    }

    /** Signs in; returns `signed_in` (with `device` and maybe `token`) or `error`. */
    suspend fun signIn(msg: SignIn): JsonObject {
        val waiter = CompletableDeferred<JsonObject>()
        pendingSignIn = waiter
        send(
            buildJsonObject {
                when (msg) {
                    is SignIn.Auth -> { put("t", "auth"); put("token", msg.token) }
                    is SignIn.Pair -> { put("t", "pair"); put("code", msg.code); put("name", msg.name) }
                    is SignIn.Login -> {
                        put("t", "login"); put("user", msg.user); put("password", msg.password); put("name", msg.name)
                    }
                }
            },
        )
        return withTimeoutOrNull(20_000) { waiter.await() }
            ?: buildJsonObject { put("t", "error"); put("error", "connection lost") }
    }

    suspend fun request(method: String, path: String, body: JsonElement?): LinkResponse {
        val id = nextId.incrementAndGet()
        val waiter = CompletableDeferred<LinkResponse>()
        waiters[id] = waiter
        send(
            buildJsonObject {
                put("t", "req"); put("id", id); put("method", method); put("path", path)
                put("body", body ?: JsonNull)
            },
        )
        return withTimeoutOrNull(150_000) { waiter.await() }
            ?: LinkResponse(0, "no answer").also { waiters.remove(id) }
    }

    fun close() {
        ws.close(1000, null)
        abort()
    }

    /** Fails everything still waiting (the connection dropped). */
    private fun abort() {
        closed.complete(Unit)
        for (w in waiters.values) w.complete(LinkResponse(0, "connection lost"))
        waiters.clear()
        pendingSignIn?.complete(buildJsonObject { put("t", "error"); put("error", "connection lost") })
        pendingSignIn = null
    }

    companion object {
        /** Opens the socket and does the handshake with the engine `key`. */
        suspend fun open(
            http: OkHttpClient,
            relay: String,
            key: String,
            onRelay: (String) -> Unit,
        ): LinkSession {
            val opened = CompletableDeferred<Unit>()
            val first = CompletableDeferred<String>()
            var session: LinkSession? = null
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    opened.complete(Unit)
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (text.startsWith("{\"relay\"")) {
                        val r = runCatching { json.parseToJsonElement(text).jsonObject["relay"]!!.jsonPrimitive.content }
                            .getOrNull() ?: return
                        onRelay(r)
                        if (r == "offline") first.completeExceptionally(LinkException(LinkException.Kind.ServerOffline, "server-offline"))
                        return
                    }
                    val s = session
                    if (s != null) s.receive(text) else first.complete(text)
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    webSocket.close(1000, null)
                    gone()
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = gone()

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = gone()

                private fun gone() {
                    val err = LinkException(LinkException.Kind.RelayUnreachable, "offline")
                    opened.completeExceptionally(err)
                    first.completeExceptionally(err)
                    session?.abort()
                }
            }
            val ws = http.newWebSocket(Request.Builder().url("${relay.trimEnd('/')}/link/client").build(), listener)
            try {
                withTimeout(15_000) { opened.await() }
                val mine = LinkCrypto.newKeyPair()
                val myPub = LinkCrypto.rawPublic(mine.public)
                ws.send(buildJsonObject { put("t", "hello"); put("v", 1); put("epk", LinkCrypto.b64url(myPub)) }.toString())
                val welcome = withTimeoutOrNull(10_000) { first.await() }
                    ?: throw LinkException(LinkException.Kind.ServerOffline, "server-offline")
                val w = json.parseToJsonElement(welcome).jsonObject
                if (w["t"]?.jsonPrimitive?.content != "welcome") throw IllegalStateException("unexpected reply from the engine")
                val eph = LinkCrypto.unb64(w["epk"]!!.jsonPrimitive.content)
                val ikm = LinkCrypto.ecdh(mine.private, LinkCrypto.publicFromRaw(LinkCrypto.unb64(key))) +
                    LinkCrypto.ecdh(mine.private, LinkCrypto.publicFromRaw(eph))
                val okm = LinkCrypto.hkdf(ikm, "iris-link-v1".toByteArray(), myPub + eph, 64)
                return LinkSession(ws, okm.copyOfRange(0, 32), okm.copyOfRange(32, 64)).also { session = it }
            } catch (e: Exception) {
                ws.cancel()
                throw if (e is kotlinx.coroutines.TimeoutCancellationException) {
                    LinkException(LinkException.Kind.RelayUnreachable, "offline")
                } else e
            }
        }
    }
}

val linkHttp: OkHttpClient by lazy {
    OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()
}

/** Opens a session and signs in once (pairing, password); returns what to remember. */
suspend fun signInOnce(relay: String, key: String, msg: SignIn): LinkCredentials {
    var serverAway = false
    val session = try {
        LinkSession.open(linkHttp, relay, key) { if (it == "offline") serverAway = true }
    } catch (e: LinkException) {
        throw LinkException(
            e.kind,
            if (e.kind == LinkException.Kind.ServerOffline || serverAway) {
                "Your server isn't connected right now. Check that it's on and Remote access is turned on."
            } else {
                "Can't reach the relay. Check your internet connection."
            },
        )
    }
    try {
        val res = session.signIn(msg)
        if (res["t"]?.jsonPrimitive?.content == "error") {
            throw LinkException(LinkException.Kind.Refused, res["error"]?.jsonPrimitive?.content ?: "refused")
        }
        val token = res["token"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content
            ?: (msg as? SignIn.Auth)?.token.orEmpty()
        return LinkCredentials(relay, key, token, res["device"]?.jsonPrimitive?.content.orEmpty())
    } finally {
        session.close()
    }
}

/** One request on a short-lived session: the push service uses it while the app is closed. */
suspend fun requestOnce(creds: LinkCredentials, method: String, path: String, body: JsonElement? = null): LinkResponse {
    val session = LinkSession.open(linkHttp, creds.relay, creds.key) {}
    try {
        val res = session.signIn(SignIn.Auth(creds.token))
        if (res["t"]?.jsonPrimitive?.content == "error") {
            throw LinkException(LinkException.Kind.Refused, res["error"]?.jsonPrimitive?.content ?: "refused")
        }
        return session.request(method, path, body)
    } finally {
        session.close()
    }
}

/**
 * A lasting connection: reconnects by itself (new handshake, sign-in with the token),
 * and calls [onOpen] after every (re)connect so the app can refetch.
 */
class LinkConnection(
    val creds: LinkCredentials,
    private val scope: CoroutineScope,
    private val onEvent: (JsonObject) -> Unit,
    private val onOpen: () -> Unit,
    /** The token was refused (device removed): sign in again. */
    private val onSignedOut: (String) -> Unit,
    /** Keep-alive interval: 20 s on screen; the background service pings less often. */
    private val pingEvery: Long = 20_000,
) {
    private val _state = MutableStateFlow(LinkState.Connecting)
    val state: StateFlow<LinkState> = _state

    @Volatile private var session: LinkSession? = null
    @Volatile private var closed = false
    private var loop: Job? = null
    private val nudges = Channel<Unit>(Channel.CONFLATED)

    fun start() {
        if (loop?.isActive == true || closed) return
        loop = scope.launch(Dispatchers.IO) { run() }
    }

    private suspend fun run() {
        var backoff = 1000L
        while (scope.isActive && !closed) {
            if (_state.value == LinkState.Online) _state.value = LinkState.Connecting
            var quick = false
            try {
                val s = LinkSession.open(linkHttp, creds.relay, creds.key) { r ->
                    if (r == "offline") {
                        _state.value = LinkState.ServerOffline
                    } else if (r == "online") {
                        // The server reconnected to the relay: its old sessions are gone.
                        quick = true
                        session?.close()
                    }
                }
                val res = s.signIn(SignIn.Auth(creds.token))
                if (res["t"]?.jsonPrimitive?.content == "error") {
                    s.close()
                    val error = res["error"]?.jsonPrimitive?.content ?: "signed out"
                    if (error != "connection lost") {
                        closed = true
                        onSignedOut(error)
                        return
                    }
                    throw LinkException(LinkException.Kind.RelayUnreachable, error)
                }
                s.onEvent = onEvent
                session = s
                backoff = 1000L
                _state.value = LinkState.Online
                onOpen()
                // Keep-alive, and a way to notice a session the engine has forgotten.
                val pinger = scope.launch(Dispatchers.IO) {
                    while (isActive) {
                        delay(pingEvery)
                        if (System.currentTimeMillis() - s.lastPong > pingEvery * 5 / 2) s.close()
                        else runCatching { s.send(buildJsonObject { put("t", "ping") }) }
                    }
                }
                s.closed.await()
                pinger.cancel()
            } catch (e: LinkException) {
                _state.value = if (e.kind == LinkException.Kind.ServerOffline) LinkState.ServerOffline else LinkState.Offline
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (_: Exception) {
                _state.value = LinkState.Offline
            }
            session = null
            if (closed) break
            if (_state.value == LinkState.Online) _state.value = LinkState.Connecting
            withTimeoutOrNull(if (quick) 200 else backoff) { nudges.receive() }
            backoff = minOf(backoff * 2, 15_000)
        }
    }

    /** Reconnects now (e.g. the app came back to the front). */
    fun nudge() {
        if (closed) return
        if (session?.isOpen != true) nudges.trySend(Unit)
        start()
    }

    /** A request; while the link is reconnecting it waits (up to 20 s) for it. */
    suspend fun request(method: String, path: String, body: JsonElement? = null): LinkResponse {
        val deadline = System.currentTimeMillis() + 20_000
        while (true) {
            if (closed) return LinkResponse(0, "signed out")
            val s = session
            if (_state.value == LinkState.Online && s != null && s.isOpen) {
                val res = s.request(method, path, body)
                // This device was revoked: reconnecting runs the sign-in again, which
                // fails and sends the app back to its sign-in screen.
                if (res.status == 401) s.close()
                // Lost mid-request: a read is retried once the link is back (a write may
                // already have happened, so it isn't).
                if (res.status != 0 || method != "GET" || System.currentTimeMillis() > deadline) return res
            } else if (System.currentTimeMillis() > deadline) {
                return LinkResponse(0, "Can't reach the I.R.I.S. engine")
            }
            start()
            withTimeoutOrNull(maxOf(0, deadline - System.currentTimeMillis())) {
                _state.first { it == LinkState.Online }
                delay(50)
            }
        }
    }

    fun stop() {
        closed = true
        loop?.cancel()
        session?.close()
    }
}

/** The pairing link in the server's QR: `https://relay/#pair=CODE&k=KEY`. */
data class PairingLink(val relay: String, val code: String, val key: String)

fun parsePairingLink(text: String): PairingLink? {
    val t = text.trim()
    val hash = t.indexOf('#')
    if (!t.startsWith("http") || hash < 0) return null
    val params = t.substring(hash + 1).split('&').mapNotNull {
        val i = it.indexOf('=')
        if (i > 0) it.substring(0, i) to java.net.URLDecoder.decode(it.substring(i + 1), "UTF-8") else null
    }.toMap()
    val code = params["pair"] ?: return null
    val key = params["k"] ?: return null
    // Always over https, whatever the link says.
    val relay = "https://" + t.substring(0, hash).trimEnd('/').removePrefix("https://").removePrefix("http://")
    return PairingLink(relay, code, key)
}
