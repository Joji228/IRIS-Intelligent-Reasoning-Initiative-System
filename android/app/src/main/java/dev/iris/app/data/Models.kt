@file:OptIn(ExperimentalSerializationApi::class)

package dev.iris.app.data

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNamingStrategy

// Mirrors the engine's JSON (crates/core/src/model.rs) the same way app/src/lib/types.ts does.

val irisJson = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
    explicitNulls = false
    namingStrategy = JsonNamingStrategy.SnakeCase
}

@Serializable
data class Agent(
    val id: String,
    val name: String,
    val description: String = "",
    val avatar: String = "",
    val color: String = "",
    val systemPrompt: String = "",
    val provider: String = "",
    val model: String = "",
    val reasoningEffort: String = "",
    val tools: List<String> = emptyList(),
    /** "main" for Iris, "new" for a blank orb waiting for its first message. */
    val role: String = "",
    val createdBy: String = "",
    val autoApprove: Boolean = false,
    val autoFull: Boolean = false,
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
)

const val MAX_AGENTS = 10

@Serializable
data class ToolStep(
    val id: String,
    val name: String,
    val arguments: String = "",
    /** awaiting_approval, running, done, error, denied, cancelled */
    val status: String = "running",
    val summary: String = "",
    val output: String = "",
    val approvalId: String? = null,
    val autoApproved: String? = null,
    val startedAt: Long = 0,
    val finishedAt: Long? = null,
)

@Serializable
data class Round(
    val reasoning: String = "",
    val content: String = "",
    val tools: List<ToolStep> = emptyList(),
)

@Serializable
data class Attachment(
    val id: String,
    val name: String,
    val mime: String = "",
    val size: Long = 0,
    /** image, text, pdf, file */
    val kind: String = "file",
)

@Serializable
data class Message(
    val id: String,
    val threadId: String,
    /** system, user, assistant */
    val role: String,
    val content: String = "",
    val reasoning: String = "",
    val rounds: List<Round> = emptyList(),
    /** streaming, complete, error, cancelled */
    val status: String = "complete",
    val error: String? = null,
    val model: String? = null,
    val promptTokens: Long? = null,
    val completionTokens: Long? = null,
    val contextTokens: Long? = null,
    val createdAt: Long = 0,
    val attachments: List<Attachment> = emptyList(),
)

@Serializable
data class Thread(
    val id: String,
    val agentId: String,
    val title: String = "",
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
    val summary: String = "",
    val summaryUntil: Long = 0,
)

@Serializable
data class ThreadDetail(val thread: Thread, val messages: List<Message> = emptyList(), val running: Boolean = false)

@Serializable
data class QueuedMessage(
    val id: String,
    val content: String = "",
    val attachments: List<Attachment> = emptyList(),
    val createdAt: Long = 0,
)

@Serializable
data class Posted(
    val userMessage: Message? = null,
    /** Missing when the agent was busy and the message was queued. */
    val assistantMessage: Message? = null,
    val queued: QueuedMessage? = null,
)

@Serializable
data class PendingApproval(
    val id: String,
    val agentId: String,
    val threadId: String,
    val messageId: String = "",
    val tool: String = "",
    val arguments: String = "",
    val summary: String = "",
    val createdAt: Long = 0,
    /** More to review before deciding: a code change's diff. */
    val detail: String? = null,
)

@Serializable
data class RunInfo(
    val agentId: String,
    val threadId: String,
    val messageId: String = "",
    /** "thinking", "waiting", or the tool in use (e.g. "web_search"). */
    val activity: String? = null,
    /** That step in words ("Searching the web for “…”"); empty while thinking. */
    val detail: String = "",
)

/** A failed reply or routine in the problems inbox. */
@Serializable
data class Problem(
    val id: String,
    /** "reply" or "routine". */
    val kind: String = "reply",
    val agentId: String = "",
    val threadId: String = "",
    val messageId: String = "",
    val routineId: String = "",
    val title: String = "",
    val detail: String = "",
    val createdAt: Long = 0,
    val resolvedAt: Long? = null,
)

/** Which notifications this phone shows (kept on the phone). */
data class PushPrefs(val replies: Boolean = true, val questions: Boolean = true, val approvals: Boolean = true, val problems: Boolean = true)

/** An agent's latest chat line, for the chat list. */
@Serializable
data class ChatPreview(
    val agentId: String,
    val role: String = "assistant",
    val text: String = "",
    val createdAt: Long = 0,
)

@Serializable
data class Status(
    val running: List<RunInfo> = emptyList(),
    val online: Boolean = true,
    val queues: Map<String, List<QueuedMessage>> = emptyMap(),
)

@Serializable
data class Provider(
    val id: String,
    val name: String = "",
    val baseUrl: String = "",
    val defaultModel: String = "",
    val hasKey: Boolean = false,
    /** inline, env, file, none */
    val keySource: String = "none",
)

@Serializable
data class DefaultModel(
    val provider: String = "",
    val providerName: String = "",
    val model: String = "",
    val contextWindow: Long? = null,
)

/** Remaining API credit (DeepSeek); amounts are strings as the provider sends them. */
@Serializable
data class Balance(val currency: String = "", val total: String = "", val available: Boolean = false)

// ---- settings ---------------------------------------------------------------------

