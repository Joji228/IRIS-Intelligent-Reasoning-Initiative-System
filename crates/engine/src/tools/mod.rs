//! Tools agents can use.
//!
//! Agents enable *toolsets* by id (`web_search`, `files`, ...); each toolset
//! provides one or more functions the model can call. Every call returns text
//! for the model plus a one-line summary for people. Risky toolsets require the
//! user's approval for every call.

pub(crate) mod files;
mod shell;
mod time;
pub mod web;

use std::{path::PathBuf, time::Duration};

use iris_providers::{ToolCall, ToolSpec};
use serde::Serialize;
use serde_json::{Value, json};

pub use shell::run as run_powershell;
pub use time::now_line;
pub use web::SearchSettings;

/// A group of related tools that an agent turns on or off as one switch.
#[derive(Debug, Clone, Serialize)]
pub struct Toolset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Every call waits for the user's Approve/Deny.
    pub needs_approval: bool,
}

/// Abilities every agent has without a toggle: low risk, and what lets the
/// team work together (talk to each other and Iris, schedule, look things up).
pub const BUILT_IN: &[&str] = &["web_search", "web_fetch", "get_time", "team", "routines", "browser", "models", "settings"];
/// Team functions only the coordinator (Iris) uses: the others can talk to
/// and hand work to agents, not create or remove them.
pub const COORDINATOR_ONLY: &[&str] = &["create_agent", "keep_agent", "dismiss_agent"];

/// The toolsets an agent can actually use: Iris has all of them; the others
/// have the built-in ones plus what was switched on (Files, Computer control).
/// `ask_user`: a question with clickable options. Every agent has it; the
/// reply ends there and the user's pick comes back as their next message.
pub fn ask_user_spec() -> iris_providers::ToolSpec {
    spec(
        "ask_user",
        "Ask the user to choose: the question shows with clickable options (and a box to type their own answer). \
         Use it whenever you need a decision or a missing detail and the likely answers are a few short choices, \
         instead of asking in text. Your reply ends right after it; their answer comes as their next message.",
        json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "One short question." },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "2 to 6 short answers (a few words each), most likely first."
                },
                "allow_other": { "type": "boolean", "description": "Offer a box for their own answer (default true)." }
            },
            "required": ["question", "options"]
        }),
    )
}

/// Checks an `ask_user` call; the card itself is drawn by the app.
pub fn ask_user(call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let question = args["question"].as_str().unwrap_or_default().trim().to_owned();
    let options: Vec<String> = args["options"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o.as_str()).map(|o| o.trim().to_owned()).filter(|o| !o.is_empty()).collect())
        .unwrap_or_default();
    if question.is_empty() || !(2..=6).contains(&options.len()) || options.iter().any(|o| o.chars().count() > 80) {
        return ToolOutcome {
            ok: false,
            output: "Error: give one question and 2 to 6 short options (80 characters at most each).".into(),
            summary: "Couldn't ask the question".into(),
        };
    }
    ToolOutcome {
        ok: true,
        output: "The question is on the user's screen with the options as buttons. Your reply ends here; their \
                 answer comes as their next message."
            .into(),
        summary: format!("Asked: {question}"),
    }
}

pub fn effective_tools(agent: &iris_core::Agent) -> Vec<String> {
    // Iris has everything except the code tools: she hands code work to the Developer.
    if agent.role == "main" {
        return TOOLSETS.iter().filter(|t| t.id != "dev").map(|t| t.id.to_owned()).collect();
    }
    let mut tools: Vec<String> = BUILT_IN.iter().map(|t| (*t).to_owned()).collect();
    for t in &agent.tools {
        if !tools.contains(t) && TOOLSETS.iter().any(|x| x.id == t) {
            tools.push(t.clone());
        }
    }
    tools
}

