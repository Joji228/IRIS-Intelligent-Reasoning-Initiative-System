package dev.iris.app.data

/**
 * Voice calls: an agent's reply may have a line that is exactly `[details]`. Everything
 * before it is said aloud; everything after it is only shown in the chat (code, tables,
 * links, long reports). Matches the engine's voice instructions in chat.rs.
 */
data class ReplyParts(val spoken: String, val details: String?)

private val DETAILS_LINE = Regex("^[ \\t]*\\[details][ \\t]*$", RegexOption.MULTILINE)

fun splitDetails(text: String): ReplyParts {
    val m = DETAILS_LINE.find(text) ?: return ReplyParts(text, null)
    return ReplyParts(text.substring(0, m.range.first).trimEnd(), text.substring(m.range.last + 1).trimStart())
}
