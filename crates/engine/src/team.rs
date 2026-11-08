//! Team tools: how agents create each other, hand out tasks and collect the
//! results. A delegated task is an ordinary conversation in the other agent's
//! thread list (so the user can watch it); its id is the task id.

use std::time::{Duration, Instant};

use iris_core::{Agent, MessageRole, MessageStatus, NewAgent, MAX_AGENTS};
use iris_providers::ToolCall;
use serde_json::Value;

use crate::{
    api::{self, AppState},
    events::Event,
    tools::{ToolOutcome, truncate},
};

const DEFAULT_TOOLS: &[&str] = &["web_search", "web_fetch", "get_time", "files"];
/// Tools an agent may hand to agents it creates (not `team`: one coordinator).
const GRANTABLE_TOOLS: &[&str] = &["web_search", "web_fetch", "get_time", "files", "shell"];
const MAX_RESULT_CHARS: usize = 8_000;
/// Role of a one-off agent: removed automatically once its task is done.
pub const TEMP_ROLE: &str = "temp";
/// Lets the user see the finished orb's happy hop before it disappears.
const REMOVE_AFTER: Duration = Duration::from_secs(3);
/// Never wait longer than this for the coordinator's reply to end.
const MAX_REMOVE_WAIT: Duration = Duration::from_secs(3600);

/// `thread_id` is the caller's own conversation (one-off agents it collects
/// results from leave once that reply ends).
pub async fn run(st: &AppState, me: &Agent, thread_id: &str, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    match call.name.as_str() {
        "list_agents" => list(st, me),
        "create_agent" => match (s("name"), s("purpose")) {
            (Some(name), Some(purpose)) => {
                let keep = args["keep"].as_bool().unwrap_or(false);
                create(st, me, &name, &purpose, s("instructions"), &args["tools"], keep)
            }
            _ => err("Creating an agent failed", "`name` and `purpose` are required"),
        },
        "delegate_task" => match (s("agent"), s("task")) {
            (Some(agent), Some(task)) => delegate(st, me, thread_id, &agent, &task),
            _ => err("Delegating failed", "`agent` and `task` are required"),
        },
        "check_task" => match s("task_id") {
            Some(id) => check(st, me, thread_id, &id, args["wait_seconds"].as_u64().unwrap_or(0).min(60)).await,
            None => err("Checking a task failed", "`task_id` is required"),
        },
        "keep_agent" => match s("agent") {
            Some(agent) => keep(st, me, &agent),
            None => err("Keeping an agent failed", "`agent` is required"),
        },
        "dismiss_agent" => match s("agent") {
            Some(agent) => dismiss(st, me, &agent),
            None => err("Dismissing failed", "`agent` is required"),
        },
        other => err(format!("Unknown team tool {other}"), "no such tool"),
    }
}

fn err(summary: impl Into<String>, e: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: false, output: format!("Error: {}", e.into()), summary: summary.into() }
}

fn ok(summary: impl Into<String>, output: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: true, output: output.into(), summary: summary.into() }
}

pub(crate) fn find(st: &AppState, name: &str) -> Option<Agent> {
    let agents = st.store.list_agents().ok()?;
    agents
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(name) || a.id == name)
        .or_else(|| agents.iter().find(|a| a.name.to_lowercase().contains(&name.to_lowercase())))
        .cloned()
}

fn list(st: &AppState, me: &Agent) -> ToolOutcome {
    let agents = match st.store.list_agents() {
        Ok(a) => a,
        Err(e) => return err("Couldn't list the team", e.to_string()),
    };
    let names: std::collections::HashMap<String, String> =
        agents.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
    let lines: Vec<String> = agents
        .iter()
        .map(|a| {
            let status = if st.is_agent_waiting(&a.id) {
                "waiting for the user's approval"
            } else if st.is_agent_busy(&a.id) {
                "busy"
            } else {
                "idle"
            };
            let who = if a.id == me.id {
                " (you)"
            } else if a.role == "main" {
                " (main agent)"
            } else if a.role == TEMP_ROLE {
                " (one-off: removed after its task)"
            } else {
                ""
            };
            let by = names.get(&a.created_by).map(|n| format!(", created by {n}")).unwrap_or_default();
            format!(
                "- {}{who}: {} [{status}; tools: {}{by}]",
                a.name,
                if a.description.is_empty() { "no description" } else { &a.description },
                if a.tools.is_empty() { "none".to_owned() } else { a.tools.join(", ") }
            )
        })
        .collect();
    ok(
        format!("Checked the team · {} agents", agents.len()),
        format!("{} of {MAX_AGENTS} agent slots used:\n{}", agents.len(), lines.join("\n")),
    )
}

