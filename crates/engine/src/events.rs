//! Live events pushed to every connected client over `/v1/events`.

use iris_core::{Agent, Message, Round, Thread};
use serde::Serialize;

/// A tool call waiting for the user's decision.
#[derive(Debug, Clone, Serialize)]
pub struct PendingApproval {
    pub id: String,
    pub agent_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub tool: String,
    pub arguments: String,
    /// e.g. `Run: Get-ChildItem`.
    pub summary: String,
    pub created_at: i64,
    /// More to look at before deciding (a code change's diff).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    AgentUpdated { agent: Agent },
    AgentDeleted { agent_id: String },
    ThreadUpdated { thread: Thread },
    ThreadDeleted { thread_id: String, agent_id: String },
    MessageCreated { message: Message },
    MessageDeleted { thread_id: String, message_id: String },
    /// A piece of a reply that is still streaming, for round `round`.
    MessageDelta {
        thread_id: String,
        message_id: String,
        round: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    /// The reply's rounds changed structurally (a tool started, finished,
    /// needs approval, ...). Carries the full rounds so clients just replace.
    MessageProgress { thread_id: String, message_id: String, rounds: Vec<Round> },
    /// A reply reached its final state (complete, error or cancelled).
    MessageCompleted { message: Message },
    /// An agent started working on a reply (drives the "working" indicators).
    RunStarted { agent_id: String, thread_id: String, message_id: String },
    /// `status`: how the reply ended (complete, error, cancelled); drives the
    /// orbs' happy / sad faces.
    RunFinished { agent_id: String, thread_id: String, message_id: String, status: String },
    /// What an agent is doing right now: "thinking", "waiting", or a tool name,
    /// and the step in words (`detail`, empty while thinking).
    RunActivity { agent_id: String, thread_id: String, activity: String, detail: String },
    ApprovalRequested { approval: PendingApproval },
    /// The problems inbox changed; refetch `/v1/problems`.
    ProblemsChanged,
    ApprovalResolved { approval_id: String, agent_id: String, approved: bool },
    /// Providers were added, edited or removed; refetch `/v1/providers`.
    ProvidersChanged,
    /// A routine was added, changed, removed or ran.
    RoutinesChanged,
    /// The engine lost or regained its connection to the AI provider.
    Connectivity { online: bool },
    /// The engine connected to or lost the I.R.I.S. Link relay.
    LinkChanged { connected: bool },
    /// Messages waiting in a busy chat changed (the whole queue, in order).
    QueueChanged { thread_id: String, queue: Vec<crate::queue::Queued> },
    /// A phone or browser paired or signed in through the link.
    DeviceAdded { name: String },
}