@Serializable
data class Fallback(val enabled: Boolean = false, val provider: String = "", val model: String = "")

@Serializable
data class Reasoning(val effort: String = "")

@Serializable
data class AutoDefault(val mode: String = "off")

/** How agents talk to the user: a preset plus the user's own words. */
@Serializable
data class ResponseStyle(val preset: String = "balanced", val custom: String = "")

/** A memory file's text (MEMORY.md / USER.md) and its size cap. */
@Serializable
data class MemoryText(val text: String = "", val limit: Int = 2000)

@Serializable
data class Limits(
    val dailyTokens: Long = 0,
    val weeklyTokens: Long = 0,
    val usedToday: Long = 0,
    val usedThisWeek: Long = 0,
)

@Serializable
data class SearchStatus(val provider: String = "auto", val hasTavilyKey: Boolean = false, val active: String = "duckduckgo")

@Serializable
data class Routine(
    val id: String,
    val agentId: String,
    val name: String = "",
    val schedule: String = "",
    /** prompt, command, heartbeat */
    val kind: String = "prompt",
    val body: String = "",
    /** failure, always */
    val notify: String = "failure",
    val enabled: Boolean = true,
    val nextRun: Long? = null,
    val lastRun: Long? = null,
    val lastResult: String = "",
    val createdAt: Long = 0,
)

@Serializable
data class Device(
    val id: String,
    val name: String = "",
    val createdAt: Long = 0,
    val lastSeenAt: Long? = null,
    /** The I.R.I.S. app on the server itself (can't be revoked). */
    val local: Boolean = false,
)

@Serializable
data class LinkStatus(
    val enabled: Boolean = false,
    val url: String = "",
    val connected: Boolean = false,
    val hasRelayKey: Boolean = false,
    val publicKey: String = "",
    val loginUser: String? = null,
)

@Serializable
data class Pairing(val code: String, val expiresAt: Long = 0, val url: String = "")

@Serializable
data class ToolLogEntry(
    val id: String,
    val agentId: String = "",
    val tool: String = "",
    val arguments: String = "",
    val status: String = "",
    val summary: String = "",
    val startedAt: Long = 0,
    val durationMs: Long = 0,
)

@Serializable
data class Skill(val name: String, val description: String = "", val body: String = "")

@Serializable
data class McpTool(val name: String, val description: String = "", val readOnly: Boolean = false)

@Serializable
data class McpServer(
    val name: String,
    val transport: String = "stdio",
    val command: String = "",
    val args: List<String> = emptyList(),
    val url: String = "",
    val enabled: Boolean = true,
    val autoApprove: List<String> = emptyList(),
    val connected: Boolean = false,
    val error: String? = null,
    val tools: List<McpTool> = emptyList(),
)

/** A saved login for agents; the password never comes back from the engine. */
@Serializable
data class VaultAccount(
    val id: String,
    val name: String = "",
    val site: String = "",
    val username: String = "",
    val hasPassword: Boolean = false,
    val notes: String = "",
)

/** Reasoning levels, least to most thinking; "" follows the default. */
val REASONING_LEVELS = listOf("off" to "Off", "low" to "Low", "medium" to "Medium", "high" to "High", "max" to "Max")

fun reasoningLabel(id: String) = if (id.isEmpty()) "Auto" else REASONING_LEVELS.firstOrNull { it.first == id }?.second ?: id

/** Friendly names for model ids that hide their version (app/src/lib/format.ts). */
private val MODEL_VERSIONS = mapOf("deepseek-flash" to "DeepSeek V4.1 Flash", "deepseek-v4-pro" to "DeepSeek V4 Pro")

fun modelName(id: String): String = MODEL_VERSIONS[id] ?: id

/** Soft pastel orb colors (the engine's AGENT_COLORS). */
val AGENT_COLORS = listOf(
    "#c4b5fd", "#a7f3d0", "#fde68a", "#fca5a5", "#bae6fd", "#fed7aa", "#fbcfe8", "#e2e8f0", "#99f6e4", "#d9f99d",
)

/** The abilities that can be switched on per agent (types.ts ABILITY_GROUPS); Iris has all. */
data class AbilityGroup(val id: String, val name: String, val description: String, val tools: List<String>, val asks: Boolean)

val ABILITY_GROUPS = listOf(
    AbilityGroup("files", "Files", "Read and write files in its own folder on the server.", listOf("files"), false),
    AbilityGroup("computer", "Computer control", "Run PowerShell commands, check updates, install or remove apps, restart.", listOf("shell", "system"), true),
    AbilityGroup("code", "Develop I.R.I.S.", "Read and change the app's own code. Every change shows you the diff and asks first.", listOf("dev"), true),
    AbilityGroup("connections", "Connections", "Tools from services connected in Settings → Connections (MCP servers).", listOf("mcp"), true),
)

const val BUILT_IN_ABILITIES =
    "web search and pages, a browser, date and time, routines, the team (talking to other agents and Iris), models and settings"

/** 1234 → "1.2k", 1_500_000 → "1.5M". */
fun formatTokens(n: Long): String = when {
    n >= 1_000_000 -> "%.2f".format(n / 1_000_000.0).trimEnd('0').trimEnd('.') + "M"
    n >= 1_000 -> "%.1f".format(n / 1_000.0).trimEnd('0').trimEnd('.') + "k"
    else -> n.toString()
}
