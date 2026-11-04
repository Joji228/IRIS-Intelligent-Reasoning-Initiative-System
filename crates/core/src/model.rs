use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    /// An emoji or short glyph shown in the UI.
    pub avatar: String,
    /// Accent color, e.g. `#8b5cf6`.
    pub color: String,
    pub system_prompt: String,
    /// Provider id and model. Empty (the normal case) means "use the default
    /// model chosen in Settings", so one choice applies to every agent.
    pub provider: String,
    pub model: String,
    /// How hard the model thinks (see [`REASONING_LEVELS`]). Empty, the normal
    /// case, follows the default chosen in Settings.
    pub reasoning_effort: String,
    /// Names of the tools this agent may use (see the engine's tool registry).
    pub tools: Vec<String>,
    /// `"main"` for the boss agent (the Chief of Staff the user talks to);
    /// empty for everyone else.
    pub role: String,
    /// The agent that created this one (empty if the user did).
    pub created_by: String,
    /// Auto mode: a safety check approves clearly safe commands instead of
    /// asking the user each time. Agents it creates follow it too.
    #[serde(default)]
    pub auto_approve: bool,
    /// With `auto_approve`: skip the review and run everything except what
    /// the hard rules stop ("Full auto").
    #[serde(default)]
    pub auto_full: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// At most this many agents (orbs) exist at once, the main one included.
pub const MAX_AGENTS: usize = 10;

/// Reasoning effort levels, from no thinking to the most. Empty means "the
/// provider's own default" and is handled by the caller, not listed here.
pub const REASONING_LEVELS: &[&str] = &["off", "low", "medium", "high", "max"];

/// True for an empty value (follow the default) or one of [`REASONING_LEVELS`].
pub fn valid_reasoning(level: &str) -> bool {
    level.is_empty() || REASONING_LEVELS.contains(&level)
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAgent {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub system_prompt: String,
    /// Leave empty to follow the default model.
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub created_by: String,
}

/// Soft pastel orb colors; each new agent gets the next one that's free.
pub const AGENT_COLORS: &[&str] = &[
    "#c4b5fd", // lavender (Iris)
    "#a7f3d0", // mint
    "#fde68a", // butter yellow
    "#fca5a5", // coral red
    "#bae6fd", // sky blue
    "#fed7aa", // peach
    "#fbcfe8", // pink
    "#e2e8f0", // pearl white
    "#99f6e4", // aqua
    "#d9f99d", // lime
];

/// The first palette color not in `used` (cycling once all are taken).
pub fn next_agent_color(used: &[String]) -> String {
    AGENT_COLORS
        .iter()
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
        .unwrap_or(&AGENT_COLORS[used.len() % AGENT_COLORS.len()])
        .to_string()
}

/// Partial update; only the fields that are present change.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    pub system_prompt: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub tools: Option<Vec<String>>,
    pub auto_approve: Option<bool>,
    pub auto_full: Option<bool>,
}

/// Something that went wrong while the user may not have been looking: a
/// failed reply or a failed routine. Stays in the problems inbox until it's
/// retried, dismissed, or (for a reply) the agent answers fine again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Problem {
    pub id: String,
    /// "reply" or "routine".
    pub kind: String,
    pub agent_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub routine_id: String,
    pub title: String,
    pub detail: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

/// An agent's latest chat line, for the chat list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatPreview {
    pub agent_id: String,
    pub role: String,
    /// The start of the message (at most 160 characters).
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Thread {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Compacted history: a summary of every message created at or before
    /// `summary_until` (ms), sent to the model instead of those messages.
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub summary_until: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    /// The reply is still being generated.
    Streaming,
    Complete,
    Error,
    /// Stopped by the user; `content` holds whatever arrived before the stop.
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub role: MessageRole,
    /// The reply's visible text (all rounds' text joined).
    pub content: String,
    /// Separate "thinking" text from reasoning models; empty if none.
    pub reasoning: String,
    /// How an assistant reply came together: each round is the model's
    /// thinking, text and tool calls before it continues. Empty for user
    /// messages and for replies that used no tools in older versions.
    pub rounds: Vec<Round>,
    pub status: MessageStatus,
    pub error: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    /// Tokens in the model's context at the end of the reply (last call's
    /// prompt + completion): how full the conversation is.
    pub context_tokens: Option<i64>,
    pub created_at: i64,
    /// Files the user attached (user messages only).
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// A message found by full-text search, with the matching words marked «so».
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub message_id: String,
    pub thread_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub role: String,
    pub created_at: i64,
    pub snippet: String,
}

