//! Messages sent while an agent is busy wait in its chat's queue and go out,
//! in order, as soon as its current reply ends. "Steer" puts one first and
//! stops the current reply, so the agent turns to it right away.

use iris_core::{Attachment, MessageRole, MessageStatus};
use serde::Serialize;

use crate::{
    api::{self, AppState},
    events::Event,
};

/// Each chat's queue holds at most this many messages.
pub const MAX_QUEUED: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct Queued {
    pub id: String,
    pub content: String,
    pub attachments: Vec<Attachment>,
    pub created_at: i64,
}

fn changed(st: &AppState, thread_id: &str, queue: Vec<Queued>) {
    st.emit(Event::QueueChanged { thread_id: thread_id.to_owned(), queue });
}

/// Adds a message to a busy chat's queue.
pub fn push(st: &AppState, thread_id: &str, content: &str, attachments: Vec<Attachment>) -> Result<Queued, String> {
    let item = Queued {
        id: uuid::Uuid::now_v7().simple().to_string(),
        content: content.to_owned(),
        attachments,
        created_at: iris_core::now_ms(),
    };
    let snapshot = {
        let mut queues = st.queues.lock().unwrap();
        let queue = queues.entry(thread_id.to_owned()).or_default();
        if queue.len() >= MAX_QUEUED {
            return Err(format!("{MAX_QUEUED} messages are already waiting in this chat"));
        }
        queue.push(item.clone());
        queue.clone()
    };
    changed(st, thread_id, snapshot);
    Ok(item)
}

/// Removes a queued message; false if it isn't there (already sent).
pub fn remove(st: &AppState, thread_id: &str, id: &str) -> bool {
    let snapshot = {
        let mut queues = st.queues.lock().unwrap();
        let Some(queue) = queues.get_mut(thread_id) else { return false };
        let before = queue.len();
        queue.retain(|q| q.id != id);
        if queue.len() == before {
            return false;
        }
        queue.clone()
    };
    changed(st, thread_id, snapshot);
    true
}

/// Puts a queued message first and stops the current reply, so it goes next.
pub fn steer(st: &AppState, thread_id: &str, id: &str) -> bool {
    let snapshot = {
        let mut queues = st.queues.lock().unwrap();
        let Some(queue) = queues.get_mut(thread_id) else { return false };
        let Some(pos) = queue.iter().position(|q| q.id == id) else { return false };
        let item = queue.remove(pos);
        queue.insert(0, item);
        queue.clone()
    };
    changed(st, thread_id, snapshot);
    // The reply's end starts the next queued message; nothing running: start it now.
    if !st.stop_thread_run(thread_id) {
        start_next(st, thread_id);
    }
    true
}

/// Every chat's queue (for clients catching up).
pub fn all(st: &AppState) -> std::collections::HashMap<String, Vec<Queued>> {
    st.queues.lock().unwrap().iter().filter(|(_, q)| !q.is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Sends the next queued message, if the chat is free. Called when a reply ends.
pub fn start_next(st: &AppState, thread_id: &str) {
    let Ok(Some(thread)) = st.store.get_thread(thread_id) else {
        st.queues.lock().unwrap().remove(thread_id);
        return;
    };
    let Ok(Some(agent)) = st.store.get_agent(&thread.agent_id) else { return };
    if st.queues.lock().unwrap().get(thread_id).is_none_or(|q| q.is_empty()) {
        return;
    }
    let Ok(cancel) = api::claim_thread(st, thread_id, &agent.id) else { return };
    let (item, snapshot) = {
        let mut queues = st.queues.lock().unwrap();
        let Some(queue) = queues.get_mut(thread_id).filter(|q| !q.is_empty()) else {
            drop(queues);
            api::release_thread(st, thread_id);
            return;
        };
        let item = queue.remove(0);
        (item, queue.clone())
    };
    changed(st, thread_id, snapshot);
    let mut user = match st.store.insert_message(thread_id, MessageRole::User, &item.content, MessageStatus::Complete, None) {
        Ok(m) => m,
        Err(_) => {
            api::release_thread(st, thread_id);
            return;
        }
    };
    if !item.attachments.is_empty() && st.store.set_attachments(&user.id, &item.attachments).is_ok() {
        user.attachments = item.attachments;
    }
    st.emit(Event::MessageCreated { message: user });
    let _ = api::start_reply(st, agent, thread_id, cancel);
}