fn create(
    st: &AppState,
    me: &Agent,
    name: &str,
    purpose: &str,
    instructions: Option<String>,
    tools: &Value,
    keep: bool,
) -> ToolOutcome {
    let summary = format!("Created agent “{name}”");
    let agents = match st.store.list_agents() {
        Ok(a) => a,
        Err(e) => return err(summary, e.to_string()),
    };
    if agents.len() >= MAX_AGENTS {
        return err(
            format!("Couldn't create “{name}”: team is full"),
            format!("the team already has {MAX_AGENTS} agents (the maximum). Reuse one, or dismiss one you created."),
        );
    }
    if agents.iter().any(|a| a.name.eq_ignore_ascii_case(name)) {
        return err(format!("Couldn't create “{name}”"), format!("an agent named {name} already exists; delegate to it instead"));
    }
    let mut tools: Vec<String> = tools
        .as_array()
        .map(|a| a.iter().filter_map(|t| t.as_str()).filter(|t| GRANTABLE_TOOLS.contains(t)).map(str::to_owned).collect())
        .unwrap_or_default();
    if tools.is_empty() {
        tools = DEFAULT_TOOLS.iter().map(|t| t.to_string()).collect();
    }
    let used: Vec<String> = agents.iter().map(|a| a.color.clone()).collect();
    let system_prompt = format!(
        "You are {name}, a specialist agent on the user's I.R.I.S. team. Your purpose: {purpose}\n\
         You receive tasks from {boss} (the team's coordinator) or from the user directly. Do each task \
         thoroughly with your tools, then reply with a clear, complete summary of the result.{extra}",
        boss = me.name,
        extra = instructions.map(|i| format!("\n\n{i}")).unwrap_or_default(),
    );
    match st.store.create_agent(NewAgent {
        name: name.to_owned(),
        description: purpose.to_owned(),
        avatar: String::new(),
        color: iris_core::next_agent_color(&used),
        system_prompt,
        provider: String::new(),
        model: String::new(),
        reasoning_effort: String::new(),
        tools: tools.clone(),
        role: if keep { String::new() } else { TEMP_ROLE.to_owned() },
        created_by: me.id.clone(),
    }) {
        Ok(agent) => {
            let agent = st.apply_default_auto(agent);
            crate::memory::save_soul(&st.data_dir, &agent);
            st.emit(Event::AgentUpdated { agent: agent.clone() });
            let lifetime = if keep {
                "It stays on the team."
            } else {
                "It is a one-off agent: once you collect its finished result with check_task, it is removed when \
                 your reply ends (unless you call keep_agent)."
            };
            ok(
                if keep { summary } else { format!("{summary} (one-off)") },
                format!(
                    "Created {} (tools: {}). {} of {MAX_AGENTS} slots now used. {lifetime} Give it work with delegate_task.",
                    agent.name,
                    tools.join(", "),
                    agents.len() + 1
                ),
            )
        }
        Err(e) => err(summary, e.to_string()),
    }
}

pub(crate) fn delegate(st: &AppState, me: &Agent, my_thread: &str, name: &str, task: &str) -> ToolOutcome {
    let Some(target) = find(st, name) else {
        return err(format!("No agent called “{name}”"), format!("there is no agent called {name}; use list_agents or create_agent"));
    };
    if target.id == me.id {
        return err("Can't delegate to yourself", "do this task yourself instead");
    }
    // Every agent has one chat; the task goes into it.
    let thread = match st.store.chat_thread(&target.id, &target.name) {
        Ok(t) => t,
        Err(e) => return err("Delegating failed", e.to_string()),
    };
    let cancel = match api::claim_thread(st, &thread.id, &target.id) {
        Ok(c) => c,
        Err(_) => {
            return err(
                format!("{} is busy", target.name),
                format!("{} is busy with something else right now; wait (check_task on its current task) or use another agent", target.name),
            )
        }
    };
    let content = format!(
        "[Task from {} — your team's coordinator]\n\n{task}\n\nWhen you are done, reply with a clear summary of the result.",
        me.name
    );
    match st.store.insert_message(&thread.id, MessageRole::User, &content, MessageStatus::Complete, None) {
        Ok(m) => st.emit(Event::MessageCreated { message: m }),
        Err(e) => {
            api::release_thread(st, &thread.id);
            return err("Delegating failed", e.to_string());
        }
    }
    let reply = match api::start_reply(st, target.clone(), &thread.id, cancel) {
        Ok(r) => r,
        Err(e) => return err("Delegating failed", e.message()),
    };
    st.followups.lock().unwrap().insert(reply.id.clone(), (me.id.clone(), my_thread.to_owned(), thread.id.clone()));
    ok(
        format!("Handed {} a task: “{}”", target.name, truncate(task, 60).replace('\n', " ")),
        format!(
            "{} started working on it. task_id: {}\nDon't wait for it: tell the user in one short line and end \
             your reply. You get an automatic update when it finishes; then read the result with check_task.",
            target.name, reply.id
        ),
    )
}

