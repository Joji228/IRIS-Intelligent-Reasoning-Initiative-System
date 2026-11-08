package dev.iris.app.data

import dev.iris.app.link.LinkConnection
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

class ApiException(val status: Int, message: String) : Exception(message)

/** The engine's API (docs/DEV.md), every call sealed and sent through I.R.I.S. Link. */
class IrisApi(val link: LinkConnection) {

    private suspend fun call(method: String, path: String, body: JsonElement? = null): JsonElement? {
        val res = link.request(method, path, body)
        if (res.status == 0) throw ApiException(0, "Can't reach the I.R.I.S. engine")
        val data = res.body?.takeIf { it.isNotEmpty() }?.let { runCatching { irisJson.parseToJsonElement(it) }.getOrNull() }
        if (res.status >= 400) {
            val error = (data as? JsonObject)?.get("error")?.jsonPrimitive?.content ?: res.body ?: "error ${res.status}"
            throw ApiException(res.status, error)
        }
        return data
    }

    private suspend inline fun <reified T> get(path: String): T = irisJson.decodeFromJsonElement(call("GET", path)!!)
    private suspend inline fun <reified T> send(method: String, path: String, body: JsonElement? = null): T =
        irisJson.decodeFromJsonElement(call(method, path, body)!!)

    suspend fun agents(): List<Agent> = get("/v1/agents")
    suspend fun status(): Status = get("/v1/status")
    suspend fun approvals(): List<PendingApproval> = get("/v1/approvals")
    suspend fun recentThreads(limit: Int = 30): List<Thread> = get("/v1/threads?limit=$limit")
    suspend fun providers(): List<Provider> = get("/v1/providers")
    suspend fun previews(): List<ChatPreview> = get("/v1/previews")

    // The problems inbox: failed replies and routines.
    suspend fun problems(): List<Problem> = get("/v1/problems")
    suspend fun retryProblem(id: String) { call("POST", "/v1/problems/$id/retry") }
    suspend fun dismissProblem(id: String) { call("POST", "/v1/problems/$id/dismiss") }
    suspend fun dismissAllProblems() { call("POST", "/v1/problems/dismiss") }
    suspend fun defaultModel(): DefaultModel? = call("GET", "/v1/settings/model")?.let { irisJson.decodeFromJsonElement(it) }
    suspend fun balance(providerId: String): Balance = get("/v1/providers/$providerId/balance")

    /** An agent's one chat (created on first use). */
    suspend fun chat(agentId: String): ThreadDetail = get("/v1/agents/$agentId/chat")
    suspend fun clearChat(agentId: String): ThreadDetail = send("POST", "/v1/agents/$agentId/chat/clear")
    suspend fun compactChat(agentId: String): ThreadDetail = send("POST", "/v1/agents/$agentId/chat/compact")

    suspend fun postMessage(threadId: String, content: String, attachments: List<Attachment> = emptyList()): Posted = send(
        "POST",
        "/v1/threads/$threadId/messages",
        buildJsonObject {
            put("content", content)
            putJsonArray("attachments") { attachments.forEach { add(irisJson.encodeToJsonElement(Attachment.serializer(), it)) } }
        },
    )

    /** Uploads one file to a chat; the message that carries it is posted after. */
    suspend fun uploadAttachment(threadId: String, upload: Upload): Attachment = send(
        "POST",
        "/v1/threads/$threadId/attachments",
        buildJsonObject {
            put("name", upload.name)
            put("data", dev.iris.app.link.LinkCrypto.b64(upload.bytes))
        },
    )

    suspend fun regenerate(threadId: String): Posted = send("POST", "/v1/threads/$threadId/regenerate")
    suspend fun stop(threadId: String) { call("POST", "/v1/threads/$threadId/stop") }
    suspend fun decide(approvalId: String, approve: Boolean) {
        call("POST", "/v1/approvals/$approvalId", buildJsonObject { put("approve", approve) })
    }
    suspend fun removeQueued(threadId: String, id: String) { call("DELETE", "/v1/threads/$threadId/queue/$id") }
    /** Sends a queued message next, stopping the reply in progress. */
    suspend fun steerQueued(threadId: String, id: String) { call("POST", "/v1/threads/$threadId/queue/$id/steer") }

    /** A blank orb; its first message decides what it becomes. */
    suspend fun summonAgent(): Agent = send("POST", "/v1/agents/blank")
    suspend fun deleteAgent(id: String) { call("DELETE", "/v1/agents/$id") }

    // ---- settings (the same calls as app/src/lib/api.ts) ---------------------------

