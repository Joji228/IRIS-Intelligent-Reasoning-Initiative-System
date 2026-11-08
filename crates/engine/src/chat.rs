//! Generating an agent's reply.
//!
//! A reply is a loop of *rounds*: ask the model (streaming its thinking and
//! text to clients), and if it asks for tools, run them (after the user's
//! approval where required), feed the results back and ask again, until the
//! model answers without tools. Progress is saved as it happens, so a reload or
//! another device always sees the steps taken so far.

use iris_core::{
    Agent, Message, MessageResult, MessageRole, MessageStatus, Round, ToolLogEntry, ToolStatus,
    ToolStep, new_id, now_ms,
};
use iris_providers::{
    ChatMessage, ChatOptions, Completion, Delta, OpenAiCompatible, ProviderError, Reasoning, Role,
    ToolCall,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    api::AppState,
    events::{Event, PendingApproval},
    tools::{self, ToolContext},
};

/// Safety cap on messages sent per request; long chats are compacted (see
/// `compact.rs`) well before this matters.
const MAX_HISTORY: usize = 120;
/// Tool rounds per reply. After the last one the model is asked once more,
/// with tools switched off, for a final answer from what it has.
const MAX_ROUNDS: usize = 25;
/// Deep research reads many sources, so it gets more steps.
const MAX_RESEARCH_ROUNDS: usize = 45;
/// How a message asking for deep research starts (the app's Research switch
/// adds it and shows it as a chip).
pub const RESEARCH_PREFIX: &str = "[Deep research]";
const RESEARCH_INSTRUCTIONS: &str = "\n\nDeep research mode (the user's latest message starts with [Deep research]):
1. Break the question into 3–6 sub-questions.
2. For each, search (web_search) and read the best sources in full (web_fetch; the browser only for pages that need \
JavaScript). Use at least 6 distinct sources; prefer primary and official ones and check their dates. For a big \
question you may hand sub-questions to agents in parallel.
3. Cross-check key claims; where sources disagree, say so.
4. Write a report: a 3–5 line summary first, then sections. Cite every factual claim inline as [1], [2]… and end with \
a \"Sources\" list: [n] Title — URL. Say plainly what you couldn't verify.";
/// How a message spoken on a voice call starts (the phone app adds it and shows
/// it as a small phone chip).
pub const VOICE_PREFIX: &str = "[Voice call]";
const VOICE_INSTRUCTIONS: &str = "\n\nVoice call mode (the user's latest message starts with [Voice call]):
The user is talking to you on a voice call and your reply is read aloud. Their words were transcribed automatically, \
so allow for small mistakes. The user speaks only English or Greek: answer in the one they spoke, and never in any \
other language. If their message looks like another language or like nonsense, it is a transcription mistake: say \
in English that you didn't catch that and ask them to repeat. Keep what you say short and \
natural, 1–3 sentences unless they ask for more, with no markdown, lists, tables, emojis or URLs in the spoken part. \
Before a task that takes more than a few seconds, first say one short sentence (for example \"On it, give me a \
minute.\"). If the answer needs code, a table, links or a long report, say a one-sentence summary, then put the rest \
after a line that is exactly [details]. The app shows that part in the chat and doesn't read it aloud. Approvals \
can't be given by voice: when you need one, say that the user should check the screen.";

/// Whether a user message is a spoken turn from a voice call.
fn is_voice_message(content: &str) -> bool {
    content.starts_with(VOICE_PREFIX)
}

/// The extra instructions a reply gets for the kind of message it answers
/// (deep research or a voice call), or nothing for a normal message.
fn mode_instructions(research: bool, voice: bool) -> &'static str {
    if research {
        RESEARCH_INSTRUCTIONS
    } else if voice {
        VOICE_INSTRUCTIONS
    } else {
        ""
    }
}
/// Added to the history when the rounds run out.
const WRAP_UP_NOTE: &str = "[Note from I.R.I.S.] You have used every tool step allowed for this reply. Do not \
call any more tools. Give your final answer now from what you have found so far, and say briefly what you \
could not check yet.";
/// Images are sent again only from this many of the latest messages with files.
const RECENT_IMAGE_MESSAGES: usize = 3;
/// Tool results from earlier replies are sent again only this long.
const OLD_TOOL_OUTPUT_CHARS: usize = 1_500;
/// What a tool result may add to the context at most.
const MAX_TOOL_OUTPUT_CHARS: usize = 16_000;

/// Whether the reply just before `current` read outside content (web pages,
/// connected services). Its text is still in the conversation and may have
/// steered this reply too, so the "ask before lasting changes" rule carries
/// over to it instead of starting fresh.
fn previous_reply_read_outside_content(state: &AppState, thread_id: &str, current: &str) -> bool {
    state.store.list_messages(thread_id).is_ok_and(|messages| read_outside_content_before(&messages, current))
}

fn read_outside_content_before(messages: &[Message], current: &str) -> bool {
    // A task another agent handed over says whether that agent had read outside content.
    let before: Vec<&Message> = messages.iter().take_while(|m| m.id != current).collect();
    let handed_over = before
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::User)
        .is_some_and(|m| m.content.contains(crate::outside::TASK_NOTE_START));
    handed_over
        || messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant && m.id != current)
            .is_some_and(crate::outside::reply_read_outside)
}

pub async fn run(
    state: AppState,
    agent: Agent,
    thread_id: String,
    reply_id: String,
    cancel: CancellationToken,
) {
    let mut reply = Reply {
        state: &state,
        agent: &agent,
        thread_id: &thread_id,
        reply_id: &reply_id,
        rounds: Vec::new(),
        pending_images: Vec::new(),
        handed_off: Vec::new(),
        granted: Vec::new(),
        read_web: previous_reply_read_outside_content(&state, &thread_id, &reply_id),
        private: String::new(),
        research: false,
        voice: false,
    };
    let result = reply.generate(&cancel).await;
    state.runs.lock().unwrap().remove(&thread_id);
    state.emit(Event::RunFinished {
        agent_id: agent.id.clone(),
        thread_id: thread_id.clone(),
        message_id: reply_id.clone(),
        status: result.status.as_str().to_owned(),
    });
    info!(
        "reply {reply_id} in thread {thread_id}: {} ({} rounds, {} chars)",
        result.status.as_str(),
        result.rounds.len(),
        result.content.len()
    );

    match state.store.finish_message(&reply_id, &result) {
        // None: the thread was deleted while the reply was streaming.
        Ok(Some(message)) => state.emit(Event::MessageCompleted { message }),
        Ok(None) => {}
        Err(e) => error!("storing reply {reply_id}: {e}"),
    }
    if let Ok(Some(thread)) = state.store.touch_thread(&thread_id) {
        state.emit(Event::ThreadUpdated { thread });
    }
    // Messages the user sent meanwhile go first, then team updates.
    crate::queue::start_next(&state, &thread_id);
    // A delegated task finished, or a coordinator's reply ended with results
    // still out: bring the coordinator back to report.
    crate::team::after_reply(&state, &thread_id);
}

