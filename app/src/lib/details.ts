// Voice calls: an agent's reply may have a line that is exactly "[details]".
// Everything before it is what gets said aloud; everything after it is only
// shown in the chat (code, tables, links, long reports). Matches the engine's
// voice instructions in chat.rs.

const DETAILS_LINE = /^[ \t]*\[details\][ \t]*$/m;

/** Splits a reply at its `[details]` line. `details` is null when there is none. */
export function splitDetails(text: string): { spoken: string; details: string | null } {
  const m = DETAILS_LINE.exec(text);
  if (!m) return { spoken: text, details: null };
  return { spoken: text.slice(0, m.index).trimEnd(), details: text.slice(m.index + m[0].length).trimStart() };
}
