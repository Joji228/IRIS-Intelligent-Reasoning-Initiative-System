//! Routines: things agents do on a schedule. A "prompt" routine sends the
//! agent a task in its chat; a "command" routine runs PowerShell without the
//! AI and only wakes the agent when it fails (or always, if asked). Checked
//! every 20 s; a routine whose agent is busy waits for the next check.

use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};
use iris_core::{MessageRole, MessageStatus, Routine};
use iris_providers::ToolCall;
use serde_json::Value;
use tracing::{info, warn};

use crate::{
    api::{self, AppState},
    events::Event,
    tools::{ToolOutcome, truncate},
};

/// Routines can't run more often than this (the server and the token budget).
const MIN_EVERY_MINUTES: i64 = 5;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
/// How routine messages start in the agent's chat (the app shows them as a note).
pub const ROUTINE_PREFIX: &str = "[Routine:";

/// When a schedule next fires after `now`, or why it can't be read.
pub fn next_run(schedule: &str, now: chrono::DateTime<Local>) -> Result<Option<i64>, String> {
    let s = schedule.trim().to_lowercase();
    let words: Vec<&str> = s.split_whitespace().collect();
    let time = |t: &str| NaiveTime::parse_from_str(t, "%H:%M").map_err(|_| format!("“{t}” isn't a time like 08:00"));
    let at = |date: NaiveDate, t: NaiveTime| Local.from_local_datetime(&date.and_time(t)).earliest().map(|d| d.timestamp_millis());
    match words.as_slice() {
        ["every", n] => {
            let (num, unit) = n.split_at(n.find(|c: char| !c.is_ascii_digit()).unwrap_or(n.len()));
            let num: i64 = num.parse().map_err(|_| "say e.g. “every 30m” or “every 2h”".to_string())?;
            let minutes = match unit {
                "m" | "min" | "mins" => num,
                "h" | "hr" | "hours" | "hour" => num * 60,
                "d" | "day" | "days" => num * 1440,
                _ => return Err("the unit must be m, h or d (e.g. “every 30m”)".into()),
            };
            if minutes < MIN_EVERY_MINUTES {
                return Err(format!("routines run at most every {MIN_EVERY_MINUTES} minutes"));
            }
            Ok(Some(now.timestamp_millis() + minutes * 60_000))
        }
        ["daily", t] | ["weekdays", t] | ["weekends", t] => {
            let t = time(t)?;
            let fits = |d: NaiveDate| match words[0] {
                "weekdays" => d.weekday().num_days_from_monday() < 5,
                "weekends" => d.weekday().num_days_from_monday() >= 5,
                _ => true,
            };
            Ok((0..8)
                .map(|i| now.date_naive() + chrono::Days::new(i))
                .filter(|d| fits(*d))
                .filter_map(|d| at(d, t))
                .find(|ms| *ms > now.timestamp_millis()))
        }
        ["weekly", day, t] => {
            let t = time(t)?;
            let wd: Weekday = day.parse().map_err(|_| format!("“{day}” isn't a weekday like mon"))?;
            Ok((0..8)
                .map(|i| now.date_naive() + chrono::Days::new(i))
                .filter(|d| d.weekday() == wd)
                .filter_map(|d| at(d, t))
                .find(|ms| *ms > now.timestamp_millis()))
        }
        ["once", date, t] => {
            let dt = NaiveDateTime::parse_from_str(&format!("{date} {t}"), "%Y-%m-%d %H:%M")
                .map_err(|_| "say e.g. “once 2026-11-02 08:00”".to_string())?;
            let ms = Local.from_local_datetime(&dt).earliest().map(|d| d.timestamp_millis());
            Ok(ms.filter(|ms| *ms > now.timestamp_millis()))
        }
        _ => Err("use “every 30m”, “daily 08:00”, “weekdays 09:00”, “weekly mon 09:00” or “once 2026-11-02 08:00”".into()),
    }
}

/// Creates a routine (checked) and saves it.
pub fn create(st: &AppState, agent_id: &str, name: &str, schedule: &str, kind: &str, body: &str, notify: &str) -> Result<Routine, String> {
    if !matches!(kind, "prompt" | "command" | "heartbeat") {
        return Err("kind must be prompt, command or heartbeat".into());
    }
    if name.trim().is_empty() || body.trim().is_empty() {
        return Err("a routine needs a name and a task or command".into());
    }
    let next = next_run(schedule, Local::now())?.ok_or("that time has already passed")?;
    let r = Routine {
        id: iris_core::new_id(),
        agent_id: agent_id.to_owned(),
        name: name.trim().chars().take(60).collect(),
        schedule: schedule.trim().to_lowercase(),
        kind: kind.into(),
        body: body.trim().to_owned(),
        notify: if notify == "always" { "always".into() } else { "failure".into() },
        enabled: true,
        next_run: Some(next),
        last_run: None,
        last_result: String::new(),
        created_at: iris_core::now_ms(),
    };
    st.store.save_routine(&r).map_err(|e| e.to_string())?;
    st.emit(Event::RoutinesChanged);
    Ok(r)
}