impl Reply<'_> {
    /// The tools offered right now: the agent's own, those granted during
    /// this reply, and (for everyone but Iris) a way to ask for more.
    fn specs(&self) -> Vec<iris_providers::ToolSpec> {
        let mut enabled = tools::effective_tools(self.agent);
        enabled.extend(self.granted.iter().cloned());
        let mut specs = tools::specs_for(&enabled);
        if enabled.iter().any(|t| t == "mcp") {
            specs.extend(self.state.mcp.specs());
        }
        if enabled.iter().any(|t| t == "dev") {
            specs.extend(crate::develop::specs());
        }
        specs.push(tools::ask_user_spec());
        if self.agent.role != "main" {
            specs.retain(|s| !tools::COORDINATOR_ONLY.contains(&s.name.as_str()));
            specs.push(tools::request_tools_spec());
        }
        specs
    }

    /// `request_tools`: Iris reviews the request; granted tools are saved to
    /// the agent and offered from the next step of this reply.
    async fn request_tools(&mut self, call: &ToolCall) -> tools::ToolOutcome {
        let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap_or_default();
        let have: Vec<String> = tools::effective_tools(self.agent).into_iter().chain(self.granted.iter().cloned()).collect();
        let mut wanted: Vec<String> = args["tools"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.as_str()).map(str::to_owned).collect())
            .unwrap_or_default();
        wanted.retain(|t| crate::design::DESIGNABLE_TOOLS.contains(&t.as_str()) && !have.contains(t));
        wanted.dedup();
        if wanted.is_empty() {
            return tools::ToolOutcome {
                ok: false,
                output: "Nothing to ask for: you already have those, or they can't be requested (team, models and \
                         settings stay with Iris)."
                    .into(),
                summary: "No new tools to ask for".into(),
            };
        }
        let reason = args["reason"].as_str().unwrap_or("the user asked for it");
        let (granted, declined, note) = crate::design::review_request(self.state, self.agent, &wanted, reason).await;
        if !granted.is_empty() {
            let mut all = self.agent.tools.clone();
            all.extend(self.granted.iter().cloned());
            all.extend(granted.iter().cloned());
            let patch = iris_core::AgentPatch { tools: Some(all), ..Default::default() };
            if let Ok(Some(agent)) = self.state.store.update_agent(&self.agent.id, patch) {
                self.state.emit(Event::AgentUpdated { agent });
            }
            self.granted.extend(granted.iter().cloned());
        }
        let mut output = if granted.is_empty() {
            "Iris granted nothing.".to_owned()
        } else {
            format!("Iris granted: {}. They work right away: do the task now.", granted.join(", "))
        };
        if !declined.is_empty() {
            output.push_str(&format!(" Declined: {}.", declined.join(", ")));
        }
        if !note.is_empty() {
            output.push_str(&format!(" Iris: {note}"));
        }
        let summary = match (granted.is_empty(), declined.is_empty()) {
            (false, true) => format!("Iris granted {}", granted.join(", ")),
            (false, false) => format!("Iris granted {} · declined {}", granted.join(", "), declined.join(", ")),
            _ => format!("Iris declined {}", declined.join(", ")),
        };
        tools::ToolOutcome { ok: !granted.is_empty(), output, summary }
    }

    /// The specialist (an idle, top-level agent with the same toolset) a
    /// coordinator should hand this call to, once per toolset per reply.
    fn specialist_for(&self, function: &str) -> Option<(&'static str, String)> {
        // Only the coordinator hands work to specialists.
        if self.agent.role != "main" {
            return None;
        }
        let toolset = tools::toolset_of(function)?;
        let family = SPECIALIST_FAMILIES.iter().find(|f| f.contains(&toolset))?;
        if self.handed_off.contains(&toolset) {
            return None;
        }
        let agents = self.state.store.list_agents().ok()?;
        agents
            .into_iter()
            .find(|a| {
                a.id != self.agent.id
                    && a.role != "main"
                    && a.created_by.is_empty()
                    && a.tools.iter().any(|t| family.contains(&t.as_str()))
                    && !self.state.is_agent_busy(&a.id)
            })
            .map(|a| (toolset, a.name))
    }
}

struct Reply<'a> {
    state: &'a AppState,
    agent: &'a Agent,
    thread_id: &'a str,
    reply_id: &'a str,
    rounds: Vec<Round>,
    /// Images an agent asked to see (view_image), shown to it next round.
    pending_images: Vec<String>,
    /// Toolsets a coordinator was already pointed to its specialist for
    /// (a second call goes through: the user wanted it done here).
    handed_off: Vec<&'static str>,
    /// Tools Iris granted during this reply (usable from the next step).
    granted: Vec<String>,
    /// Whether this reply has read web content (pages can carry prompt
    /// injections): lasting changes then ask the user, and Careful auto is stricter.
    read_web: bool,
    /// What the user's own tools returned in this reply (files, chats, command output…): what a
    /// planted instruction would want to carry out. Only used to spot it leaving (see `outside.rs`).
    private: String,
    /// The user asked for deep research (see [`RESEARCH_PREFIX`]).
    research: bool,
    /// The user is on a voice call (see [`VOICE_PREFIX`]): the reply is read aloud.
    voice: bool,
}

/// Toolsets that make an agent a specialist, in families of the same kind of
/// work: a coordinator reaching for one is pointed to a specialist that has
/// any toolset of that family (a health checker may use the shell or the
/// system tools).
const SPECIALIST_FAMILIES: &[&[&str]] = &[&["system", "shell"]];

/// Token usage and model across all rounds.
#[derive(Default)]
struct Totals {
    model: Option<String>,
    prompt_tokens: i64,
    completion_tokens: i64,
    reported: bool,
    /// The last model call's prompt + completion: how full the context is.
    context: Option<i64>,
}

/// The provider and model a reply is using right now.
struct Conn {
    provider_name: String,
    model: String,
    client: OpenAiCompatible,
    /// DeepSeek's thinking mode needs earlier reasoning sent back.
    deepseek: bool,
}

/// Local servers (Ollama, LM Studio) need no key.
fn is_local(base_url: &str) -> bool {
    base_url.contains("://localhost") || base_url.contains("://127.0.0.1")
}