pub const TOOLSETS: &[Toolset] = &[
    Toolset {
        id: "web_search",
        name: "Web search",
        description: "Search the internet for current information.",
        needs_approval: false,
    },
    Toolset {
        id: "web_fetch",
        name: "Read web pages",
        description: "Open a link and read the page's text.",
        needs_approval: false,
    },
    Toolset {
        id: "get_time",
        name: "Date & time",
        description: "Know today's date, the time and the timezone.",
        needs_approval: false,
    },
    Toolset {
        id: "files",
        name: "Files",
        description: "Read, write and list files in the agent's own workspace folder.",
        needs_approval: false,
    },
    Toolset {
        id: "shell",
        name: "Run commands",
        description: "Run PowerShell commands in the agent's workspace. Asks you first, every time.",
        needs_approval: true,
    },
    Toolset {
        id: "team",
        name: "Team",
        description: "Create agents, hand them tasks and collect their results. Dismissing an agent asks you first.",
        needs_approval: false,
    },
    Toolset {
        id: "routines",
        name: "Routines",
        description: "Schedule tasks and checks (every 30 min, daily at 8, …). Scheduled commands ask you first.",
        needs_approval: false,
    },
    Toolset {
        id: "browser",
        name: "Browser",
        description: "Use a real browser: open pages, click, type and read sites that need JavaScript or a login. Paying or ordering always asks you.",
        needs_approval: false,
    },
    Toolset {
        id: "system",
        name: "This computer",
        description: "See the server's health, update, install or remove apps, Windows Update, restart or shut down. Changes ask you first.",
        needs_approval: false,
    },
    Toolset {
        id: "models",
        name: "Models",
        description: "Say which AI model it runs on, list the models you have set up and switch them when you ask.",
        needs_approval: false,
    },
    Toolset {
        id: "settings",
        name: "Settings",
        description: "See every setting and change it when you ask. Risky changes (remote access, devices, safety, limits, \
                      instructions) always ask you first.",
        needs_approval: false,
    },
    Toolset {
        id: "mcp",
        name: "Connections",
        description: "Tools from services you connect in Settings → Connections (MCP servers). Anything that isn't \
                      read-only asks you first, unless you trusted it.",
        // Decided per tool (see `mcp::Hub::needs_approval`).
        needs_approval: false,
    },
    Toolset {
        id: "dev",
        name: "Develop I.R.I.S.",
        description: "Read and change I.R.I.S.'s own source code on a branch. Applying a change (you see the diff) and                       installing it always ask you first.",
        // code_apply and code_install ask (see `develop::needs_approval`).
        needs_approval: false,
    },
];

/// MCP tools are offered as `mcp__<server>__<tool>`.
pub const MCP_PREFIX: &str = "mcp__";

/// Team functions are run by the chat loop itself (they need the whole engine).
pub const TEAM_FUNCTIONS: &[&str] =
    &["list_agents", "create_agent", "delegate_task", "check_task", "keep_agent", "dismiss_agent"];
/// Functions of the routines toolset (see `routines.rs`).
pub const ROUTINE_FUNCTIONS: &[&str] = &["create_routine", "list_routines", "delete_routine"];
/// Functions of the browser toolset (see `browser.rs`).
pub const BROWSER_FUNCTIONS: &[&str] = &[
    "browser_open", "browser_read", "browser_click", "browser_type", "browser_scroll", "browser_back", "browser_close",
    "browser_login", "list_accounts",
];
/// Functions of the "This computer" toolset (see `system.rs`).
pub const SYSTEM_FUNCTIONS: &[&str] =
    &["system_status", "app_updates", "search_apps", "update_apps", "install_app", "uninstall_app", "windows_updates", "power"];
/// Model functions are run by the chat loop too (they change settings).
pub const MODEL_FUNCTIONS: &[&str] = &["list_models", "switch_model"];
/// Settings functions are run by the chat loop (they use the engine's API).
pub const SETTINGS_FUNCTIONS: &[&str] = &["get_settings", "change_setting"];
/// Functions that bring web content (possibly a prompt injection) into a reply.
pub const WEB_CONTENT_FUNCTIONS: &[&str] = &[
    "web_search", "web_fetch", "browser_open", "browser_read", "browser_click", "browser_type", "browser_scroll", "browser_back",
];
/// Functions whose effects outlast the reply (settings, schedules, tools,
/// skills, connections, memory): after web content was read, they ask the user.
/// Memory counts: what an agent saves is put back into its instructions in
/// every later reply, so text from a web page must not get in unnoticed.
pub const LASTING_FUNCTIONS: &[&str] =
    &["change_setting", "switch_model", "create_routine", "delete_routine", "request_tools", "save_skill", "memory"];
/// Of those, the ones Full auto lets through even after web content was read
/// (the user asked for few interruptions; settings, routines and skills still ask).
pub const LASTING_OK_IN_FULL: &[&str] = &["switch_model", "request_tools", "memory"];

