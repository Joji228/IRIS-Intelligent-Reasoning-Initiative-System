package dev.iris.app.data

/**
 * Turning a streamed reply into things worth saying aloud (docs/VOICE-PLAN.md, Phase 3).
 * Pure Kotlin, no Android classes, so it is tested on the JVM.
 */

/** Markdown → plain text worth hearing: no code, URLs or markup. Same rules as `speakable()` in the desktop app. */
fun speakable(md: String): String = md
    .replace(Regex("```[\\s\\S]*?```"), " (code) ")
    .replace(Regex("`([^`]+)`"), "$1")
    .replace(Regex("!\\[([^\\]]*)]\\([^)]*\\)"), "$1")
    .replace(Regex("\\[([^\\]]+)]\\([^)]*\\)"), "$1")
    .replace(Regex("https?://\\S+"), "")
    .replace(Regex("^\\s{0,3}(#{1,6}|>|[-*+]|\\d+[.)])\\s+", RegexOption.MULTILINE), "")
    .replace(Regex("[*_~|]+"), "")
    .replace(Regex("\\[\\d+]"), "")
    .replace(Regex("\\s+"), " ")
    .trim()

/**
 * The language to speak a sentence in: Greek if it has any Greek letter, otherwise English.
 * Greek replies often carry brand names ("Άνοιξα το Gmail"), which the Greek voice reads
 * fine; the other way round is rare.
 */
fun languageOf(sentence: String): String =
    if (sentence.any { it in 'Ͱ'..'Ͽ' || it in 'ἀ'..'῿' }) "el" else "en"

/**
 * Feeds on the `message_delta` content of a reply, a few characters at a time, and hands back
 * sentences as soon as they are complete, already cleaned up for speaking.
 *
 * - A sentence ends at `.` `!` `?` `…`, the Greek question mark `;`, the Greek ano teleia `·`
 *   or a line break, when a space (or the end of the reply) follows. So `3.5`, `example.com`
 *   and `e.g.` never split.
 * - A fenced code block is not read (it becomes the word "code").
 * - A line that is exactly `[details]` ends what is spoken: nothing after it is read aloud.
 *   (The app still shows that part in the chat.)
 *
 * Only feed it the reply's text, never its "thinking".
 */
class SpeechStream {
    /** Text not yet looked at: a line start may still turn out to be a code fence or `[details]`. */
    private val pending = StringBuilder()

    /** Text that passed the first step and waits for a sentence end. */
    private val text = StringBuilder()
    private var atLineStart = true
    private var inFence = false

    /** True once a `[details]` line was seen: everything after it stays silent. */
    var stopped = false
        private set

    /** More of the reply arrived; returns the sentences it completed (maybe none). */
    fun push(delta: String): List<String> {
        if (stopped || delta.isEmpty()) return emptyList()
        pending.append(delta)
        route(final = false)
        return sentences(final = false)
    }

    /**
     * The end of a reply, or of one round of it (before a tool runs): says whatever is left, even
     * without a full stop. A `[details]` line stays in force.
     */
    fun flush(): List<String> {
        if (!stopped) route(final = true)
        val out = sentences(final = true)
        atLineStart = true
        inFence = false
        pending.clear()
        return out
    }

    // ---- step 1: code fences and the [details] line (they need to see a whole line start) ----