/// Whether a schedule runs only once ("once 2026-11-02 08:00").
fn is_once(schedule: &str) -> bool {
    schedule.trim().to_lowercase().starts_with("once ")
}

/// A one-off that is done: it ran, its time has passed, and it didn't end in a failed
/// command (that one stays, switched off, so the user sees it and can run it again).
fn finished_one_off(r: &Routine, now: chrono::DateTime<Local>) -> bool {
    is_once(&r.schedule)
        && !r.enabled
        && r.last_run.is_some()
        && !r.last_result.starts_with("failed")
        && matches!(next_run(&r.schedule, now), Ok(None))
}

/// Removes one-offs that already ran (left over from versions that only switched them off).
fn sweep_finished(st: &AppState) {
    let now = Local::now();
    let mut removed = 0;
    for r in st.store.list_routines().unwrap_or_default() {
        if finished_one_off(&r, now) && st.store.delete_routine(&r.id).unwrap_or(false) {
            info!("removed finished one-off routine “{}”", r.name);
            removed += 1;
        }
    }
    if removed > 0 {
        st.emit(Event::RoutinesChanged);
    }
}

/// The scheduler: runs whatever is due, forever.
pub async fn run_forever(st: AppState) {
    sweep_finished(&st);
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let now = iris_core::now_ms();
        let due: Vec<Routine> = st
            .store
            .list_routines()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.enabled && r.next_run.is_some_and(|t| t <= now))
            .collect();
        for r in due {
            fire(&st, r).await;
        }
    }
}

/// Runs one routine now and schedules its next run.
pub async fn fire(st: &AppState, mut r: Routine) {
    let Ok(Some(agent)) = st.store.get_agent(&r.agent_id) else { return };
    let Ok(thread) = st.store.chat_thread(&agent.id, &agent.name) else { return };
    if st.is_thread_running(&thread.id) {
        return; // busy: try again at the next check
    }
    info!("routine “{}” for {}", r.name, agent.name);
    let wake = match r.kind.as_str() {
        "command" => {
            let workspace = st.data_dir.join("workspaces").join(&agent.id);
            let out = crate::tools::run_powershell(&workspace, &r.body, COMMAND_TIMEOUT).await;
            r.last_result = if out.ok { "ok".into() } else { format!("failed · {}", truncate(out.output.trim(), 200)) };
            if !out.ok {
                crate::problems::record(
                    st,
                    "routine",
                    &agent.id,
                    &thread.id,
                    "",
                    &r.id,
                    format!("Routine “{}” failed", r.name),
                    // PowerShell echoes the whole script line, our UTF-8 setup included.
                    truncate(out.output.replace("[Console]::OutputEncoding = [Text.Encoding]::UTF8; ", "").trim(), 300),
                );
            }
            (!out.ok || r.notify == "always").then(|| {
                format!(
                    "{ROUTINE_PREFIX} {}] The scheduled command {}:\n```\n{}\n```\n{}",
                    r.name,
                    if out.ok { "ran" } else { "FAILED" },
                    truncate(&out.output, 4000),
                    if out.ok { "Tell the user the result in one or two lines." } else { "Look into it and tell the user briefly what's wrong." }
                )
            })
        }
        "heartbeat" => {
            r.last_result = "checking".into();
            Some(format!(
                "{ROUTINE_PREFIX} {}] Heartbeat: go through this checklist (use your tools as needed). If nothing \
                 needs the user's attention, reply with exactly {HEARTBEAT_OK} and nothing else; otherwise tell the \
                 user briefly what needs attention.\n\nChecklist:\n{}",
                r.name, r.body
            ))
        }
        _ => {
            r.last_result = "sent to the agent".into();
            Some(format!("{ROUTINE_PREFIX} {}] {}", r.name, r.body))
        }
    };
    r.last_run = Some(iris_core::now_ms());
    r.next_run = match next_run(&r.schedule, Local::now()) {
        Ok(n) => n,
        Err(e) => {
            warn!("routine {}: {e}", r.name);
            None
        }
    };
    if r.next_run.is_none() {
        r.enabled = false; // a one-off that has run (or a schedule that no longer reads)
    }
    // A one-off is done once it has run: remove it, unless its command failed.
    if finished_one_off(&r, Local::now()) {
        let _ = st.store.delete_routine(&r.id);
        info!("one-off routine “{}” ran; removed", r.name);
    } else {
        let _ = st.store.save_routine(&r);
    }
    st.emit(Event::RoutinesChanged);

    if let Some(content) = wake {
        let Ok(cancel) = api::claim_thread(st, &thread.id, &agent.id) else { return };
        let trigger = match st.store.insert_message(&thread.id, MessageRole::User, &content, MessageStatus::Complete, None) {
            Ok(m) => {
                st.emit(Event::MessageCreated { message: m.clone() });
                m
            }
            Err(_) => {
                api::release_thread(st, &thread.id);
                return;
            }
        };
        // Subscribe before the reply starts, so its end can't be missed.
        let events = st.events.subscribe();
        if let Ok(reply) = api::start_reply(st, agent, &thread.id, cancel) {
            if r.kind == "heartbeat" {
                tokio::spawn(settle_heartbeat(st.clone(), r.id.clone(), trigger.id, reply.id, events));
            }
        }
    }
}