/// Which toolset a callable function belongs to.
pub fn toolset_of(function: &str) -> Option<&'static str> {
    Some(match function {
        "web_search" => "web_search",
        "web_fetch" => "web_fetch",
        "get_time" => "get_time",
        "read_file" | "write_file" | "list_files" | "view_image" => "files",
        "run_command" => "shell",
        f if TEAM_FUNCTIONS.contains(&f) => "team",
        f if MODEL_FUNCTIONS.contains(&f) => "models",
        f if SETTINGS_FUNCTIONS.contains(&f) => "settings",
        f if SYSTEM_FUNCTIONS.contains(&f) => "system",
        f if BROWSER_FUNCTIONS.contains(&f) => "browser",
        f if ROUTINE_FUNCTIONS.contains(&f) => "routines",
        f if f.starts_with(MCP_PREFIX) => "mcp",
        f if crate::develop::FUNCTIONS.contains(&f) => "dev",
        _ => return None,
    })
}

pub fn needs_approval(function: &str) -> bool {
    // Removing an agent deletes its conversations: always ask.
    if function == "dismiss_agent" {
        return true;
    }
    toolset_of(function)
        .and_then(|id| TOOLSETS.iter().find(|t| t.id == id))
        .is_some_and(|t| t.needs_approval)
}

/// `request_tools`: offered to every agent but Iris, so an agent missing a
/// tool asks for it instead of refusing the user.
pub fn request_tools_spec() -> ToolSpec {
    spec(
        "request_tools",
        "Ask Iris (the Chief of Staff) for tools you don't have, when the user asks for something your tools \
         can't do: files (read/write files), shell (run PowerShell commands), system (updates, installs, \
         restart, system status). Iris reviews it; granted tools work right away, so then do the task. Say why: \
         what the user asked.",
        json!({
            "type": "object",
            "properties": {
                "tools": { "type": "array", "items": { "type": "string", "enum": crate::design::DESIGNABLE_TOOLS } },
                "reason": { "type": "string", "description": "What the user asked for that needs these tools." }
            },
            "required": ["tools", "reason"]
        }),
    )
}