/// The gist of a provider error, for a one-line notice.
fn short_error(e: &str) -> String {
    let first = e.lines().next().unwrap_or(e);
    tools::truncate(first, 120)
}

impl Reply<'_> {
    async fn generate(&mut self, cancel: &CancellationToken) -> MessageResult {
        let Some(primary) = self.state.resolve_model(self.agent) else {
            return self.finish(
                MessageStatus::Error,
                Some("No AI provider is set up yet. Add one in Settings → Providers.".into()),
                Totals::default(),
            );
        };
        // The fallback steps in once per reply if the main provider fails.
        let mut fallback = self.state.fallback_model().filter(|f| *f != primary);
        let mut conn = match self.connect(&primary) {
            Ok(c) => c,
            Err(msg) => match fallback.take().and_then(|f| self.connect(&f).ok()) {
                Some(c) => c,
                None => return self.finish(MessageStatus::Error, Some(msg), Totals::default()),
            },
        };
        // A chat close to the context limit is compacted before it continues.
        match crate::compact::compact(self.state, self.agent, self.thread_id, &conn.client, &conn.model, false).await {
            Ok(true) => self.state.set_activity(self.thread_id, "thinking", ""),
            Ok(false) => {}
            Err(e) => warn!("compacting {}'s chat failed: {e}", self.agent.name),
        }
        let latest_user = self
            .state
            .store
            .list_messages(self.thread_id)
            .ok()
            .and_then(|ms| ms.into_iter().rev().find(|m| m.role == MessageRole::User));
        self.research = latest_user.as_ref().is_some_and(|m| m.content.starts_with(RESEARCH_PREFIX));
        self.voice = latest_user.as_ref().is_some_and(|m| is_voice_message(&m.content));
        let mut history = match self.build_history(&conn) {
            Ok(h) => h,
            Err(e) => return self.finish(MessageStatus::Error, Some(format!("Could not load the conversation: {e}")), Totals::default()),
        };
        let ctx = ToolContext {
            http: self.state.http.clone(),
            workspace: self.state.data_dir.join("workspaces").join(&self.agent.id),
            search: self.state.search.read().unwrap().clone(),
        };
        let reasoning = Reasoning::parse(&self.state.reasoning_for(self.agent));
        let mut opts = ChatOptions { tools: self.specs(), reasoning, ..Default::default() };
        let mut totals = Totals::default();
        let max_rounds = if self.research { MAX_RESEARCH_ROUNDS } else { MAX_ROUNDS };

        for round in 0..max_rounds {
            if let Some(reason) = self.state.limit_reached(totals.prompt_tokens + totals.completion_tokens) {
                return self.finish(MessageStatus::Error, Some(reason), totals);
            }
            self.state.set_activity(self.thread_id, "thinking", "");
            self.rounds.push(Round::default());
            // Tools granted in the last step are offered from now on.
            opts.tools = self.specs();

            // ---- ask the model, streaming its output ----
            let (mut outcome, mut content, mut reasoning) = self.ask(&conn, &history, &opts, round, cancel).await;
            // Nothing streamed yet and the provider failed: retry on the fallback.
            if let Some(Err(e)) = &outcome {
                if content.is_empty() && reasoning.is_empty() {
                    if let Some(next) = fallback.take().and_then(|f| self.connect(&f).ok()) {
                        warn!("{} failed ({e}); falling back to {} {}", conn.provider_name, next.provider_name, next.model);
                        let notice = format!(
                            "_{} didn't respond ({}), so this answer comes from the fallback model {} ({})._\n\n",
                            conn.provider_name,
                            short_error(&e.to_string()),
                            next.model,
                            next.provider_name
                        );
                        conn = next;
                        history[0] = ChatMessage::new(Role::System, system_prompt(self.state, self.agent, self.thread_id, &conn, self.research, self.voice));
                        self.state.emit(Event::MessageDelta {
                            thread_id: self.thread_id.to_owned(),
                            message_id: self.reply_id.to_owned(),
                            round,
                            content: Some(notice.clone()),
                            reasoning: None,
                        });
                        let retry = self.ask(&conn, &history, &opts, round, cancel).await;
                        outcome = retry.0;
                        content = notice + &retry.1;
                        reasoning = retry.2;
                    }
                }
            }
            self.rounds[round].content = content.clone();
            self.rounds[round].reasoning = reasoning.clone();

            let done = match outcome {
                None => return self.finish(MessageStatus::Cancelled, None, totals),
                Some(Err(e)) => return self.finish(MessageStatus::Error, Some(e.to_string()), totals),
                Some(Ok(done)) => done,
            };
            if let Some(u) = &done.usage {
                if let Some(hit) = u.prompt_cache_hit_tokens {
                    info!("{}: {} prompt tokens, {hit} from cache", self.agent.name, u.prompt_tokens);
                }
                totals.prompt_tokens += i64::from(u.prompt_tokens);
                totals.completion_tokens += i64::from(u.completion_tokens);
                totals.reported = true;
                totals.context = Some(i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
            }
            totals.model = done.model.clone().or_else(|| Some(conn.model.clone()));

            // ---- a final answer ends the reply ----
            if done.tool_calls.is_empty() {
                return self.finish(MessageStatus::Complete, None, totals);
            }

            // ---- otherwise run the tools it asked for ----
            let calls: Vec<ToolCall> = done
                .tool_calls
                .into_iter()
                .map(|mut c| {
                    if c.id.is_empty() {
                        c.id = format!("call_{}", new_id());
                    }
                    c
                })
                .collect();
            history.push(ChatMessage::assistant(content, reasoning, calls.clone()));
            for call in &calls {
                let output = match self.run_tool(round, call, &ctx, &conn, cancel).await {
                    Some(output) => output,
                    None => return self.finish(MessageStatus::Cancelled, None, totals),
                };
                history.push(ChatMessage::tool_result(&call.id, output));
            }
            // A question was put to the user: the reply ends on it, and their
            // answer starts the next one.
            let asked = self.rounds[round].tools.iter().any(|t| t.name == "ask_user" && t.status == ToolStatus::Done);
            if asked {
                return self.finish(MessageStatus::Complete, None, totals);
            }
            // Images can only reach the model in a user message.
            if !self.pending_images.is_empty() {
                let images = std::mem::take(&mut self.pending_images);
                history.push(ChatMessage::user_with_images("[The image(s) you asked to view]", images));
            }
        }

        // ---- out of rounds: one last call, tools off, for a final answer ----
        if let Some(reason) = self.state.limit_reached(totals.prompt_tokens + totals.completion_tokens) {
            return self.finish(MessageStatus::Error, Some(reason), totals);
        }
        self.state.set_activity(self.thread_id, "thinking", "");
        self.rounds.push(Round::default());
        let round = self.rounds.len() - 1;
        history.push(ChatMessage::new(Role::User, WRAP_UP_NOTE));
        let final_opts = ChatOptions { no_tool_calls: true, ..opts.clone() };
        let (outcome, content, reasoning) = self.ask(&conn, &history, &final_opts, round, cancel).await;
        self.rounds[round].content = content;
        self.rounds[round].reasoning = reasoning;
        match outcome {
            None => self.finish(MessageStatus::Cancelled, None, totals),
            Some(Err(e)) => self.finish(MessageStatus::Error, Some(e.to_string()), totals),
            Some(Ok(done)) => {
                if let Some(u) = &done.usage {
                    totals.prompt_tokens += i64::from(u.prompt_tokens);
                    totals.completion_tokens += i64::from(u.completion_tokens);
                    totals.reported = true;
                    totals.context = Some(i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
                }
                if self.rounds[round].content.trim().is_empty() {
                    return self.finish(
                        MessageStatus::Error,
                        Some(format!("Used all {max_rounds} tool steps without reaching an answer.")),
                        totals,
                    );
                }
                self.finish(MessageStatus::Complete, None, totals)
            }
        }
    }

    /// The client for a provider and model, or why it can't be used.
    fn connect(&self, (provider_id, model): &(String, String)) -> Result<Conn, String> {
        let providers = self.state.providers.read().unwrap();
        let Some(p) = providers.get(provider_id) else {
            return Err(format!("Unknown provider {provider_id:?}. Check Settings → Providers."));
        };
        if p.key_source == crate::providers::KeySource::None && !is_local(&p.base_url) {
            return Err(format!("No API key is set for {}. Add your key in Settings → Providers.", p.name));
        }
        Ok(Conn {
            provider_name: p.name.clone(),
            model: model.clone(),
            client: p.client.clone(),
            deepseek: p.base_url.contains("deepseek"),
        })
    }

    /// One model call, streaming its text and thinking to clients. The
    /// outcome is `None` if the reply was stopped meanwhile.
    async fn ask(
        &self,
        conn: &Conn,
        history: &[ChatMessage],
        opts: &ChatOptions,
        round: usize,
        cancel: &CancellationToken,
    ) -> (Option<Result<Completion, ProviderError>>, String, String) {
        // Only DeepSeek wants its earlier reasoning back; others may reject it.
        let stripped: Vec<ChatMessage>;
        let messages = if conn.deepseek {
            history
        } else {
            stripped = history
                .iter()
                .cloned()
                .map(|mut m| {
                    m.reasoning_content.clear();
                    m
                })
                .collect();
            &stripped
        };
        let mut content = String::new();
        let mut reasoning = String::new();
        let outcome = {
            let stream = conn.client.stream_chat(&conn.model, messages, opts, |delta| {
                let (text, thinking) = match delta {
                    Delta::Content(s) => {
                        content.push_str(&s);
                        (Some(s), None)
                    }
                    Delta::Reasoning(s) => {
                        reasoning.push_str(&s);
                        (None, Some(s))
                    }
                    Delta::ToolCallStarted { .. } => return,
                };
                self.state.emit(Event::MessageDelta {
                    thread_id: self.thread_id.to_owned(),
                    message_id: self.reply_id.to_owned(),
                    round,
                    content: text,
                    reasoning: thinking,
                });
            });
            tokio::pin!(stream);
            tokio::select! {
                result = &mut stream => Some(result),
                _ = cancel.cancelled() => None,
            }
            // The stream (and its borrow of `content`/`reasoning`) ends here.
        };
        (outcome, content, reasoning)
    }

    /// Runs one tool call (asking for approval first if needed) and returns
    /// the text for the model. `None` means the reply was cancelled meanwhile.
    async fn run_tool(
        &mut self,
        round: usize,
        call: &ToolCall,
        ctx: &ToolContext,
        conn: &Conn,
        cancel: &CancellationToken,
    ) -> Option<String> {
        let mut started = now_ms();
        self.rounds[round].tools.push(ToolStep {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            status: ToolStatus::Running,
            summary: tools::describe(call),
            output: String::new(),
            approval_id: None,
            auto_approved: None,
            started_at: started,
            finished_at: None,
        });
        let idx = self.rounds[round].tools.len() - 1;
        self.state.set_activity(self.thread_id, &call.name, &tools::describe(call));

        let allowed = self.specs().iter().any(|s| s.name == call.name);
        // A coordinator about to do a specialist's job: suggest handing it over.
        if allowed {
            if let Some((toolset, specialist)) = self.specialist_for(&call.name) {
                self.handed_off.push(toolset);
                let text = format!(
                    "Not run: {specialist} is your specialist for this. Hand the task to {specialist} with \
                     delegate_task, tell the user in one line, and report its result when the update arrives. Only \
                     if the user asked you to do it yourself, call this tool again."
                );
                self.end_step(round, idx, ToolStatus::Error, format!("{specialist} covers this · handing it over"), text.clone(), started);
                return Some(text);
            }
        }
        // Auto mode: a safety check may approve a command instead of the user.
        let mut auto = None;
        // After outside content was read, Full auto stops skipping the command review.
        let mode = crate::outside::effective_mode(self.state.auto_mode_for(self.agent), self.read_web);
        if allowed && call.name == "run_command" && mode != crate::api::AutoMode::Off {
            self.state.set_activity(self.thread_id, "reviewing", &tools::describe(call));
            let command = serde_json::from_str::<serde_json::Value>(&call.arguments)
                .ok()
                .and_then(|v| v["command"].as_str().map(str::to_owned))
                .unwrap_or_default();
            let task = self.task();
            let full = mode == crate::api::AutoMode::Full;
            let review = async {
                // Full auto: only the hard rules stop a command; no model call.
                if full {
                    match crate::autoapprove::hard_rule(&command) {
                        Some(why) => (crate::autoapprove::Verdict::Ask(format!("Needs you: {why}")), 0),
                        None => (crate::autoapprove::Verdict::Allow("full auto".into()), 0),
                    }
                } else {
                    crate::autoapprove::review(&conn.client, &conn.model, &ctx.workspace, &task, &command, self.read_web).await
                }
            };
            let (verdict, tokens) = tokio::select! {
                v = review => v,
                _ = cancel.cancelled() => {
                    self.end_step(round, idx, ToolStatus::Cancelled, "Stopped".into(), String::new(), started);
                    return None;
                }
            };
            let _ = self.state.store.add_usage(tokens);
            match verdict {
                crate::autoapprove::Verdict::Allow(reason) => auto = Some(reason),
                crate::autoapprove::Verdict::Ask(reason) => {
                    self.set_step(round, idx, |s| s.auto_approved = None);
                    info!("auto mode asks the user: {reason}");
                }
            }
            self.state.set_activity(self.thread_id, &call.name, &tools::describe(call));
        }
        if let Some(reason) = &auto {
            let reason = reason.clone();
            self.set_step(round, idx, |s| s.auto_approved = Some(reason));
        } else if allowed
            && (tools::needs_approval(&call.name)
                || (self.read_web && tools::LASTING_FUNCTIONS.contains(&call.name.as_str()))
                || (call.name.starts_with(tools::MCP_PREFIX) && self.state.mcp.needs_approval(&self.state.data_dir, &call.name))
                || crate::develop::needs_approval(&call.name)
                || crate::system::needs_approval(call, mode)
                || crate::routines::needs_approval(call, mode)
                || crate::settings::needs_approval(call, mode)
                || self.leak_note(call).is_some()
                || crate::browser::needs_approval(self.state, &self.agent.id, call, mode).await)
        {
            self.state.set_activity(self.thread_id, "waiting", &tools::describe(call));
            match self.wait_for_approval(round, idx, call, cancel).await {
                Some(true) => {
                    self.state.set_activity(self.thread_id, &call.name, &tools::describe(call));
                    // Time the command itself, not how long it waited for the user.
                    started = now_ms();
                    self.set_step(round, idx, |s| {
                        s.status = ToolStatus::Running;
                        s.started_at = started;
                    });
                }
                Some(false) => {
                    let text = "The user denied this action. Do not retry it; ask them or continue without it.".to_owned();
                    self.end_step(round, idx, ToolStatus::Denied, "Denied by you".into(), text.clone(), started);
                    return Some(text);
                }
                None => {
                    self.end_step(round, idx, ToolStatus::Cancelled, "Stopped".into(), String::new(), started);
                    return None;
                }
            }
        } else {
            self.publish();
        }

        let outcome = if allowed && call.name == "ask_user" {
            tools::ask_user(call)
        } else if allowed && call.name == "request_tools" {
            self.state.set_activity(self.thread_id, "request_tools", "Asking Iris for tools");
            self.request_tools(call).await
        } else if allowed && call.name == "view_image" {
            let (outcome, image) = self.view_image(ctx, call);
            self.pending_images.extend(image);
            outcome
        } else if allowed {
            // Team and model tools need the whole engine.
            let (st, me) = (self.state.clone(), self.agent.clone());
            let tainted = self.read_web;
            let work = async {
                if tools::TEAM_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::team::run(&st, &me, self.thread_id, call, tainted).await
                } else if tools::ROUTINE_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::routines::tool(&st, &me, call).await
                } else if call.name == "list_accounts" {
                    let list: Vec<String> = crate::vault::load(&st.data_dir)
                        .iter()
                        .map(|a| format!("- {} ({})", a.name, a.site))
                        .collect();
                    tools::ToolOutcome {
                        ok: true,
                        output: if list.is_empty() {
                            "No saved accounts. The user can add them in Settings → Accounts.".into()
                        } else {
                            format!("Saved accounts (use browser_login on the site's sign-in page):\n{}", list.join("\n"))
                        },
                        summary: format!("Checked saved accounts · {}", list.len()),
                    }
                } else if tools::BROWSER_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::browser::run(&st, &me.id, call).await
                } else if tools::SYSTEM_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::system::run(call).await
                } else if call.name == "memory" {
                    crate::memory::run(&st, &me, call)
                } else if call.name.starts_with(tools::MCP_PREFIX) {
                    st.mcp.call(call).await
                } else if crate::develop::FUNCTIONS.contains(&call.name.as_str()) {
                    crate::develop::run(&st, &me, self.thread_id, call).await
                } else if call.name == "search_chats" {
                    crate::recall::run(&st, call)
                } else if call.name == "use_skill" || call.name == "save_skill" {
                    crate::skills::run(&st.data_dir, &me.name, call)
                } else if tools::MODEL_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::modelctl::run(&st, &me, call).await
                } else if tools::SETTINGS_FUNCTIONS.contains(&call.name.as_str()) {
                    crate::settings::run(&st, &me, call).await
                } else {
                    tools::run(ctx, call).await
                }
            };
            tokio::select! {
                o = work => o,
                _ = cancel.cancelled() => {
                    self.end_step(round, idx, ToolStatus::Cancelled, "Stopped".into(), String::new(), started);
                    return None;
                }
            }
        } else {
            tools::ToolOutcome {
                ok: false,
                output: format!("Error: the tool `{}` is not available to you.", call.name),
                summary: format!("Unavailable tool {}", call.name),
            }
        };
        // Connected services' results are outside content too (emails, issues…).
        // (And a finished task from an agent that had read outside content.)
        if crate::outside::taints(&call.name, outcome.ok, &outcome.output) {
            self.read_web = true;
        }
        let output = tools::truncate(&outcome.output, MAX_TOOL_OUTPUT_CHARS);
        // What the user's own tools returned, kept to spot it leaving in a web address (capped).
        if outcome.ok && crate::outside::is_private_tool(&call.name) && self.private.len() < 400_000 {
            self.private.push('\n');
            self.private.push_str(&output);
        }
        // Results from outside carry a label for the model (and show it in the step).
        let output = crate::outside::label(&call.name, outcome.ok, output);
        let status = if outcome.ok { ToolStatus::Done } else { ToolStatus::Error };
        let summary = if auto.is_some() { format!("{} · auto-approved", outcome.summary) } else { outcome.summary };
        self.end_step(round, idx, status, summary, output.clone(), started);
        Some(output)
    }

    /// Loads an image file for the model to look at (any folder except the
    /// app's own data, where the keys are).
    fn view_image(&self, ctx: &ToolContext, call: &ToolCall) -> (tools::ToolOutcome, Option<String>) {
        let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap_or_default();
        let raw = args["path"].as_str().unwrap_or_default().trim();
        let path = if std::path::Path::new(raw).is_absolute() { std::path::PathBuf::from(raw) } else { ctx.workspace.join(raw) };
        let fail = |msg: String| (tools::ToolOutcome { ok: false, output: format!("Error: {msg}"), summary: format!("Couldn't view {raw}") }, None);
        let real = match std::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(e) => return fail(format!("can't open {raw}: {e}")),
        };
        let data_dir = std::fs::canonicalize(&*self.state.data_dir).unwrap_or_else(|_| (*self.state.data_dir).clone());
        let in_workspace = std::fs::canonicalize(&ctx.workspace).is_ok_and(|w| real.starts_with(w));
        if real.starts_with(&data_dir) && !in_workspace {
            return fail("that folder holds the app's own data and keys".into());
        }
        if crate::attachments::kind_of(&real.to_string_lossy()) != "image" {
            return fail("only images (png, jpg, gif, webp) can be viewed; read text files with read_file or a command".into());
        }
        match crate::attachments::image_data_url(&real) {
            Some(url) => (
                tools::ToolOutcome {
                    ok: true,
                    output: format!("{} is shown to you in the next message.", real.display()),
                    summary: format!("Looked at {}", real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                },
                Some(url),
            ),
            None => fail("the image is unreadable or larger than 20 MB".into()),
        }
    }

    /// A user message's text with its attached files: text files inline,
    /// images as images (or a note, for older ones).
    fn with_attachments(&self, m: &Message, images: bool, full: bool) -> (String, Vec<String>) {
        let mut text = m.content.clone();
        let mut urls = Vec::new();
        for a in &m.attachments {
            let path = crate::attachments::path_of(&self.state.data_dir, self.thread_id, a);
            match a.kind.as_str() {
                "image" if images => match crate::attachments::image_data_url(&path) {
                    Some(url) => {
                        text.push_str(&format!("\n\n[Attached image: {}]", a.name));
                        urls.push(url);
                    }
                    None => text.push_str(&format!("\n\n[Attached image {} could not be read]", a.name)),
                },
                "image" => text.push_str(&format!("\n\n[Image {} was attached earlier]", a.name)),
                "text" | "pdf" => match crate::attachments::text_of(&path, &a.kind, full) {
                    Some(body) => text.push_str(&format!("\n\n[Attached file: {}]\n```\n{body}\n```", a.name)),
                    None => text.push_str(&format!("\n\n[Attached file {} could not be read]", a.name)),
                },
                _ => text.push_str(&format!(
                    "\n\n[Attached file {} ({} bytes), saved at {}]",
                    a.name,
                    a.size,
                    path.display()
                )),
            }
        }
        (text, urls)
    }

    /// What this conversation is about: its first user message (for a
    /// delegated task, the task itself).
    fn task(&self) -> String {
        self.state
            .store
            .list_messages(self.thread_id)
            .ok()
            .and_then(|m| m.into_iter().find(|m| m.role == MessageRole::User))
            .map(|m| m.content)
            .unwrap_or_default()
    }

    /// Pauses until the user approves or denies the step. `None` if the reply
    /// is stopped first.
    /// Why this call needs the user: it would carry the user's own text (from their files,
    /// chats, memory or services) out in a web address or a form, in a reply that has read
    /// outside content. That is how a planted instruction sends data away. (A heuristic: it
    /// doesn't see text that was encoded first.)
    fn leak_note(&self, call: &ToolCall) -> Option<String> {
        if !self.read_web {
            return None;
        }
        let args: serde_json::Value = serde_json::from_str(&call.arguments).ok()?;
        let candidate = match call.name.as_str() {
            "web_fetch" | "browser_open" => args["url"].as_str()?,
            "browser_type" => args["text"].as_str()?,
            _ => return None,
        };
        let mut private = self.private.clone();
        // What the agent remembers about the user, and the summary of earlier chat, count too.
        for text in [
            crate::memory::read(&crate::memory::memory_path(&self.state.data_dir, &self.agent.id)),
            crate::memory::read(&crate::memory::user_path(&self.state.data_dir)),
            self.state.store.get_thread(self.thread_id).ok().flatten().map(|t| t.summary).unwrap_or_default(),
        ] {
            private.push('\n');
            private.push_str(&text);
        }
        crate::outside::leaks(candidate, &private).then(|| {
            "This contains text that also appears in your files, chats, memory or connected services, in a reply that has read \
             web pages or connected services. A planted instruction may be sending it away. Approve only if you meant this."
                .to_owned()
        })
    }

    async fn wait_for_approval(&mut self, round: usize, idx: usize, call: &ToolCall, cancel: &CancellationToken) -> Option<bool> {
        let leak = self.leak_note(call);
        let approval = PendingApproval {
            id: new_id(),
            agent_id: self.agent.id.clone(),
            thread_id: self.thread_id.to_owned(),
            message_id: self.reply_id.to_owned(),
            tool: call.name.clone(),
            arguments: call.arguments.clone(),
            summary: tools::describe(call),
            created_at: now_ms(),
            // Code changes show the user the whole diff; a possible leak says so.
            detail: crate::develop::approval_detail(call).await.or(leak),
        };
        let (tx, rx) = oneshot::channel();
        self.state.approvals.lock().unwrap().insert(approval.id.clone(), (approval.clone(), tx));
        self.set_step(round, idx, |s| {
            s.status = ToolStatus::AwaitingApproval;
            s.approval_id = Some(approval.id.clone());
        });
        self.state.emit(Event::ApprovalRequested { approval: approval.clone() });

        let decision = tokio::select! {
            d = rx => Some(d.unwrap_or(false)),
            _ = cancel.cancelled() => None,
        };
        self.state.approvals.lock().unwrap().remove(&approval.id);
        self.state.emit(Event::ApprovalResolved {
            approval_id: approval.id.clone(),
            agent_id: self.agent.id.clone(),
            approved: decision == Some(true),
        });
        self.set_step(round, idx, |s| s.approval_id = None);
        decision
    }

    fn set_step(&mut self, round: usize, idx: usize, f: impl FnOnce(&mut ToolStep)) {
        f(&mut self.rounds[round].tools[idx]);
        self.publish();
    }

    fn end_step(&mut self, round: usize, idx: usize, status: ToolStatus, summary: String, output: String, started: i64) {
        let finished = now_ms();
        let step = &mut self.rounds[round].tools[idx];
        step.status = status;
        step.summary = summary;
        step.output = output;
        step.finished_at = Some(finished);
        let entry = ToolLogEntry {
            id: new_id(),
            agent_id: self.agent.id.clone(),
            thread_id: self.thread_id.to_owned(),
            message_id: self.reply_id.to_owned(),
            tool: step.name.clone(),
            arguments: step.arguments.clone(),
            status,
            summary: step.summary.clone(),
            started_at: started,
            duration_ms: finished - started,
        };
        if let Err(e) = self.state.store.log_tool(&entry) {
            warn!("writing the tool log: {e}");
        }
        self.publish();
    }

    /// Saves the rounds so far and tells clients.
    fn publish(&self) {
        if let Err(e) = self.state.store.save_rounds(self.reply_id, &self.rounds) {
            warn!("saving reply progress: {e}");
        }
        self.state.emit(Event::MessageProgress {
            thread_id: self.thread_id.to_owned(),
            message_id: self.reply_id.to_owned(),
            rounds: self.rounds.clone(),
        });
    }

    fn finish(&mut self, status: MessageStatus, error: Option<String>, totals: Totals) -> MessageResult {
        // Steps still open when the reply ends can never finish.
        for step in self.rounds.iter_mut().flat_map(|r| r.tools.iter_mut()) {
            if matches!(step.status, ToolStatus::Running | ToolStatus::AwaitingApproval) {
                step.status = ToolStatus::Cancelled;
                step.approval_id = None;
            }
        }
        // Drop a trailing empty round (e.g. cancelled before any output).
        if self.rounds.last().is_some_and(|r| r.content.is_empty() && r.reasoning.is_empty() && r.tools.is_empty()) {
            self.rounds.pop();
        }
        let join = |f: fn(&Round) -> &str| {
            self.rounds.iter().map(f).filter(|s| !s.trim().is_empty()).collect::<Vec<_>>().join("\n\n")
        };
        MessageResult {
            content: join(|r| &r.content),
            reasoning: join(|r| &r.reasoning),
            rounds: std::mem::take(&mut self.rounds),
            status,
            error,
            model: totals.model,
            prompt_tokens: totals.reported.then_some(totals.prompt_tokens),
            completion_tokens: totals.reported.then_some(totals.completion_tokens),
            context_tokens: totals.context,
        }
    }

    /// The conversation as the provider expects it, replaying earlier replies
    /// round by round (with their reasoning and tool calls, which DeepSeek's
    /// thinking mode requires when tools are in use).
    fn build_history(&self, conn: &Conn) -> iris_core::store::Result<Vec<ChatMessage>> {
        // Messages already compacted into the summary aren't sent again.
        let since = self.state.store.get_thread(self.thread_id)?.map_or(0, |t| t.summary_until);
        let past: Vec<Message> = self
            .state
            .store
            .list_messages(self.thread_id)?
            .into_iter()
            .filter(|m| m.id != self.reply_id && m.created_at > since)
            .filter(|m| match m.role {
                MessageRole::User => true,
                // Failed replies would only confuse the model.
                MessageRole::Assistant => matches!(m.status, MessageStatus::Complete | MessageStatus::Cancelled),
                MessageRole::System => false,
            })
            .collect();
        let skip = past.len().saturating_sub(MAX_HISTORY);

        let mut history = Vec::with_capacity(MAX_HISTORY * 2 + 1);
        history.push(ChatMessage::new(Role::System, system_prompt(self.state, self.agent, self.thread_id, conn, self.research, self.voice)));
        // Attachments: images from the last few messages that have them, the
        // full text of files in the last two; older ones shortened.
        let with_files: Vec<&str> =
            past.iter().rev().filter(|m| !m.attachments.is_empty()).map(|m| m.id.as_str()).collect();
        let recent_images: Vec<String> = with_files.iter().take(RECENT_IMAGE_MESSAGES).map(|s| s.to_string()).collect();
        let recent_files: Vec<String> = with_files.iter().take(2).map(|s| s.to_string()).collect();
        for m in past.into_iter().skip(skip) {
            match m.role {
                MessageRole::User if !m.attachments.is_empty() => {
                    let (text, images) = self.with_attachments(&m, recent_images.contains(&m.id), recent_files.contains(&m.id));
                    history.push(ChatMessage::user_with_images(text, images));
                }
                MessageRole::User => history.push(ChatMessage::new(Role::User, m.content)),
                MessageRole::Assistant if m.rounds.is_empty() => {
                    if !m.content.is_empty() {
                        history.push(ChatMessage::assistant(m.content, m.reasoning, Vec::new()));
                    }
                }
                MessageRole::Assistant => {
                    for round in m.rounds {
                        // Only steps that finished have a result to send back,
                        // and every tool call needs one.
                        let steps: Vec<ToolStep> = round
                            .tools
                            .into_iter()
                            .filter(|s| !matches!(s.status, ToolStatus::Running | ToolStatus::AwaitingApproval))
                            .collect();
                        if steps.is_empty() && round.content.is_empty() && round.reasoning.is_empty() {
                            continue;
                        }
                        let calls = steps
                            .iter()
                            .map(|s| ToolCall { id: s.id.clone(), name: s.name.clone(), arguments: s.arguments.clone() })
                            .collect();
                        history.push(ChatMessage::assistant(round.content, round.reasoning, calls));
                        for s in steps {
                            // The model already used this; a short version keeps the gist.
                            let output = if s.output.is_empty() {
                                format!("({})", s.status.as_str())
                            } else {
                                tools::truncate(&s.output, OLD_TOOL_OUTPUT_CHARS)
                            };
                            history.push(ChatMessage::tool_result(s.id, output));
                        }
                    }
                }
                MessageRole::System => {}
            }
        }
        Ok(history)
    }
}

