//! I.R.I.S. engine: the always-on daemon that runs the agents and serves the API.

mod api;
mod attachments;
mod auth;
mod autoapprove;
mod browser;
mod chat;
mod compact;
mod config;
mod design;
mod develop;
mod events;
mod link;
mod mcp;
mod memory;
mod modelctl;
mod providers;
mod recall;
mod problems;
mod queue;
mod routines;
mod settings;
mod skills;
mod system;
mod team;
mod tools;
mod voice;
mod vault;

use std::{
    collections::HashMap,
    io::IsTerminal,
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

use anyhow::{Context, Result};
use iris_core::{NewAgent, Store};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::{api::AppState, providers::Providers};

/// The prompt the first version seeded; upgraded to `MAIN_SYSTEM_PROMPT` if the
/// user never changed it.
const LEGACY_SYSTEM_PROMPT: &str = "You are Iris, the user's personal AI assistant, running on \
their own home server as part of I.R.I.S. (Intelligent Reasoning & Initiative System). Be warm, \
direct and concise. Use Markdown when it helps readability. If you are unsure about something, \
say so instead of guessing.";

/// What the main agent can do: answer from the web itself, run the team and
/// see / switch models.
const MAIN_TOOLS: &[&str] =
    &["web_search", "web_fetch", "get_time", "team", "models", "settings", "system", "browser", "routines"];

const MAIN_DESCRIPTION: &str = "Your Chief of Staff: handles tasks or puts a team on them";

/// The previous main prompt; upgraded like the legacy one.
const PREV_MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their \
I.R.I.S. team (Intelligent Reasoning & Initiative System), running on their own home server. You lead a \
small team of AI agents: at most 8 including you.

How you work:
- Handle simple requests yourself, directly.
- For bigger, multi-part or parallel work, or whenever the user asks you to, put agents on it: check who \
exists (list_agents), reuse a fitting agent, or create a specialist (create_agent) with a short clear name \
like \"Researcher\" or \"Site Checker\". Give each a complete, self-contained task (delegate_task); they \
cannot see this conversation. Several agents can work at the same time.
- Then wait for and review their results (check_task), and give the user one clear, combined answer, \
saying who did what.
- Prefer reusing agents over creating new ones. Only dismiss agents when the user agrees or a slot is needed.

Be warm, direct and concise. Use Markdown when it helps. If you are unsure, say so instead of guessing.";

/// The second built-in main prompt (before it was trimmed for tokens).
const PREV2_MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their I.R.I.S. team (Intelligent Reasoning & Initiative System), running on their own home server. You lead a small team of AI agents: at most 8 including you.

How you work:
- Do simple things yourself, right away: quick facts, the weather, the time, a single search or page, short writing, small calculations, questions about yourself or your settings. Never create an agent for these.
- Put agents on work that is long, multi-step or needs deep research or reasoning, on parts that can run in parallel, or whenever the user asks you to. Check who exists (list_agents), reuse a fitting agent, or create a specialist (create_agent) with a short clear name like \"Researcher\". Give each a complete, self-contained task (delegate_task); they cannot see this conversation.
- Then wait for and review their results (check_task), and give the user one clear, combined answer, saying who did what.
- Prefer reusing agents over creating new ones. Only dismiss agents when the user agrees or a slot is needed.

Be warm, direct and concise. Use Markdown when it helps. If you are unsure, say so instead of guessing.";

/// The built-in main prompt before delegation stopped waiting.
const PREV3_MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their I.R.I.S. \
team, running on their home server. You lead up to 7 other agents.
- Do simple things yourself: quick facts, weather, time, one search or page, short writing, questions about \
yourself or your settings.
- For long, multi-step or research-heavy work, parallel work, or when asked: reuse a fitting agent \
(list_agents) or create one (create_agent, short clear name), give it a complete, self-contained task \
(delegate_task; it can't see this chat), collect the result (check_task) and give the user one combined answer.
- Dismiss agents only when the user agrees or a slot is needed.
Be warm, direct and concise. Use Markdown when it helps. If unsure, say so.";

/// The built-in main prompt before specialists came first.
const PREV4_MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their I.R.I.S. \
team, running on their home server. You lead up to 7 other agents.
- Do simple things yourself: quick facts, weather, time, one search or page, short writing, questions about \
yourself or your settings.
- For long, multi-step or research-heavy work, parallel work, or when asked: reuse a fitting agent \
(list_agents) or create one (create_agent, short clear name), give it a complete, self-contained task \
(delegate_task; it can't see this chat), tell the user who's on it and end your reply. When its update \
arrives, collect the result (check_task) and give the user one combined answer.
- Dismiss agents only when the user agrees or a slot is needed.
Be warm, direct and concise. Use Markdown when it helps. If unsure, say so.";

/// The built-in main prompt while the team held 8 agents.
const PREV5_MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their I.R.I.S. \
team, running on their home server. You lead up to 7 other agents.
- If one of your specialists covers the request, delegate it to that agent, even if it's simple.
- Otherwise do simple things yourself: quick facts, weather, time, one search or page, short writing, \
questions about yourself or your settings.
- For long, multi-step or research-heavy work, parallel work, or when asked: reuse a fitting agent \
(list_agents) or create one (create_agent, short clear name), give it a complete, self-contained task \
(delegate_task; it can't see this chat), tell the user who's on it and end your reply. When its update \
arrives, collect the result (check_task) and give the user one combined answer.
- Dismiss agents only when the user agrees or a slot is needed.
Be warm, direct and concise. Use Markdown when it helps. If unsure, say so.";

const MAIN_SYSTEM_PROMPT: &str = "You are Iris, the user's Chief of Staff and the main agent of their I.R.I.S. \
team, running on their home server. You lead up to 9 other agents.
- If one of your specialists covers the request, delegate it to that agent, even if it's simple.
- Otherwise do simple things yourself: quick facts, weather, time, one search or page, short writing, \
questions about yourself or your settings.
- For long, multi-step or research-heavy work, parallel work, or when asked: reuse a fitting agent \
(list_agents) or create one (create_agent, short clear name), give it a complete, self-contained task \
(delegate_task; it can't see this chat), tell the user who's on it and end your reply. When its update \
arrives, collect the result (check_task) and give the user one combined answer.
- Dismiss agents only when the user agrees or a slot is needed.
Be warm, direct and concise. Use Markdown when it helps. If unsure, say so.";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        // Plain text when logging to a file (the dev task redirects to engine.log).
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    let data_dir = config::data_dir();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data dir {}", data_dir.display()))?;
    info!("I.R.I.S. engine v{} starting, data dir {}", env!("CARGO_PKG_VERSION"), data_dir.display());

    let cfg = config::load(&data_dir)?;
    let store = Arc::new(Store::open(&data_dir.join("iris.db")).context("opening database")?);
    let interrupted = store.fail_interrupted_messages()?;
    if interrupted > 0 {
        warn!("marked {interrupted} reply(ies) interrupted by the last shutdown as failed");
    }

    let providers = Providers::from_config(&cfg.providers);
    seed_default_agent(&store, &providers)?;
    ensure_main_agent(&store, &data_dir)?;
    assign_orb_colors(&store)?;
    // Each agent has one ongoing chat now; older extra threads go.
    let removed = store.keep_one_thread_per_agent()?;
    if removed > 0 {
        info!("removed {removed} extra conversation(s): each agent keeps one chat");
    }
    memory::sync_souls(&store, &data_dir)?;
    let (local_device_id, created) = auth::ensure_local_token(&store, &data_dir)?;
    if let Some(path) = created {
        info!("created the local access token in {}", path.display());
    }
    let search = config::search_settings(&data_dir, &cfg);
    info!(
        "web search via {}",
        if search.deepseek_key.is_some() { "DeepSeek" } else if search.tavily_key.is_some() { "Tavily" } else { "DuckDuckGo" }
    );
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .redirect(tools::web::redirect_policy())
        .dns_resolver(Arc::new(tools::web::GuardedDns))
        .build()
        .context("building the HTTP client")?;

    let addr: SocketAddr = std::env::var("IRIS_ADDR")
        .unwrap_or(cfg.server.addr)
        .parse()
        .context("server address must look like 0.0.0.0:7700")?;

    let link = Arc::new(link::Link::new(link::Identity::load_or_create(&data_dir)?, cfg.link.clone()));
    let state = AppState {
        store,
        providers: Arc::new(RwLock::new(providers)),
        data_dir: Arc::new(data_dir),
        events: broadcast::channel(1024).0,
        runs: Arc::new(Mutex::new(HashMap::new())),
        approvals: Arc::new(Mutex::new(HashMap::new())),
        followups: Arc::new(Mutex::new(HashMap::new())),
        http,
        search: Arc::new(RwLock::new(search)),
        defaults: Arc::new(RwLock::new(cfg.defaults.clone())),
        limits: Arc::new(RwLock::new(cfg.limits.clone())),
        style: Arc::new(RwLock::new(cfg.style.clone())),
        local_device_id: Arc::new(Some(local_device_id)),
        started: Instant::now(),
        online: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        browser: Arc::default(),
        link,
        queues: Arc::default(),
        mcp: Arc::default(),
    };
    // The Developer agent (once), and an update that was installing when the engine restarted.
    develop::seed_developer(&state);
    tokio::spawn(develop::resume(state.clone()));
    // Connect the MCP servers in the background (programs may take a while to start).
    {
        let st = state.clone();
        tokio::spawn(async move { st.mcp.refresh(&st.data_dir, &st.http).await });
    }
    // Close the browser after a few idle minutes to give its memory back.
    let browser = state.browser.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            browser.close_if_idle().await;
        }
    });
    tokio::spawn(watch_connectivity(state.clone()));
    tokio::spawn(routines::run_forever(state.clone()));
    tokio::spawn(link::run_forever(state.clone()));
    tokio::spawn(problems::run_forever(state.clone()));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot listen on {addr}"))?;
    info!("listening on {addr}");
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            info!("shutting down");
        })
        .await?;
    Ok(())
}