/// The function definitions offered to the model for the enabled toolsets.
/// Every agent also gets `memory` (its MEMORY.md and the shared USER.md).
pub fn specs_for(enabled: &[String]) -> Vec<ToolSpec> {
    let on = |id: &str| enabled.iter().any(|e| e == id);
    let mut specs = vec![spec(
        "memory",
        "Save, update or remove a long-term note. target \"user\": facts about the user (USER.md, shared by \
         all agents); target \"memory\": your own notes (MEMORY.md). Notes persist across conversations and \
         compactions, and are shown to you in every reply.",
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "replace", "remove"] },
                "target": { "type": "string", "enum": ["memory", "user"] },
                "content": { "type": "string", "description": "The note (for add / replace). One short line." },
                "old": { "type": "string", "description": "Part of the existing note (for replace / remove)." }
            },
            "required": ["action", "target"]
        }),
    )];
    // Recall and skills: every agent, like memory.
    specs.push(spec(
        "search_chats",
        "Search the text of past conversations (every agent's chat, including parts that were compacted into a \
         summary). Use it when the user refers to something discussed before, or to check what was decided.",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Words to look for (any of them may match)." },
                "agent": { "type": "string", "description": "Only this agent's chat (its name). Optional." },
                "max_results": { "type": "integer", "description": "1-20, default 8." }
            },
            "required": ["query"]
        }),
    ));
    specs.push(spec(
        "use_skill",
        "Load a saved skill (step-by-step instructions for a kind of task) by name.",
        json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
    ));
    specs.push(spec(
        "save_skill",
        "Save (or update) a skill for the whole team: how to do a kind of task that will come up again. Same name \
         replaces it.",
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Short, like \"deploy-website\"." },
                "description": { "type": "string", "description": "One line: when to use it." },
                "instructions": { "type": "string", "description": "Concrete steps, commands, gotchas. Markdown." }
            },
            "required": ["name", "description", "instructions"]
        }),
    ));
    if on("web_search") {
        specs.push(spec(
            "web_search",
            "Search the web. Returns titles, links and snippets of the top results. Use it for anything \
             recent or that you are not sure about; then use web_fetch to read promising pages.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query." },
                    "max_results": { "type": "integer", "description": "How many results (1-10, default 5)." }
                },
                "required": ["query"]
            }),
        ));
    }
    if on("web_fetch") {
        specs.push(spec(
            "web_fetch",
            "Download a web page and return its readable text (and title). Long pages are shortened.",
            json!({
                "type": "object",
                "properties": { "url": { "type": "string", "description": "Full http(s) URL." } },
                "required": ["url"]
            }),
        ));
    }
    if on("get_time") {
        specs.push(spec(
            "get_time",
            "Get the current local date, time, weekday and timezone.",
            json!({ "type": "object", "properties": {} }),
        ));
    }
    if on("files") {
        let path = json!({ "type": "string", "description": "Path relative to your workspace folder." });
        specs.push(spec(
            "list_files",
            "List files and folders in your workspace (or a sub-folder of it).",
            json!({ "type": "object", "properties": { "path": path } }),
        ));
        specs.push(spec(
            "view_image",
            "Look at an image file (png, jpg, gif, webp): a path in your workspace or anywhere on this computer.",
            json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
        ));
        specs.push(spec(
            "read_file",
            "Read a text file from your workspace.",
            json!({ "type": "object", "properties": { "path": path }, "required": ["path"] }),
        ));
        specs.push(spec(
            "write_file",
            "Create or overwrite a text file in your workspace (folders are created as needed).",
            json!({
                "type": "object",
                "properties": { "path": path, "content": { "type": "string", "description": "Full file content." } },
                "required": ["path", "content"]
            }),
        ));
    }
    if on("shell") {
        specs.push(spec(
            "run_command",
            "Run a PowerShell command with your workspace as the current folder and return its output. \
             Commands may need the user's approval, so explain briefly why you need it. Times out after 2 minutes: \
             keep commands quick (short network timeouts like -TimeoutSec 15, a few requests at a time) and \
             combine related checks into one command instead of many small ones.",
            json!({
                "type": "object",
                "properties": { "command": { "type": "string", "description": "The PowerShell command." } },
                "required": ["command"]
            }),
        ));
    }
    if on("team") {
        let agent = json!({ "type": "string", "description": "The agent's name (as shown by list_agents)." });
        specs.push(spec(
            "list_agents",
            "List the agents on the team with their purpose, tools and whether they are busy.",
            json!({ "type": "object", "properties": {} }),
        ));
        specs.push(spec(
            "create_agent",
            "Create a new specialist agent (it appears as a new orb for the user). Only do this when no \
             existing agent fits. The team holds at most 10 agents including you.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short, clear name, e.g. \"Researcher\" or \"Site Checker\"." },
                    "purpose": { "type": "string", "description": "One sentence: what this agent is for." },
                    "instructions": { "type": "string", "description": "Optional detailed instructions / personality." },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["web_search", "web_fetch", "get_time", "files", "shell"] },
                        "description": "Tools to give it (default: web_search, web_fetch, get_time, files)."
                    },
                    "keep": {
                        "type": "boolean",
                        "description": "Keep it on the team after its task: true when the user asked for this agent, or for ongoing or recurring work. Default false: a one-off agent, removed when you finish the reply in which you collected its result with check_task."
                    }
                },
                "required": ["name", "purpose"]
            }),
        ));
        specs.push(spec(
            "delegate_task",
            "Give another agent a task. It starts working right away (in parallel with you and others) in its \
             own conversation. Returns a task_id. Don't wait: you get an automatic update when it finishes, \
             then read the result with check_task. Make the task \
             self-contained: the agent cannot see your conversation.",
            json!({
                "type": "object",
                "properties": {
                    "agent": agent,
                    "task": { "type": "string", "description": "Everything the agent needs to know to do the task." }
                },
                "required": ["agent", "task"]
            }),
        ));
        specs.push(spec(
            "check_task",
            "Read a delegated task's result, or see how it is going. By default it looks right away; \
             `wait_seconds` (max 60) waits for a task that is about to finish.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "wait_seconds": { "type": "integer" }
                },
                "required": ["task_id"]
            }),
        ));
        specs.push(spec(
            "keep_agent",
            "Keep a one-off agent on the team instead of removing it after its task, e.g. when its result shows \
             it needs more information or a decision from the user. Not needed while it only waits for an \
             approval: you are brought back automatically when its task finishes.",
            json!({ "type": "object", "properties": { "agent": agent }, "required": ["agent"] }),
        ));
        specs.push(spec(
            "dismiss_agent",
            "Remove an agent you created when it is no longer needed (frees a slot). Asks the user first.",
            json!({ "type": "object", "properties": { "agent": agent }, "required": ["agent"] }),
        ));
    }
    if on("routines") {
        specs.push(spec(
            "create_routine",
            "Schedule something. kind \"prompt\": at each run you (or `agent`) get `task` as a message and do it. \
             kind \"command\": `task` is a PowerShell command run without the AI; the agent is only woken when it \
             fails (notify \"failure\", default) or every time (\"always\"). Prefer a cheap command for simple \
             checks (site up? disk space?). kind \"heartbeat\": `task` is a checklist the agent goes through at \
             each run; when nothing needs attention the check is silent (removed from the chat), otherwise it \
             tells the user. Good for \"keep an eye on …\" (every 30m or more). schedule: \"every 30m\", \"every 2h\", \"daily 08:00\", \
             \"weekdays 09:00\", \"weekends 10:00\", \"weekly mon 09:00\" or \"once 2026-11-02 08:00\" (local time).",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short name, e.g. \"Morning site check\"." },
                    "schedule": { "type": "string" },
                    "kind": { "type": "string", "enum": ["prompt", "command", "heartbeat"] },
                    "task": { "type": "string" },
                    "notify": { "type": "string", "enum": ["failure", "always"] },
                    "agent": { "type": "string", "description": "Which agent runs it (default: you)." }
                },
                "required": ["name", "schedule", "kind", "task"]
            }),
        ));
        specs.push(spec("list_routines", "List the scheduled routines.", json!({ "type": "object", "properties": {} })));
        specs.push(spec(
            "delete_routine",
            "Delete a routine by name.",
            json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
        ));
    }
    if on("browser") {
        let n = json!({ "type": "integer", "description": "The element's number from the page listing." });
        specs.push(spec(
            "browser_open",
            "Open a web page in a real browser (runs JavaScript, keeps logins). Returns the page text and its \
             numbered buttons, links and fields. Prefer web_fetch for simple reading.",
            json!({ "type": "object", "properties": { "url": { "type": "string" } }, "required": ["url"] }),
        ));
        specs.push(spec("browser_read", "Read the current page again (text + numbered elements).", json!({ "type": "object", "properties": {} })));
        specs.push(spec(
            "browser_click",
            "Click numbered element n. Anything that pays or places an order asks the user first.",
            json!({ "type": "object", "properties": { "n": n }, "required": ["n"] }),
        ));
        specs.push(spec(
            "browser_type",
            "Type text into numbered field n (replacing what's there); submit=true presses Enter. Never passwords.",
            json!({
                "type": "object",
                "properties": { "n": n, "text": { "type": "string" }, "submit": { "type": "boolean" } },
                "required": ["n", "text"]
            }),
        ));
        specs.push(spec(
            "browser_scroll",
            "Scroll the page down (default) or up to see more.",
            json!({ "type": "object", "properties": { "direction": { "type": "string", "enum": ["down", "up"] } } }),
        ));
        specs.push(spec("browser_back", "Go back to the previous page.", json!({ "type": "object", "properties": {} })));
        specs.push(spec("browser_close", "Close your browser tab when done.", json!({ "type": "object", "properties": {} })));
        specs.push(spec(
            "list_accounts",
            "The user's saved logins (names and sites only; you never see the passwords).",
            json!({ "type": "object", "properties": {} }),
        ));
        specs.push(spec(
            "browser_login",
            "On a site's sign-in page, fill in the user's saved login for `account` (only works on that account's site). \
             Then click the sign-in button yourself. For two-step logins, call it again on the password page.",
            json!({ "type": "object", "properties": { "account": { "type": "string" } }, "required": ["account"] }),
        ));
    }
    if on("system") {
        let id = json!({ "type": "string", "description": "The app's winget id, e.g. \"Mozilla.Firefox\"." });
        specs.push(spec("system_status", "This server's health: OS, uptime, CPU load, RAM, disks, pending restart, top memory users.", json!({ "type": "object", "properties": {} })));
        specs.push(spec("app_updates", "List installed apps that have updates (winget).", json!({ "type": "object", "properties": {} })));
        specs.push(spec(
            "search_apps",
            "Find apps to install by name (winget); returns ids.",
            json!({ "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] }),
        ));
        specs.push(spec(
            "update_apps",
            "Update apps: the given winget ids, or all=true for every app with an update.",
            json!({
                "type": "object",
                "properties": { "ids": { "type": "array", "items": { "type": "string" } }, "all": { "type": "boolean" } }
            }),
        ));
        specs.push(spec("install_app", "Install an app by winget id. Asks the user first.", json!({ "type": "object", "properties": { "id": id }, "required": ["id"] })));
        specs.push(spec("uninstall_app", "Uninstall an app by winget id. Asks the user first.", json!({ "type": "object", "properties": { "id": id }, "required": ["id"] })));
        specs.push(spec(
            "windows_updates",
            "Check for Windows updates, or install them with install=true.",
            json!({ "type": "object", "properties": { "install": { "type": "boolean" } } }),
        ));
        specs.push(spec(
            "power",
            "Restart or shut down this server (after delay_minutes, default 1), or cancel a pending one. Always asks the \
             user first. Warn the user: agents are offline until the server is back.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["restart", "shutdown", "cancel"] },
                    "delay_minutes": { "type": "integer" }
                },
                "required": ["action"]
            }),
        ));
    }
    if on("settings") {
        specs.push(spec(
            "get_settings",
            "Read I.R.I.S.'s settings (never secrets) to answer the user's questions about how things are set up. \
             section: all (default), models, responses, limits, search, remote, agents, providers, devices, routines.",
            json!({
                "type": "object",
                "properties": { "section": { "type": "string", "enum": crate::settings::SECTIONS } }
            }),
        ));
        specs.push(spec(
            "change_setting",
            "Change a setting when the user asks. Risky changes ask the user automatically. Models and the \
             fallback: use switch_model. Values: style = balanced|concise|analyst|witty|teacher|professional; \
             style_custom = free text; reasoning_default and agent_reasoning = off|low|medium|high|max|auto \
             (agent: default); *_token_limit = number, 0 = none; search_provider = auto|duckduckgo|tavily|deepseek; \
             remote_access = on|off; agent_auto and default_auto (new agents) = off|careful|full; agent_add_tool / agent_remove_tool = a \
             toolset id (web_search, web_fetch, get_time, files, shell, team, routines, browser, system, models, \
             settings); revoke_device = device name; remove_provider = provider id. `agent` = the agent's name \
             (default: you). API keys and passwords can't be set here: the user does that in Settings.",
            json!({
                "type": "object",
                "properties": {
                    "setting": { "type": "string", "enum": crate::settings::SETTINGS },
                    "value": { "type": "string" },
                    "agent": { "type": "string", "description": "For agent_* settings: which agent (default: you)." }
                },
                "required": ["setting", "value"]
            }),
        ));
    }
    if on("models") {
        specs.push(spec(
            "list_models",
            "Show the AI setup: the default model, the fallback, agents with their own model, and each provider \
             with the models it offers.",
            json!({
                "type": "object",
                "properties": { "provider": { "type": "string", "description": "Only this provider (id or name)." } }
            }),
        ));
        specs.push(spec(
            "switch_model",
            "Switch the model when the user asks. target: \"all\" (default: every agent without its own model), \
             an agent's name or \"me\" (just that agent; model \"default\" puts it back on the default), \
             \"fallback\" (the model used when the main provider fails) or \"fallback_off\". Use exact model \
             ids from list_models.",
            json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string", "description": "Exact model id, e.g. \"deepseek-v4-pro\"." },
                    "provider": { "type": "string", "description": "Provider id or name (optional: found from the model)." },
                    "target": { "type": "string", "description": "\"all\" (default), an agent name, \"me\", \"fallback\" or \"fallback_off\"." }
                },
                "required": ["model"]
            }),
        ));
    }
    specs
}