/// Something an agent does on a schedule: send itself a task (`kind`
/// "prompt"), or run a PowerShell command without the AI ("command") and only
/// wake the agent when `notify` says so.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Routine {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    /// e.g. "every 30m", "daily 08:00", "weekdays 09:00", "weekly mon 09:00",
    /// "once 2026-11-02 08:00".
    pub schedule: String,
    pub kind: String,
    /// The task (prompt) or the command.
    pub body: String,
    /// For commands: "failure" (wake the agent when it fails) or "always".
    pub notify: String,
    pub enabled: bool,
    pub next_run: Option<i64>,
    pub last_run: Option<i64>,
    pub last_result: String,
    pub created_at: i64,
}

/// A file attached to a message, stored under `attachments/<thread>/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    /// "image", "text", "pdf" or "file".
    pub kind: String,
}

/// One model call within a reply.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Round {
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tools: Vec<ToolStep>,
}

/// A tool call the model made and what came of it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolStep {
    /// The model's id for this call (answered by a `tool` message).
    pub id: String,
    pub name: String,
    /// The raw JSON arguments the model wrote.
    pub arguments: String,
    pub status: ToolStatus,
    /// One line for people, e.g. `Searched the web for "rust async"`.
    #[serde(default)]
    pub summary: String,
    /// What the model was given back (possibly shortened).
    #[serde(default)]
    pub output: String,
    /// Set while the step waits for the user's approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Why auto mode ran it without asking, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approved: Option<String>,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    AwaitingApproval,
    Running,
    Done,
    Error,
    /// The user said no.
    Denied,
    /// The reply was stopped before the tool finished.
    Cancelled,
}

/// The final state of an assistant reply, written once streaming ends.
#[derive(Debug, Clone)]
pub struct MessageResult {
    pub content: String,
    pub reasoning: String,
    pub rounds: Vec<Round>,
    pub status: MessageStatus,
    pub error: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub context_tokens: Option<i64>,
}

/// One row of the tool audit log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolLogEntry {
    pub id: String,
    pub agent_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub tool: String,
    pub arguments: String,
    pub status: ToolStatus,
    pub summary: String,
    pub started_at: i64,
    pub duration_ms: i64,
}

/// A paired client (desktop app, phone, CLI). Only the token's hash is stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

macro_rules! text_enum {
    ($ty:ty { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $ty {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s { $($text => Ok(Self::$variant),)+ other => Err(format!("unknown value {other:?}")) }
            }
        }
    };
}

text_enum!(MessageRole { System => "system", User => "user", Assistant => "assistant" });
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_handed_out_in_order() {
        assert_eq!(next_agent_color(&[]), AGENT_COLORS[0]);
        let used = vec![AGENT_COLORS[0].to_uppercase(), AGENT_COLORS[2].to_string()];
        assert_eq!(next_agent_color(&used), AGENT_COLORS[1]);
        let all: Vec<String> = AGENT_COLORS.iter().map(|c| c.to_string()).collect();
        assert!(AGENT_COLORS.contains(&next_agent_color(&all).as_str()), "cycles when full");
    }
}

text_enum!(ToolStatus {
    AwaitingApproval => "awaiting_approval",
    Running => "running",
    Done => "done",
    Error => "error",
    Denied => "denied",
    Cancelled => "cancelled",
});
text_enum!(MessageStatus {
    Streaming => "streaming",
    Complete => "complete",
    Error => "error",
    Cancelled => "cancelled",
});