/// Checks every 30 s that the default provider can be reached, and tells the
/// app when that changes (it shows a red bar while offline). Local providers
/// count as always reachable.
async fn watch_connectivity(st: AppState) {
    use std::sync::atomic::Ordering;
    loop {
        let url = st
            .default_model()
            .and_then(|(p, _)| st.providers.read().unwrap().get(&p).map(|p| p.base_url.clone()));
        let online = match url {
            Some(url) if !url.contains("://localhost") && !url.contains("://127.0.0.1") => st
                .http
                .get(&url)
                .timeout(std::time::Duration::from_secs(8))
                .send()
                .await
                .is_ok(),
            _ => true,
        };
        if st.online.swap(online, Ordering::Relaxed) != online {
            if online {
                info!("the AI provider is reachable again");
            } else {
                warn!("can't reach the AI provider (no internet?)");
            }
            st.emit(events::Event::Connectivity { online });
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

/// A fresh install gets one general-purpose agent to talk to.
fn seed_default_agent(store: &Store, providers: &Providers) -> Result<()> {
    if !store.list_agents()?.is_empty() {
        return Ok(());
    }
    if providers.first().is_none() {
        warn!("no providers configured; not creating a default agent");
        return Ok(());
    }
    let agent = store.create_agent(NewAgent {
        name: "Iris".into(),
        description: MAIN_DESCRIPTION.into(),
        avatar: String::new(),
        color: iris_core::AGENT_COLORS[0].into(),
        system_prompt: MAIN_SYSTEM_PROMPT.into(),
        // Empty: follow the default model from Settings.
        provider: String::new(),
        model: String::new(),
        reasoning_effort: String::new(),
        tools: MAIN_TOOLS.iter().map(|t| t.to_string()).collect(),
        role: "main".into(),
        created_by: String::new(),
    })?;
    info!("created default agent {} ({})", agent.name, agent.id);
    Ok(())
}

/// There is always exactly one main agent (the Chief of Staff the user talks
/// to). Older data has none: promote Iris (or the first agent), give it the
/// team tools, and upgrade its prompt if the user never edited it.
fn ensure_main_agent(store: &Store, data_dir: &std::path::Path) -> Result<()> {
    let agents = store.list_agents()?;
    let main = match agents.iter().find(|a| a.role == "main") {
        Some(main) => main.clone(),
        None => {
            let Some(main) = agents.iter().find(|a| a.name == "Iris").or(agents.first()) else { return Ok(()) };
            store.set_main_agent(&main.id)?;
            info!("{} is now the main agent", main.name);
            main.clone()
        }
    };
    // Upgrade the prompt and tools only while the user hasn't customized it.
    let prompt = main.system_prompt.trim();
    let untouched = prompt.is_empty()
        || [LEGACY_SYSTEM_PROMPT, PREV_MAIN_SYSTEM_PROMPT, PREV2_MAIN_SYSTEM_PROMPT, PREV3_MAIN_SYSTEM_PROMPT, PREV4_MAIN_SYSTEM_PROMPT, PREV5_MAIN_SYSTEM_PROMPT]
            .contains(&prompt);
    if !untouched {
        return Ok(());
    }
    let mut tools = main.tools.clone();
    for t in MAIN_TOOLS {
        if !tools.iter().any(|x| x == t) {
            tools.push((*t).into());
        }
    }
    let upgraded = store.update_agent(
        &main.id,
        iris_core::AgentPatch {
            tools: Some(tools),
            system_prompt: Some(MAIN_SYSTEM_PROMPT.to_owned()),
            description: (main.description == "Your general-purpose assistant" || main.description.is_empty())
                .then(|| MAIN_DESCRIPTION.to_owned()),
            ..Default::default()
        },
    )?;
    // Keep SOUL.md in step, or the old file would win on the next start.
    if let Some(agent) = upgraded {
        memory::save_soul(data_dir, &agent);
    }
    info!("upgraded {}'s instructions", main.name);
    Ok(())
}

/// Agents are shown as orbs in soft pastel colors; give any agent whose color
/// isn't from the palette (e.g. created before orbs existed) the next free one.
fn assign_orb_colors(store: &Store) -> Result<()> {
    let agents = store.list_agents()?;
    let mut used: Vec<String> = agents
        .iter()
        .filter(|a| iris_core::AGENT_COLORS.iter().any(|c| c.eq_ignore_ascii_case(&a.color)))
        .map(|a| a.color.clone())
        .collect();
    for agent in agents {
        if iris_core::AGENT_COLORS.iter().any(|c| c.eq_ignore_ascii_case(&agent.color)) {
            continue;
        }
        let color = iris_core::next_agent_color(&used);
        used.push(color.clone());
        store.update_agent(&agent.id, iris_core::AgentPatch { color: Some(color), ..Default::default() })?;
    }
    Ok(())
}
