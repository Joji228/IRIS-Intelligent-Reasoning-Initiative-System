//! The problems inbox: replies that failed and routines that failed, in one
//! place with a Retry button, so trouble that happened while the user was
//! away doesn't hide inside separate chats. A failed reply's problem clears
//! itself once the agent answers fine in that chat again.

use iris_core::{Message, MessageRole, MessageStatus, Problem};
use tracing::warn;

use crate::{api::AppState, events::Event};

/// Records a problem and tells the clients.
#[allow(clippy::too_many_arguments)]
pub fn record(st: &AppState, kind: &str, agent_id: &str, thread_id: &str, message_id: &str, routine_id: &str, title: String, detail: String) {
    let p = Problem {
        id: iris_core::new_id(),
        kind: kind.to_owned(),
        agent_id: agent_id.to_owned(),
        thread_id: thread_id.to_owned(),
        message_id: message_id.to_owned(),
        routine_id: routine_id.to_owned(),
        title,
        detail,
        created_at: iris_core::now_ms(),
        resolved_at: None,
    };
    if let Err(e) = st.store.add_problem(&p) {
        warn!("couldn't save a problem: {e}");
        return;
    }
    st.emit(Event::ProblemsChanged);
}

/// Watches replies: a failure becomes a problem, a good answer clears that
/// chat's earlier ones.
pub async fn run_forever(st: AppState) {
    let mut events = st.events.subscribe();
    loop {
        match events.recv().await {
            Ok(Event::MessageCompleted { message }) if message.role == MessageRole::Assistant => after_reply(&st, &message),
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

fn after_reply(st: &AppState, m: &Message) {
    match m.status {
        MessageStatus::Complete => {
            if st.store.resolve_thread_problems(&m.thread_id).unwrap_or(0) > 0 {
                st.emit(Event::ProblemsChanged);
            }
        }
        MessageStatus::Error => {
            let Ok(Some(thread)) = st.store.get_thread(&m.thread_id) else { return };
            let name = st.store.get_agent(&thread.agent_id).ok().flatten().map(|a| a.name).unwrap_or_else(|| "An agent".into());
            let detail = m.error.clone().unwrap_or_else(|| "The reply failed.".into());
            record(st, "reply", &thread.agent_id, &m.thread_id, &m.id, "", format!("{name}'s reply failed"), detail);
        }
        _ => {}
    }
}