fn spec(name: &str, description: &str, parameters: Value) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), parameters }
}

/// What a tool needs to run.
pub struct ToolContext {
    pub http: reqwest::Client,
    /// The agent's private folder; file and shell tools never leave it.
    pub workspace: PathBuf,
    pub search: SearchSettings,
}

pub struct ToolOutcome {
    pub ok: bool,
    /// Text handed back to the model.
    pub output: String,
    /// One line for people.
    pub summary: String,
}

impl ToolOutcome {
    pub fn ok(summary: impl Into<String>, output: impl Into<String>) -> Self {
        Self { ok: true, output: output.into(), summary: summary.into() }
    }
    pub fn err(summary: impl Into<String>, error: impl Into<String>) -> Self {
        let error = error.into();
        Self { ok: false, output: format!("Error: {error}"), summary: summary.into() }
    }
}

/// A short "what is about to happen" line, shown while a tool runs or waits
/// for approval.
pub fn describe(call: &ToolCall) -> String {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    match call.name.as_str() {
        "web_search" => format!("Searching the web for “{}”", s("query")),
        "web_fetch" => format!("Reading {}", short_url(&s("url"))),
        "get_time" => "Checking the date and time".into(),
        "memory" => "Updating memory".into(),
        n if n.starts_with(MCP_PREFIX) => {
            let mut parts = n[MCP_PREFIX.len()..].splitn(2, "__");
            let server = parts.next().unwrap_or_default();
            format!("{server}: {}", parts.next().unwrap_or_default().replace('_', " "))
        }
        "search_chats" => format!("Searching past chats for “{}”", s("query")),
        "code_status" => "Checking the source repo".into(),
        "code_search" => format!("Searching the code for “{}”", s("pattern")),
        "code_read" => format!("Reading {}", s("path")),
        "code_edit" => format!("Editing {}", s("path")),
        "code_write" => format!("Writing {}", s("path")),
        "code_diff" => "Looking at the changes".into(),
        "code_discard" => "Discarding the changes".into(),
        "code_apply" => format!("Apply and test: {}", s("message")),
        "code_install" => "Build and install the new version (the app restarts)".into(),
        "code_verify" => "Verifying the installed build".into(),
        "use_skill" => format!("Loading the skill “{}”", s("name")),
        "save_skill" => format!("Saving the skill “{}”", s("name")),
        "view_image" => format!("Looking at {}", s("path")),
        "system_status" => "Checking this computer".into(),
        "create_routine" => format!("Scheduling “{}” ({})", s("name"), s("schedule")),
        "list_routines" => "Checking the routines".into(),
        "delete_routine" => format!("Deleting routine “{}”", s("name")),
        "browser_open" => format!("Opening {}", short_url(&s("url"))),
        "browser_read" => "Reading the page".into(),
        "browser_click" => format!("Clicking [{}]", args["n"]),
        "browser_type" => format!("Typing into [{}]", args["n"]),
        "browser_scroll" => "Scrolling".into(),
        "browser_back" => "Going back".into(),
        "browser_close" => "Closing the browser tab".into(),
        "browser_login" => format!("Sign in with the {} account", s("account")),
        "list_accounts" => "Checking saved accounts".into(),
        "app_updates" => "Checking for app updates".into(),
        "search_apps" => format!("Searching apps for “{}”", s("query")),
        "update_apps" => "Updating apps".into(),
        "install_app" => format!("Install {}", s("id")),
        "uninstall_app" => format!("Uninstall {}", s("id")),
        "windows_updates" => "Checking Windows Update".into(),
        "power" => format!("Power: {}", s("action")),
        "list_files" => format!("Listing files in {}", or_root(&s("path"))),
        "read_file" => format!("Reading {}", s("path")),
        "write_file" => format!("Writing {}", s("path")),
        "run_command" => format!("Run: {}", s("command")),
        "list_agents" => "Checking the team".into(),
        "create_agent" => format!("Creating agent “{}”", s("name")),
        "delegate_task" => format!("Handing {} a task", s("agent")),
        "check_task" => "Checking on a task".into(),
        "dismiss_agent" => format!("Dismiss agent “{}”", s("agent")),
        "keep_agent" => format!("Keeping {} on the team", s("agent")),
        "list_models" => "Checking the models".into(),
        "switch_model" => format!("Switching to {}", s("model")),
        "get_settings" => "Looking at the settings".into(),
        "request_tools" => format!(
            "Asking Iris for {}",
            args["tools"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
        ),
        "change_setting" => format!("Change {} to “{}”", s("setting").replace('_', " "), s("value")),
        other => format!("Using {other}"),
    }
}

/// Runs one tool call. Never panics; problems come back as an error outcome
/// the model can read and react to.
pub async fn run(ctx: &ToolContext, call: &ToolCall) -> ToolOutcome {
    let args: Value = match serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments }) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("Invalid arguments for {}", call.name), format!("arguments are not valid JSON: {e}")),
    };
    let s = |k: &str| args[k].as_str().map(str::to_owned);
    match call.name.as_str() {
        "web_search" => match s("query") {
            Some(q) => web::search(ctx, &q, args["max_results"].as_u64().unwrap_or(5).clamp(1, 10) as usize).await,
            None => ToolOutcome::err("Web search failed", "missing `query`"),
        },
        "web_fetch" => match s("url") {
            Some(u) => web::fetch(ctx, &u).await,
            None => ToolOutcome::err("Reading a page failed", "missing `url`"),
        },
        "get_time" => time::now(),
        "list_files" => files::list(&ctx.workspace, &s("path").unwrap_or_default()),
        "read_file" => match s("path") {
            Some(p) => files::read(&ctx.workspace, &p),
            None => ToolOutcome::err("Reading a file failed", "missing `path`"),
        },
        "write_file" => match (s("path"), s("content")) {
            (Some(p), Some(c)) => files::write(&ctx.workspace, &p, &c),
            _ => ToolOutcome::err("Writing a file failed", "missing `path` or `content`"),
        },
        "run_command" => match s("command") {
            Some(c) => shell::run(&ctx.workspace, &c, Duration::from_secs(120)).await,
            None => ToolOutcome::err("Command failed", "missing `command`"),
        },
        other => ToolOutcome::err(format!("Unknown tool {other}"), format!("there is no tool called `{other}` (or it is not enabled for this agent)")),
    }
}