/// What an agent answers when a heartbeat finds nothing to report.
const HEARTBEAT_OK: &str = "HEARTBEAT_OK";
/// A heartbeat reply taking longer than this is left alone.
const HEARTBEAT_WAIT: Duration = Duration::from_secs(30 * 60);

/// Waits for a heartbeat's reply. "Nothing to report" is removed from the chat
/// with its trigger, so quiet checks don't fill the conversation; anything
/// else stays, for the user to see.
async fn settle_heartbeat(
    st: AppState,
    routine_id: String,
    trigger_id: String,
    reply_id: String,
    mut events: tokio::sync::broadcast::Receiver<Event>,
) {
    let finished = tokio::time::timeout(HEARTBEAT_WAIT, async {
        loop {
            match events.recv().await {
                Ok(Event::MessageCompleted { message }) if message.id == reply_id => return Some(message),
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    })
    .await;
    // Missed the event (lagged or timed out): read what was saved.
    let message = match finished {
        Ok(Some(m)) => m,
        _ => match st.store.get_message(&reply_id) {
            Ok(Some(m)) if m.status != MessageStatus::Streaming => m,
            _ => return,
        },
    };
    let quiet = message.status == MessageStatus::Complete && is_all_clear(&message.content);
    if quiet {
        for id in [&trigger_id, &reply_id] {
            if st.store.delete_message(id).unwrap_or(false) {
                st.emit(Event::MessageDeleted { thread_id: message.thread_id.clone(), message_id: id.clone() });
            }
        }
    }
    if let Ok(Some(mut r)) = st.store.get_routine(&routine_id) {
        r.last_result = if quiet { "all clear".into() } else { "reported something".into() };
        let _ = st.store.save_routine(&r);
        st.emit(Event::RoutinesChanged);
    }
}

/// Whether a heartbeat reply means "nothing to report".
fn is_all_clear(reply: &str) -> bool {
    let t = reply.trim().trim_matches(|c: char| c == '`' || c == '*' || c == '.' || c.is_whitespace());
    t.eq_ignore_ascii_case(HEARTBEAT_OK)
}

/// Agent tools: create_routine, list_routines, delete_routine.
pub async fn tool(st: &AppState, me: &iris_core::Agent, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().map(str::trim).unwrap_or_default().to_owned();
    let fail = |summary: &str, e: String| ToolOutcome { ok: false, output: format!("Error: {e}"), summary: summary.into() };
    match call.name.as_str() {
        "create_routine" => {
            // Iris can schedule for another agent by name.
            let agent_id = match s("agent").as_str() {
                "" | "me" => me.id.clone(),
                name => match crate::team::find(st, name) {
                    Some(a) => a.id,
                    None => return fail("Couldn't create the routine", format!("there is no agent called {name}")),
                },
            };
            match create(st, &agent_id, &s("name"), &s("schedule"), &s("kind"), &s("task"), &s("notify")) {
                Ok(r) => ToolOutcome {
                    ok: true,
                    output: format!("Routine “{}” created ({}). Next run: {}.", r.name, r.schedule, when(r.next_run)),
                    summary: format!("Scheduled “{}” · {}", r.name, r.schedule),
                },
                Err(e) => fail("Couldn't create the routine", e),
            }
        }
        "list_routines" => {
            let names: std::collections::HashMap<String, String> =
                st.store.list_agents().unwrap_or_default().into_iter().map(|a| (a.id, a.name)).collect();
            let list: Vec<String> = st
                .store
                .list_routines()
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    format!(
                        "- {} [{}] for {}: {} · {} · next {} · last: {}",
                        r.name,
                        r.kind,
                        names.get(&r.agent_id).map(String::as_str).unwrap_or("?"),
                        r.schedule,
                        if r.enabled { "on" } else { "off" },
                        when(r.next_run),
                        if r.last_result.is_empty() { "never ran" } else { &r.last_result }
                    )
                })
                .collect();
            ToolOutcome {
                ok: true,
                output: if list.is_empty() { "No routines yet.".into() } else { list.join("\n") },
                summary: format!("Checked the routines · {}", list.len()),
            }
        }
        "delete_routine" => {
            let name = s("name");
            let found = st.store.list_routines().unwrap_or_default().into_iter().find(|r| r.name.eq_ignore_ascii_case(&name));
            match found {
                Some(r) => {
                    let _ = st.store.delete_routine(&r.id);
                    st.emit(Event::RoutinesChanged);
                    ToolOutcome { ok: true, output: format!("Deleted “{}”.", r.name), summary: format!("Deleted routine “{}”", r.name) }
                }
                None => fail("Routine not found", format!("there is no routine called {name}")),
            }
        }
        other => fail("Unknown routine tool", format!("no tool called {other}")),
    }
}