/// The agent's instructions plus what it needs to know about its situation:
/// the date, which model it runs on, the user's preferred style and its tools.
fn system_prompt(state: &AppState, agent: &Agent, thread_id: &str, conn: &Conn, research: bool, voice: bool) -> String {
    // Order matters for cost: providers cache the unchanging start of a
    // request, so stable text comes first and what changes (memory, summary,
    // the clock) comes last.
    let mut prompt = agent.system_prompt.trim().to_owned();
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    let has = |t: &str| tools::effective_tools(agent).iter().any(|x| x == t);
    // The coordinator's specialists, right after its own instructions so the
    // rule to use them isn't lost among the rest.
    if agent.role == "main" {
        let team: Vec<String> = state
            .store
            .list_agents()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.id != agent.id && a.role != "main" && a.role != crate::design::BLANK_ROLE && a.created_by.is_empty())
            .map(|a| format!("- {}: {}", a.name, if a.description.is_empty() { "no description" } else { &a.description }))
            .collect();
        if !team.is_empty() {
            prompt.push_str(&format!(
                "Most important rule: your specialists are\n{}\nWhen a request falls in a specialist's area, even \
                 a quick or simple one and even if you have the tools to do it yourself, don't do it yourself: \
                 hand it to that specialist (delegate_task), tell the user in one line who is on it, and report its \
                 result when the update arrives. Do it yourself only if no specialist fits, it is busy, or the user \
                 says to.\n\n",
                team.join("\n")
            ));
        }
    }
    prompt.push_str(&format!(
        "You are {name} in the user's I.R.I.S. app, running on {provider}'s model \"{model}\"; if asked what model \
         you are, say exactly that. Be brief: answer directly, without preambles, filler, recaps or offers of more \
         help, and go into detail only when the task needs it. Think only as much as the task needs.",
        name = agent.name,
        model = conn.model,
        provider = conn.provider_name,
    ));
    if let Some((p, m)) = state.fallback_model() {
        prompt.push_str(&format!(" If that provider fails, replies switch to the fallback {m} ({}).", state.provider_name(&p)));
    }
    prompt.push_str(" Use tools when they make the answer more accurate; never invent their results.");
    prompt.push_str(crate::outside::PROMPT_RULE);
    if agent.role != "main" {
        prompt.push_str(
            " If the user asks for something your tools can't do, don't refuse: ask Iris for the tools you need \
             (request_tools), then do it.",
        );
    }
    if has("models") {
        prompt.push_str(" When the user asks, list_models shows the setup and switch_model changes it.");
    }
    if has("settings") {
        prompt.push_str(
            " You can see every setting (get_settings) and change them when the user asks (change_setting); \
             look before answering questions about the setup. Risky changes ask the user by themselves.",
        );
    }
    if agent.role != "main" {
        prompt.push_str(
            "\nTeam: you can ask other agents or Iris for help (list_agents, then delegate_task with a complete, \
             self-contained task: they can't see this chat). Don't wait: tell the user in one line, end your reply, \
             and when the [I.R.I.S. update] arrives, read the result with check_task and report.",
        );
    } else {
        prompt.push_str(
            "\nTeam: agents you create are one-off unless you pass keep=true (do that when the user asked for the \
             agent or it's for recurring work). One-off agents are removed after the reply in which you collect \
             their result, so put what matters into your answer; call keep_agent if a result needs the user's \
             input. Don't wait for delegated work: after delegate_task, tell the user in one short line who is on \
             it and end your reply, so they can keep talking to you. When a task finishes you get an automatic \
             [I.R.I.S. update]; then collect the result with check_task and report. If an agent waits for the \
             user's approval, say so in one line and end your reply.",
        );
    }
    // The user's chosen style overrides the tone above; it's for talking to
    // the user, not for agents reporting to another agent.
    if agent.created_by.is_empty() {
        if let Some(style) = state.style.read().unwrap().instructions() {
            prompt.push_str(&format!("\n\nThe user's chosen response style (overrides the above):\n{style}"));
        }
    }
    prompt.push_str(
        " When the user refers to something from an earlier conversation that isn't in front of you, look it up \
         with search_chats.",
    );
    prompt.push_str(&crate::skills::prompt_section(&state.data_dir));
    prompt.push_str(&crate::memory::prompt_section(&state.data_dir, agent));
    // One endless chat: what was compacted away comes back as a summary.
    if let Ok(Some(thread)) = state.store.get_thread(thread_id) {
        if !thread.summary.trim().is_empty() {
            prompt.push_str(&format!(
                "\n\nSummary of the earlier conversation (compacted; treat it as memory):\n{}",
                thread.summary.trim()
            ));
        }
    }
    prompt.push_str(mode_instructions(research, voice));
    prompt.push_str(&format!("\n\nNow: {}.", tools::now_line()));
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, role: &str, tools: &[(&str, &str)]) -> Message {
        let steps: Vec<_> = tools
            .iter()
            .enumerate()
            .map(|(i, (name, status))| {
                serde_json::json!({ "id": format!("c{i}"), "name": name, "arguments": "{}", "status": status })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": id, "thread_id": "t", "role": role, "content": "", "reasoning": "",
            "rounds": [{ "tools": steps }], "status": "complete", "error": null, "model": null,
            "prompt_tokens": null, "completion_tokens": null, "context_tokens": null, "created_at": 0
        }))
        .unwrap()
    }

    #[test]
    fn the_voice_prefix_turns_the_call_instructions_on() {
        assert!(is_voice_message("[Voice call] what's the weather like?"));
        assert!(is_voice_message("[Voice call] τι ώρα είναι;"));
        assert_eq!(mode_instructions(false, true), VOICE_INSTRUCTIONS);
        assert!(VOICE_INSTRUCTIONS.contains("[details]"));
        // Calls are English and Greek only.
        assert!(VOICE_INSTRUCTIONS.contains("only English or Greek"));
    }

    #[test]
    fn a_normal_message_gets_no_call_instructions() {
        assert!(!is_voice_message("what's the weather like?"));
        // The prefix only counts at the very start of the message.
        assert!(!is_voice_message("I said [Voice call] earlier"));
        assert!(!is_voice_message("[Deep research] solar panels"));
        assert_eq!(mode_instructions(false, false), "");
        // Deep research keeps its own instructions.
        assert_eq!(mode_instructions(true, false), RESEARCH_INSTRUCTIONS);
    }

    #[test]
    fn reading_the_web_carries_over_to_the_next_reply() {
        let web = reply("a1", "assistant", &[("web_fetch", "done")]);
        let user = reply("u2", "user", &[]);
        let current = reply("a2", "assistant", &[]);
        // The reply before this one read a page: this one starts out cautious.
        assert!(read_outside_content_before(&[web.clone(), user.clone(), current.clone()], "a2"));
        // Connected services count too.
        let mcp = reply("a1", "assistant", &[("mcp__mail__list", "done")]);
        assert!(read_outside_content_before(&[mcp, user.clone(), current.clone()], "a2"));
        // A failed read brought nothing in; ordinary tools aren't outside content.
        let failed = reply("a1", "assistant", &[("web_fetch", "error")]);
        assert!(!read_outside_content_before(&[failed, user.clone(), current.clone()], "a2"));
        let plain = reply("a1", "assistant", &[("read_file", "done")]);
        assert!(!read_outside_content_before(&[plain, user.clone(), current.clone()], "a2"));
        // Only the reply right before counts, so it fades after one clean reply.
        let clean = reply("a1b", "assistant", &[("get_time", "done")]);
        assert!(!read_outside_content_before(&[web, user.clone(), clean, user, current], "a2"));
        assert!(!read_outside_content_before(&[], "a2"));
    }

    #[test]
    fn a_task_from_an_agent_that_read_outside_content_starts_out_cautious() {
        let mut task = reply("u1", "user", &[]);
        task.content = format!("[Task from Iris — your team's coordinator]\n\nResearch X\n\n{}\n\nWhen you are done, reply.", crate::outside::TASK_NOTE);
        let current = reply("a1", "assistant", &[]);
        assert!(read_outside_content_before(&[task, current.clone()], "a1"), "the note on the task carries over");
        // An ordinary task from a coordinator that had read nothing doesn't.
        let mut plain = reply("u1", "user", &[]);
        plain.content = "[Task from Iris — your team's coordinator]\n\nResearch X".into();
        assert!(!read_outside_content_before(&[plain, current], "a1"));
    }
}