    private fun route(final: Boolean) {
        while (pending.isNotEmpty() && !stopped) {
            if (atLineStart) {
                if (pending[0] == '\n') { // an empty line: still a sentence break
                    text.append('\n')
                    pending.deleteCharAt(0)
                    continue
                }
                val all = pending.toString()
                val trimmed = all.trimStart(' ', '\t', '\r')
                if (trimmed.isEmpty()) { // only spaces so far: wait for something to look at
                    if (final) pending.clear()
                    return
                }
                val lineEnd = trimmed.indexOf('\n')
                val line = (if (lineEnd >= 0) trimmed.substring(0, lineEnd) else trimmed).trimEnd()
                val complete = lineEnd >= 0 || final
                val used = all.length - trimmed.length + (if (lineEnd >= 0) lineEnd + 1 else trimmed.length)
                if (inFence) {
                    if (!complete) return // the code is thrown away, but a closing fence needs the whole line
                    if (line.startsWith("```")) inFence = false
                    pending.delete(0, used)
                    continue
                }
                if (line.startsWith("```")) {
                    if (!complete) return
                    // ```like this``` on one line is over at once; otherwise the block runs until a closing fence.
                    if (line.indexOf("```", 3) < 0) inFence = true
                    text.append("(code)\n")
                    pending.delete(0, used)
                    continue
                }
                if (line.startsWith(DETAILS)) {
                    if (!complete) return
                    if (line.removePrefix(DETAILS).isBlank()) {
                        stopped = true
                        pending.clear()
                        return
                    }
                } else if (!complete && (DETAILS.startsWith(line) || "```".startsWith(line))) {
                    return // "[det" or "``" might still become one of them
                }
                atLineStart = false
            }
            val nl = pending.indexOf("\n")
            if (nl >= 0) {
                text.append(pending, 0, nl + 1)
                pending.delete(0, nl + 1)
                atLineStart = true
            } else {
                text.append(pending)
                pending.clear()
            }
        }
    }

    // ---- step 2: cut the text into sentences ----

    private fun sentences(final: Boolean): List<String> {
        val out = ArrayList<String>()
        var start = 0
        var i = 0
        scan@ while (i < text.length) {
            val c = text[i]
            if (c == '\n') {
                add(out, text.substring(start, i))
                start = i + 1
                i++
                continue
            }
            if (c !in ENDINGS) {
                i++
                continue
            }
            // A run of ending marks ("?!", "...") and then any closing quotes or brackets.
            var j = i
            while (j + 1 < text.length && text[j + 1] in ENDINGS) j++
            var k = j + 1
            while (k < text.length && text[k] in CLOSERS) k++
            if (k >= text.length) { // can't tell what follows yet
                if (final) {
                    add(out, text.substring(start, k))
                    start = k
                }
                break@scan
            }
            if (!text[k].isWhitespace() || !endsASentence(text.substring(start, i), c, j == i)) {
                i = j + 1 // "3.5", "example.com", "e.g. ", "1. "
                continue
            }
            add(out, text.substring(start, k))
            start = k
            i = k
        }
        if (final && start < text.length) {
            add(out, text.substring(start))
            start = text.length
        }
        text.delete(0, start)
        return out
    }

    /** False for an abbreviation ("Dr.", "e.g.", "π.χ.") or a list number ("1.") before the mark. */
    private fun endsASentence(before: String, mark: Char, single: Boolean): Boolean {
        if (mark != '.' || !single) return true
        if (before.trim().let { it.isNotEmpty() && it.length <= 3 && it.all(Char::isDigit) }) return false
        val word = Regex("[\\p{L}.]+$").find(before)?.value?.lowercase() ?: return true
        return word !in ABBREVIATIONS
    }

    private fun add(out: MutableList<String>, raw: String) {
        val say = speakable(raw)
        if (say.none { it.isLetterOrDigit() }) return
        // Calls are English and Greek only; another script would be read out as gibberish.
        if (hasForeignScript(say)) return
        // Engines stop at a few thousand characters; very long sentences are cut at a comma or space.
        var rest = say
        while (rest.length > MAX_PIECE) {
            val cut = (rest.lastIndexOf(", ", MAX_PIECE).takeIf { it > 40 } ?: rest.lastIndexOf(' ', MAX_PIECE).takeIf { it > 40 } ?: MAX_PIECE)
            out.add(rest.substring(0, cut + 1).trim())
            rest = rest.substring(cut + 1).trim()
        }
        if (rest.any { it.isLetterOrDigit() }) out.add(rest)
    }

    private companion object {
        const val DETAILS = "[details]"
        const val ENDINGS = ".!?;·…"
        const val CLOSERS = "\"')]»”’"
        const val MAX_PIECE = 300
        val ABBREVIATIONS = setOf(
            "dr", "mr", "mrs", "ms", "prof", "sr", "jr", "st", "vs", "inc", "ltd", "co", "fig", "al", "approx", "e.g", "i.e",
            "κ", "κα", "π.χ", "δηλ", "αρ", "σελ", "κ.α",
        )
    }
}