async fn check(st: &AppState, me: &Agent, my_thread: &str, task_id: &str, wait_secs: u64) -> ToolOutcome {
    let Ok(Some(task)) = st.store.get_message(task_id) else {
        return err("Unknown task", format!("no task with id {task_id}"));
    };
    let Ok(Some(thread)) = st.store.get_thread(&task.thread_id) else {
        return err("Unknown task", "the agent doing this task is gone");
    };
    let agent = st.store.get_agent(&thread.agent_id).ok().flatten();
    let running = || st.is_reply_running(&thread.id, task_id);
    let agent_name = agent.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| "The agent".into());

    // Wait (politely) for it to finish, so the coordinator doesn't burn tokens polling.
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    while running() && Instant::now() < deadline {
        // It needs the user: tell the coordinator now instead of waiting it out.
        if let Some(approval) = st.approval_of(&thread.agent_id).filter(|a| a.message_id == task_id) {
            return ok(
                format!("{agent_name} needs your approval"),
                format!(
                    "{agent_name} is paused, waiting for the user to approve this step: {}\n\
                     Tell the user in one short line that {agent_name} needs their approval (they can approve it \
                     from the pop-up in the app, or turn on Auto in the chat box to let a safety check approve \
                     safe commands), then end your reply: you get an automatic update when the task finishes.",
                    approval.summary
                ),
            );
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    if running() {
        let activity = st.thread_activity(&thread.id).unwrap_or_else(|| "working".into());
        return ok(
            format!("{agent_name} is still working ({activity})"),
            format!("{agent_name} is still working on it (currently: {activity}). Check again later."),
        );
    }
    let last = st.store.get_message(task_id).ok().flatten();
    // The result is collected now; no need to bring the coordinator back for it.
    st.followups.lock().unwrap().remove(task_id);
    match last {
        Some(m) if m.status == MessageStatus::Complete => {
            let mut output = format!("Result from {agent_name}:\n\n{}", truncate(&m.content, MAX_RESULT_CHARS));
            let mut summary = format!("{agent_name} finished the task");
            // One-off agents leave once the coordinator's reply ends.
            if let Some(a) = agent.filter(|a| a.role == TEMP_ROLE && a.created_by == me.id) {
                remove_after_reply(st.clone(), a.id.clone(), my_thread.to_owned());
                output.push_str(&format!(
                    "\n\n({agent_name} is a one-off agent: it will be removed, with its chat, when you \
                     finish this reply, so include what matters from this result in your answer. If the result \
                     shows it needs more information or a decision from the user, call keep_agent so it stays.)"
                ));
                summary.push_str(" · one-off");
            }
            ok(summary, output)
        }
        Some(m) if m.status == MessageStatus::Cancelled => ok(
            format!("{agent_name}'s task was stopped"),
            format!("The task was stopped by the user. Partial result:\n\n{}", truncate(&m.content, MAX_RESULT_CHARS)),
        ),
        Some(m) => err(
            format!("{agent_name}'s task failed"),
            format!("{agent_name} could not finish: {}", m.error.unwrap_or_else(|| "unknown error".into())),
        ),
        None => err(format!("{agent_name} has no answer yet"), "the task has no reply"),
    }
}

/// Turns a one-off agent into a lasting team member.
fn keep(st: &AppState, me: &Agent, name: &str) -> ToolOutcome {
    let Some(target) = find(st, name) else {
        return err(format!("No agent called “{name}”"), format!("there is no agent called {name}"));
    };
    if target.role != TEMP_ROLE {
        return ok(format!("{} stays on the team", target.name), format!("{} is already a lasting team member.", target.name));
    }
    if target.created_by != me.id {
        return err(format!("Can't keep {}", target.name), "you can only keep agents you created");
    }
    match st.store.set_agent_role(&target.id, "") {
        Ok(_) => {
            if let Ok(Some(agent)) = st.store.get_agent(&target.id) {
                st.emit(Event::AgentUpdated { agent });
            }
            ok(format!("Kept {} on the team", target.name), format!("{} will stay on the team.", target.name))
        }
        Err(e) => err(format!("Couldn't keep {}", target.name), e.to_string()),
    }
}

fn dismiss(st: &AppState, me: &Agent, name: &str) -> ToolOutcome {
    let Some(target) = find(st, name) else {
        return err(format!("No agent called “{name}”"), format!("there is no agent called {name}"));
    };
    if target.created_by != me.id {
        return err(
            format!("Can't dismiss {}", target.name),
            format!("you can only dismiss agents you created; {} was created by someone else", target.name),
        );
    }
    let mut thread_ids = Vec::new();
    for thread in st.store.list_threads(&target.id).unwrap_or_default() {
        st.cancel_thread(&thread.id);
        thread_ids.push(thread.id);
    }
    match st.store.delete_agent(&target.id) {
        Ok(_) => {
            crate::memory::remove_agent_files(&st.data_dir, &target.id);
            crate::attachments::remove_threads(&st.data_dir, &thread_ids);
            st.emit(Event::AgentDeleted { agent_id: target.id.clone() });
            ok(format!("Dismissed {}", target.name), format!("{} was removed from the team.", target.name))
        }
        Err(e) => err(format!("Couldn't dismiss {}", target.name), e.to_string()),
    }
}

/// Removes a finished one-off agent once the coordinator's reply (the one
/// that collected its result) ends, after a short pause so its happy orb is
/// seen. Skipped if it was kept or picked up other work in the meantime.
fn remove_after_reply(st: AppState, agent_id: String, coordinator_thread: String) {
    tokio::spawn(async move {
        let started = Instant::now();
        while st.is_thread_running(&coordinator_thread) && started.elapsed() < MAX_REMOVE_WAIT {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        tokio::time::sleep(REMOVE_AFTER).await;
        if st.is_agent_busy(&agent_id) || st.is_agent_waiting(&agent_id) {
            return;
        }
        match st.store.get_agent(&agent_id) {
            Ok(Some(a)) if a.role == TEMP_ROLE => {}
            _ => return,
        }
        let thread_ids: Vec<String> = st.store.list_threads(&agent_id).unwrap_or_default().into_iter().map(|t| t.id).collect();
        if st.store.delete_agent(&agent_id).is_ok() {
            crate::memory::remove_agent_files(&st.data_dir, &agent_id);
            crate::attachments::remove_threads(&st.data_dir, &thread_ids);
            st.emit(Event::AgentDeleted { agent_id });
        }
    });
}

/// Marks the automatic message that brings a coordinator back to a result.
pub const UPDATE_PREFIX: &str = "[I.R.I.S. update]";

/// Called whenever a reply ends. If it was a delegated task whose
/// coordinator has gone idle, or a coordinator's reply that ended with
/// finished results still uncollected, the coordinator gets an automatic
/// message and starts a new reply to collect the result and tell the user.
pub fn after_reply(st: &AppState, thread_id: &str) {
    let due: Vec<(String, String, String, String)> = {
        let followups = st.followups.lock().unwrap();
        followups
            .iter()
            .filter(|(task, (_, coord_thread, task_thread))| {
                (task_thread == thread_id || coord_thread == thread_id)
                    && !st.is_reply_running(task_thread, task)
                    && !st.is_thread_running(coord_thread)
            })
            .map(|(task, (coord, coord_thread, task_thread))| {
                (task.clone(), coord.clone(), coord_thread.clone(), task_thread.clone())
            })
            .collect()
    };
    for (task, coord, coord_thread, task_thread) in due {
        st.followups.lock().unwrap().remove(&task);
        let Ok(Some(coordinator)) = st.store.get_agent(&coord) else { continue };
        let worker = st
            .store
            .get_thread(&task_thread)
            .ok()
            .flatten()
            .and_then(|t| st.store.get_agent(&t.agent_id).ok().flatten())
            .map(|a| a.name)
            .unwrap_or_else(|| "The agent".into());
        let Ok(cancel) = api::claim_thread(st, &coord_thread, &coordinator.id) else { continue };
        let content = format!(
            "{UPDATE_PREFIX} {worker} has finished the task you gave it (task_id: {task}). Collect the result \
             with check_task and report back to the user."
        );
        match st.store.insert_message(&coord_thread, MessageRole::User, &content, MessageStatus::Complete, None) {
            Ok(m) => st.emit(Event::MessageCreated { message: m }),
            Err(_) => {
                api::release_thread(st, &coord_thread);
                continue;
            }
        }
        let _ = api::start_reply(st, coordinator, &coord_thread, cancel);
    }
}