    suspend fun models(providerId: String): List<String> = get("/v1/providers/$providerId/models")
    suspend fun putDefaultModel(provider: String, model: String): DefaultModel? =
        call("PUT", "/v1/settings/model", buildJsonObject { put("provider", provider); put("model", model) })
            ?.let { irisJson.decodeFromJsonElement(it) }
    suspend fun reasoning(): Reasoning = get("/v1/settings/reasoning")
    suspend fun putReasoning(effort: String): Reasoning =
        send("PUT", "/v1/settings/reasoning", buildJsonObject { put("effort", effort) })
    suspend fun autoDefault(): AutoDefault = get("/v1/settings/auto")
    suspend fun putAutoDefault(mode: String): AutoDefault = send("PUT", "/v1/settings/auto", buildJsonObject { put("mode", mode) })
    suspend fun fallback(): Fallback = get("/v1/settings/fallback")
    suspend fun putFallback(f: Fallback): Fallback = send(
        "PUT", "/v1/settings/fallback",
        buildJsonObject { put("enabled", f.enabled); put("provider", f.provider); put("model", f.model) },
    )
    suspend fun style(): ResponseStyle = get("/v1/settings/style")
    suspend fun putStyle(s: ResponseStyle): ResponseStyle =
        send("PUT", "/v1/settings/style", buildJsonObject { put("preset", s.preset); put("custom", s.custom) })
    suspend fun userMemory(): MemoryText = get("/v1/memory/user")
    suspend fun putUserMemory(text: String): MemoryText = send("PUT", "/v1/memory/user", buildJsonObject { put("text", text) })
    suspend fun agentMemory(agentId: String): MemoryText = get("/v1/agents/$agentId/memory")
    suspend fun putAgentMemory(agentId: String, text: String): MemoryText =
        send("PUT", "/v1/agents/$agentId/memory", buildJsonObject { put("text", text) })
    /** A partial update: only the fields in [patch] change (snake_case keys). */
    suspend fun patchAgent(agentId: String, patch: JsonObject): Agent = send("PATCH", "/v1/agents/$agentId", patch)
    suspend fun limits(): Limits = get("/v1/settings/limits")
    /** The newest published release (the server asks GitHub); [refresh] skips its cached answer. */
    suspend fun update(refresh: Boolean = false): UpdateStatus = get(if (refresh) "/v1/update?refresh=true" else "/v1/update")
    suspend fun putLimits(daily: Long, weekly: Long): Limits =
        send("PUT", "/v1/settings/limits", buildJsonObject { put("daily_tokens", daily); put("weekly_tokens", weekly) })
    suspend fun search(): SearchStatus = get("/v1/settings/search")
    /** [tavilyKey]: null keeps the saved key, "" removes it. */
    suspend fun putSearch(provider: String, tavilyKey: String?): SearchStatus = send(
        "PUT", "/v1/settings/search",
        buildJsonObject { put("provider", provider); if (tavilyKey != null) put("tavily_api_key", tavilyKey) },
    )
    suspend fun routines(): List<Routine> = get("/v1/routines")
    suspend fun createRoutine(r: JsonObject): Routine = send("POST", "/v1/routines", r)
    suspend fun editRoutine(id: String, r: JsonObject): Routine = send("PATCH", "/v1/routines/$id", r)
    suspend fun deleteRoutine(id: String) { call("DELETE", "/v1/routines/$id") }
    suspend fun runRoutine(id: String) { call("POST", "/v1/routines/$id/run") }
    suspend fun devices(): List<Device> = get("/v1/devices")
    suspend fun deleteDevice(id: String) { call("DELETE", "/v1/devices/$id") }
    suspend fun linkStatus(): LinkStatus = get("/v1/link")
    suspend fun newPairing(): Pairing = send("POST", "/v1/link/pair")
    suspend fun audit(limit: Int = 100): List<ToolLogEntry> = get("/v1/audit?limit=$limit")
    suspend fun skills(): List<Skill> = get("/v1/skills")
    suspend fun putSkill(name: String, description: String, body: String): Skill = send(
        "PUT", "/v1/skills/${java.net.URLEncoder.encode(name, "UTF-8").replace("+", "%20")}",
        buildJsonObject { put("description", description); put("body", body) },
    )
    suspend fun deleteSkill(name: String) { call("DELETE", "/v1/skills/${java.net.URLEncoder.encode(name, "UTF-8").replace("+", "%20")}") }
    suspend fun mcp(): List<McpServer> = get("/v1/mcp")
    /** Switches a connection on or off, keeping everything else as it is. */
    suspend fun setMcpEnabled(s: McpServer, enabled: Boolean): List<McpServer> = send(
        "PUT", "/v1/mcp/${java.net.URLEncoder.encode(s.name, "UTF-8").replace("+", "%20")}",
        buildJsonObject {
            put("command", s.command)
            putJsonArray("args") { s.args.forEach { add(kotlinx.serialization.json.JsonPrimitive(it)) } }
            put("url", s.url)
            put("enabled", enabled)
            putJsonArray("auto_approve") { s.autoApprove.forEach { add(kotlinx.serialization.json.JsonPrimitive(it)) } }
        },
    )
    suspend fun refreshMcp(): List<McpServer> = send("POST", "/v1/mcp/refresh")
    suspend fun vault(): List<VaultAccount> = get("/v1/vault")

    /** An attached file's bytes (images in user messages). */
    suspend fun attachment(threadId: String, id: String): ByteArray? {
        val res = link.request("GET", "/v1/attachments/$threadId/$id")
        if (res.status != 200) return null
        return res.b64?.let { dev.iris.app.link.LinkCrypto.unb64(it) } ?: res.body?.toByteArray()
    }
}

/** The event's `type` field. */
val JsonObject.type: String? get() = this["type"]?.jsonPrimitive?.content

inline fun <reified T> JsonObject.field(name: String): T? =
    this[name]?.let { runCatching { irisJson.decodeFromJsonElement<T>(it) }.getOrNull() }

fun JsonObject.str(name: String): String? = runCatching { this[name]?.jsonPrimitive?.content }.getOrNull()

fun JsonObject.obj(name: String): JsonObject? = runCatching { this[name]?.jsonObject }.getOrNull()