/// Creating a scheduled command (it would run unattended) asks the user,
/// unless Full auto and the command isn't catastrophic.
pub fn needs_approval(call: &ToolCall, mode: crate::api::AutoMode) -> bool {
    if call.name != "create_routine" {
        return false;
    }
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    if args["kind"].as_str() != Some("command") {
        return false;
    }
    mode != crate::api::AutoMode::Full || crate::autoapprove::catastrophic(args["task"].as_str().unwrap_or_default()).is_some()
}

fn when(ms: Option<i64>) -> String {
    ms.and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map(|d| d.format("%a %d %b %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules() {
        let now = Local.with_ymd_and_hms(2026, 11, 2, 10, 0, 0).unwrap(); // a Monday
        let at = |s: &str| next_run(s, now).unwrap().map(|ms| Local.timestamp_millis_opt(ms).unwrap().format("%a %H:%M").to_string());
        assert_eq!(at("every 30m").as_deref(), Some("Mon 10:30"));
        assert_eq!(at("daily 08:00").as_deref(), Some("Tue 08:00"));
        assert_eq!(at("daily 18:30").as_deref(), Some("Mon 18:30"));
        assert_eq!(at("weekly sat 09:00").as_deref(), Some("Sat 09:00"));
        assert_eq!(at("weekends 09:00").as_deref(), Some("Sat 09:00"));
        assert_eq!(at("once 2026-11-01 08:00"), None, "past one-offs never fire");
        assert!(next_run("every 1m", now).is_err());
        assert!(next_run("sometimes", now).is_err());
    }

    #[test]
    fn one_offs_are_finished_once_they_ran() {
        let now = Local.with_ymd_and_hms(2026, 11, 10, 12, 0, 0).unwrap();
        let routine = |schedule: &str, enabled: bool, ran: bool, result: &str| Routine {
            id: "r".into(),
            agent_id: "a".into(),
            name: "n".into(),
            schedule: schedule.into(),
            kind: "prompt".into(),
            body: "b".into(),
            notify: "failure".into(),
            enabled,
            next_run: None,
            last_run: ran.then_some(1),
            last_result: result.into(),
            created_at: 0,
        };
        // Ran, time passed: done, removed.
        assert!(finished_one_off(&routine("once 2026-11-05 11:30", false, true, "sent to the agent"), now));
        assert!(finished_one_off(&routine("once 2026-11-05 11:30", false, true, "ok"), now));
        // A failed command stays so the user sees it.
        assert!(!finished_one_off(&routine("once 2026-11-05 11:30", false, true, "failed · exit 1"), now));
        // Not yet run, still on, or still in the future: kept.
        assert!(!finished_one_off(&routine("once 2026-11-05 11:30", false, false, ""), now));
        assert!(!finished_one_off(&routine("once 2026-11-05 11:30", true, true, "ok"), now));
        assert!(!finished_one_off(&routine("once 2026-11-12 09:00", false, true, "ok"), now), "Run now on a future one-off keeps it");
        // Repeating routines are never removed.
        assert!(!finished_one_off(&routine("daily 08:00", false, true, "ok"), now));
    }

    #[test]
    fn heartbeat_all_clear() {
        assert!(is_all_clear("HEARTBEAT_OK"));
        assert!(is_all_clear("  `HEARTBEAT_OK`.\n"));
        assert!(is_all_clear("**heartbeat_ok**"));
        assert!(!is_all_clear("HEARTBEAT_OK, but the disk is 95% full"));
        assert!(!is_all_clear("The site is down."));
    }
}