/// Keeps tool output within a budget so one big page can't flood the context.
pub fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max_chars).collect();
    format!("{kept}\n\n[… shortened: {} more characters not shown]", count - max_chars)
}

fn short_url(url: &str) -> String {
    let u = url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    if u.chars().count() > 60 { format!("{}…", u.chars().take(59).collect::<String>()) } else { u.to_owned() }
}

fn or_root(p: &str) -> String {
    if p.trim().is_empty() || p.trim() == "." { "the workspace".into() } else { p.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_follow_enabled_toolsets() {
        // Memory, chat search and skills come first for every agent; the rest
        // follow the toolsets.
        let names = |v: &[&str]| -> Vec<String> {
            specs_for(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).into_iter().map(|s| s.name).skip(ALWAYS.len()).collect()
        };
        const ALWAYS: &[&str] = &["memory", "search_chats", "use_skill", "save_skill"];
        let first: Vec<String> = specs_for(&[]).into_iter().map(|s| s.name).collect();
        assert_eq!(first, ALWAYS);
        assert_eq!(names(&["files"]), vec!["list_files", "view_image", "read_file", "write_file"]);
        assert_eq!(names(&["shell", "get_time"]), vec!["get_time", "run_command"]);
        assert!(names(&[]).is_empty());
        assert_eq!(specs_for(&[])[0].name, "memory");
    }

    #[test]
    fn saving_to_memory_asks_after_reading_the_web() {
        assert!(LASTING_FUNCTIONS.contains(&"memory"), "memory is put into later prompts, so injected text must not slip in");
        assert!(LASTING_FUNCTIONS.contains(&"save_skill") && LASTING_FUNCTIONS.contains(&"change_setting"));
    }

    #[test]
    fn built_in_abilities_and_iris_has_everything() {
        let agent = |role: &str, tools: &[&str]| -> iris_core::Agent {
            serde_json::from_value(json!({
                "id": "a", "name": "A", "description": "", "avatar": "", "color": "", "system_prompt": "",
                "provider": "", "model": "", "reasoning_effort": "", "tools": tools, "role": role,
                "created_by": "", "created_at": 0, "updated_at": 0
            }))
            .unwrap()
        };
        let plain = effective_tools(&agent("", &[]));
        for t in BUILT_IN {
            assert!(plain.iter().any(|x| x == t), "{t} is built in");
        }
        assert!(!plain.iter().any(|x| x == "shell" || x == "files" || x == "system"), "the toggles start off");
        let switched = effective_tools(&agent("", &["shell", "system", "bogus"]));
        assert!(switched.iter().any(|x| x == "shell") && !switched.iter().any(|x| x == "bogus"));
        assert_eq!(effective_tools(&agent("main", &[])).len(), TOOLSETS.len() - 1, "Iris has every toolset but the code tools");
    }

    #[test]
    fn team_specs() {
        // After memory, search_chats, use_skill and save_skill.
        let names: Vec<String> = specs_for(&["team".to_string()]).into_iter().map(|s| s.name).skip(4).collect();
        assert_eq!(names, TEAM_FUNCTIONS.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert!(needs_approval("dismiss_agent"));
        assert!(!needs_approval("delegate_task"));
    }

    #[test]
    fn approval_only_for_shell() {
        assert!(needs_approval("run_command"));
        assert!(!needs_approval("write_file"));
        assert!(!needs_approval("web_search"));
        assert!(!needs_approval("made_up"));
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("abc", 5), "abc");
        assert!(truncate(&"x".repeat(20), 5).starts_with("xxxxx\n\n[… shortened: 15"));
    }

    #[test]
    fn descriptions() {
        let call = |name: &str, args: &str| ToolCall { id: "1".into(), name: name.into(), arguments: args.into() };
        assert_eq!(describe(&call("web_search", r#"{"query":"rust"}"#)), "Searching the web for “rust”");
        assert_eq!(describe(&call("web_fetch", r#"{"url":"https://example.com/"}"#)), "Reading example.com");
        assert_eq!(describe(&call("run_command", r#"{"command":"dir"}"#)), "Run: dir");
    }
}
