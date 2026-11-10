# Development

I.R.I.S. is a Rust workspace (`crates/`: `core` storage, `providers` LLM clients, `engine` the always-on server) plus a Tauri 2 + Svelte 5 desktop app (`app/`), a Cloudflare Worker relay for remote access (`relay/`) and a native Android app (`android/`). It runs on **Windows 10/11 (x64)**; the system tools (winget, Windows Update, PowerShell) and the browser tools (Edge) are Windows-specific.

## Building on Windows

Prerequisites: Git, Node.js LTS, Rust (stable, MSVC) and the Visual Studio 2022 C++ Build Tools. `scripts/server-setup.ps1` (run elevated) installs all of them with winget/rustup.

```powershell
scripts\build-windows.ps1      # release engine -> bundled as the app's sidecar -> UI -> NSIS installer
```

The installer lands in `target\release\bundle\nsis\IRIS_<version>_x64-setup.exe`. Other useful commands:

```powershell
cargo test --workspace --exclude iris-desktop    # engine and storage tests
npm --prefix app run check                       # Svelte/TypeScript type check
npm --prefix app run dev                         # UI on http://localhost:1420 against a running engine
```

For `npm run dev`, point the UI at an engine in `app/.env.local` (git-ignored): `VITE_IRIS_URL=http://127.0.0.1:7700` and `VITE_IRIS_TOKEN=<token>` (create one in **Settings → Paired devices**).

| Path | What |
|---|---|
| `%LOCALAPPDATA%\IRIS` | The installed app: `iris-desktop.exe` plus the bundled `iris-engine.exe` |
| `%LOCALAPPDATA%\dev.iris.desktop` | **Data:** `iris.toml`, `iris.db`, `local-token.txt`, `secrets\`, `logs\engine.log` |

The data directory is deliberately not the install folder, so uninstalling or updating the app never touches agents or conversations.

**The desktop app** (`iris-desktop.exe`):
- It starts with Windows (`HKCU\...\Run`, with `--minimized`, so it starts in the tray) and allows only one instance.
- Closing the window hides it to the tray. **Quit I.R.I.S.** in the tray menu stops the app and its engine.
- It runs the engine as a child process with no console window. A supervisor restarts the engine with backoff if it crashes. If something is already serving port 7700, the app uses that instead of starting a second engine.

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when a `v*` tag is pushed: the Windows installer and the Android APK are attached to the release.

## Talking to the engine from a terminal

`scripts/iris.mjs` is a small CLI client that needs Node 22+ and nothing else:

```bash
node scripts/iris.mjs health
node scripts/iris.mjs agents
node scripts/iris.mjs threads [--agent NAME]
node scripts/iris.mjs chat [--agent NAME] [--new] "message"   # one message, streamed
node scripts/iris.mjs chat                                    # interactive; /quit to exit
```

It reads the engine address from `IRIS_URL` (shell or `scripts/.env.local`) and the access token from `.iris-token` in the repo root (git-ignored) or from `IRIS_TOKEN`. You can create a token in the app under **Settings → Paired devices**. The engine also keeps its own `local-token.txt` in the data dir; that's the token the desktop app uses.

## Engine API

Base URL `http://<server-ip>:7700`. The port can be changed with `IRIS_ADDR` or `[server] addr` in `iris.toml`. The firewall rule "IRIS Engine API" only allows the port on the **Private** network profile.

**Auth:** everything under `/v1` needs `Authorization: Bearer <token>`. The WebSocket accepts `?token=<token>` instead. Each device has its own token, and only the token's SHA-256 hash is stored.

| Method | Path | |
|---|---|---|
| GET | `/health` | Public liveness check |
| GET | `/v1/status` | `{running: [{agent_id, thread_id, message_id}]}`: which agents are working right now |
| GET | `/v1/providers` | Configured providers with `has_key` / `key_source` (never the keys) |
| PUT, DELETE | `/v1/providers/{id}` | Add/edit (`{name, base_url, default_model, api_key?}`; omit the key to keep it, `""` to remove it) / remove (409 while agents use it) |
| GET | `/v1/providers/{id}/models` | Live model list from the provider |
| GET | `/v1/providers/{id}/balance` | Remaining API credit `{currency, total, granted, topped_up, available}` (DeepSeek only; 412 without a key, 501 for other providers) |
| GET, PUT | `/v1/settings/model` | The **default model** `{provider, provider_name, model, context_window}` / set it (`{provider, model}`). Agents with an empty `provider`/`model` follow it; an agent can also have its own (agent editor → Model) |
| GET, PUT | `/v1/settings/fallback` | `{enabled, provider, model}`: when a model call fails before any output, the reply is retried once on this model, with a one-line notice at the top of the answer |
| GET, PUT | `/v1/settings/style` | `{preset, custom}`: how agents talk to the user. Presets: `balanced`, `concise`, `analyst`, `witty`, `teacher`, `professional` (text in `config::STYLE_PRESETS`); `custom` ≤ 2000 chars. Added to the system prompt of agents the user created (not to agents created by Iris, whose "user" is Iris) |
| GET, POST | `/v1/agents` | List / create agents |
| GET, PATCH, DELETE | `/v1/agents/{id}` | Read / partial update / delete (cascades) |
| GET, POST | `/v1/agents/{id}/threads` | List (most recent first) / create (`{"title"?}`) |
| GET | `/v1/threads?limit=N` | Recently active threads across all agents |
| GET, PATCH, DELETE | `/v1/threads/{id}` | `{thread, messages, running}` / rename / delete |
| POST | `/v1/threads/{id}/messages` | `{"content"}` → 202 `{user_message, assistant_message}`. The reply streams as events; 409 if one is already running |
| POST | `/v1/threads/{id}/regenerate` | Replace the last reply with a new one (or answer a user message that has no reply) |
| POST | `/v1/threads/{id}/stop` | Stop the running reply. Partial text is kept, with status `cancelled` |
| GET | `/v1/tools` | Toolsets an agent can enable (`id`, `name`, `description`, `needs_approval`) |
| GET | `/v1/approvals` | Tool calls waiting for the user |
| POST | `/v1/approvals/{id}` | `{"approve": true\|false}`: resumes the paused reply |
| GET | `/v1/audit?limit=N` | Tool audit log, newest first |
| GET, PUT | `/v1/settings/search` | `{provider: auto\|tavily\|duckduckgo\|deepseek, has_tavily_key, has_deepseek_key, active}` / `{provider, tavily_api_key?}` (omit the key to keep it, `""` to remove it) |
| GET, POST | `/v1/devices` | List (each with `local`: the desktop app's own access) / create (`{"name"}` → token shown once) |
| DELETE | `/v1/devices/{id}` | Revoke a device (409 for the local one) |
| GET (WS) | `/v1/events` | Live event stream |
| GET | `/v1/search?q=…&agent_id=&limit=N` | Full-text search over every chat (SQLite FTS5, prefix matching, accents ignored) → `[{message_id, thread_id, agent_id, agent_name, role, created_at, snippet}]`, matches marked `«…»` |
| GET | `/v1/skills` | Skills `[{name, description, body}]` from `skills/<name>/SKILL.md` |
| PUT, DELETE | `/v1/skills/{name}` | `{description, body}` (the name becomes a slug) / remove |
| GET | `/v1/mcp` | Connected services: `[{name, transport, command, args, url, env_keys, header_keys, enabled, auto_approve, connected, error, tools}]` (secret values never come back) |
| PUT, DELETE | `/v1/mcp/{name}` | `{command, args, url, env?, headers?, enabled, auto_approve}` (empty secret values keep the stored ones) → reconnects and returns the list / remove |
| POST | `/v1/mcp/refresh` | Reconnect every server |
| GET | `/v1/voice` | `{available, provider, model, limits?}`: whether speech to text works (needs a Groq or OpenAI provider). `limits` is `{limit, remaining, resets_at}`: the free daily allowance of spoken turns as Groq last reported it in its response headers (kept in `voice-limits.json`; absent until the first spoken turn) |
| POST | `/v1/voice/transcribe` | `{data (base64), mime, language?}` → `{text}` (≤ 24 MB; empty text for silence). Errors are short and speakable; **429** means the provider's free limit is used up, 502 the speech service failed or couldn't be reached |

**Events** are JSON with a `type` field:
- `hello`
- `agent_updated`, `agent_deleted`
- `thread_updated`, `thread_deleted`
- `message_created`, `message_deleted`
- `message_delta` (`round` index plus `content` and/or `reasoning` pieces)
- `message_progress` (the reply's full `rounds` after a tool step started, finished or needs approval)
- `approval_requested` (`approval`), `approval_resolved` (`approval_id`, `agent_id`, `approved`)
- `run_started`, `run_finished` (`agent_id`, `thread_id`, `message_id`; these drive the "working" indicators)
- `providers_changed`
- `message_completed` (final message with status `complete`, `error` or `cancelled`, plus token usage)
- `lagged` (the client missed events and should refetch)

Replies are generated in the background, so they finish and get saved even if every client disconnects.

## Tools (Phase 3)

Agents enable **toolsets**:

| Toolset | Functions | Approval |
|---|---|---|
| `web_search` | `web_search` | — |
| `web_fetch` | `web_fetch` | — |
| `get_time` | `get_time` | — |
| `files` | `list_files`, `read_file`, `write_file` | — |
| `shell` | `run_command` | **every call** |

**How a reply works:** a reply is a loop of **rounds** (max 12):
1. Stream the model's thinking and text.
2. If it asked for tools, run them, waiting for approval where required, and send the results back.
3. Repeat until the model answers without tools.

Each round's reasoning, text and tool steps are stored in `messages.rounds`. They're replayed into the history on later turns, because DeepSeek's thinking mode requires earlier `reasoning_content` whenever tools are present. Every call is also written to `tool_log`.

**Workspaces:** files and commands are confined to `<data dir>\workspaces\<agent-id>\`:
- paths are relative only
- `..`, absolute paths and drive letters are rejected
- commands run with that folder as the working directory, with a 2-minute timeout

**Web:**
- `web_fetch` refuses `localhost`/loopback addresses, so agents can't reach the engine's own API. LAN devices are allowed.
- Search uses Tavily when a key is saved (`secrets/tavily.key`), otherwise DuckDuckGo's HTML page. **DeepSeek** (Settings → Web search, only when chosen, since it's paid at about $0.75 per 1,000 searches) uses DeepSeek's Search API (`/v1/tools/search`) with the key of the provider whose base URL is api.deepseek.com, and passes agents its long summaries with dates. With DeepSeek chosen, `web_fetch` also asks DeepSeek's Fetch API (`/v1/tools/fetch`) when the engine can't read a page itself: the request fails, the site answers an error (often a bot wall), or the page has almost no text or is a "verify you are human" wall. If DeepSeek can't read it either (its 404), the agent gets what the engine read. Switching to DeepSeek by chat asks the user first.
- Every agent's system prompt gets the current date and time.

## Team (multi-agent)

There is always one **main agent** (`role = "main"`): Iris, the Chief of Staff. It can't be deleted. It has the `team` toolset:

| Function | What it does |
|---|---|
| `list_agents` | The team with purpose, tools, busy/idle and who created whom |
| `create_agent` | A new specialist orb (next pastel color, `created_by` = creator). Can grant web, files and shell tools, but not `team`. One-off by default (`role = "temp"`); `keep: true` makes it a lasting team member |
| `delegate_task` | Starts a new thread in the target agent ("Task from Iris: …") with the task as the user message, and runs it in parallel. The thread id is the `task_id` |
| `check_task` | Waits up to `wait_seconds` (≤120) for the task to finish, then returns the agent's final answer (or "still working (activity)"). Collecting a finished result from a one-off agent you created removes that agent ~3 s after **your reply ends** (unless it was kept or is busy again). Failed or stopped tasks keep the agent |
| `keep_agent` | Turns a one-off agent the caller created into a lasting one (e.g. its result needs the user's input) |
| `dismiss_agent` | Deletes an agent the caller created. **Always asks for approval** |

Iris's instructions (`MAIN_SYSTEM_PROMPT`, plus the team rules added in `chat::system_prompt`) say: do simple things yourself (weather, a single search, quick facts), put agents only on long / multi-step / research-heavy or parallel work or when asked, and pass `keep: true` when the user asked for the agent. The stored prompt is upgraded on start only while it still equals a previous built-in version.

**Auto mode:** three levels per agent, picked from the chat box's Auto button: Ask me (off), Careful auto (`auto_approve`) and Full auto (`auto_approve` + `auto_full`). Agents get the stronger of their own level and their creator's. Agents it creates follow their creator's switch. With Auto on, every `run_command` goes through `autoapprove.rs`. First, fixed rules (checked on the command with quoted text removed, and on paths with URLs removed) sort it into tiers: *catastrophic* (secrets, reading or copying I.R.I.S.'s data folder, wiping disks, recursive deletes of drive roots, system folders or the user's main folders, shutdown, security or network off, user accounts, hidden/downloaded code, stopping I.R.I.S.) always goes to the user; *opaque* (commands built at run time, Add-Type, base64, COM/WMI, remoting) gets a strict review in Careful and a normal one in Full; *caution* (registry, services, installs, POST, other programs, writes outside the workspace…) is only pointed out to the reviewer. Then one model call (temperature 0, thinking off) answers `allow`/`ask` as JSON with a mode-specific prompt: Careful allows normal, safe steps of the task and asks about real damage (unclear means ask); Full only stops the catastrophic ("when in doubt, allow"; a failed or unclear check allows). The reviewer gets the first and latest user messages, the last few commands of the reply, and is told when outside content was read; a failed call is retried once, and a command that already ran in the same reply isn't reviewed again. Allowed steps get `auto_approved` (shown as an "Auto" badge) and "· auto-approved" in the audit log; the review's tokens go into the usage ledger. `dismiss_agent` always asks.

**Approvals and follow-ups:** `check_task` returns as soon as the delegated agent is waiting for approval, so the coordinator can tell the user. The app shows approvals from chats you aren't viewing as pop-ups (Approve / Deny / Approve + Auto / open the chat). `delegate_task` records a follow-up (task thread → coordinator thread); when the task ends after the coordinator's reply has ended, `team::after_reply` posts an automatic `[I.R.I.S. update]` message to the coordinator's thread and starts a new reply, so Iris collects the result and reports without being asked. The app shows that message as a small status line.

**Long tasks:** a reply may use 25 tool rounds; after the last one the model is asked once more with `tool_choice: "none"` for a final answer from what it has, instead of failing.

**Iris's chat:** the main agent's tool steps fold into one box per reply (the helpers' orbs, a live status such as "Site Auditor is working · run command" or "needs your approval"); click it for every step. Steps waiting for approval stay outside the box.

**Model and effort picker (chat box):** one button ("DeepSeek V4.1 Flash · High") opens a menu with the models and the thinking effort. It changes the app-wide default (the same settings as Settings → AI providers: `PUT /v1/settings/model`, `PUT /v1/settings/reasoning`), so a choice sticks for every chat and agent. Only an agent that has its own model or level (from its editor) is changed individually, with a "Use default" option. The default reasoning level is High when never chosen. `deepseek-flash` is DeepSeek V4.1 Flash (per DeepSeek's pricing page); the pickers show that next to the id.

**One chat per agent:** every agent (Iris included) has exactly one ongoing chat (`GET /v1/agents/{id}/chat`, created on first use; `POST …/chat/clear`, `POST …/chat/compact`). On start the engine keeps only each agent's most recent thread. Delegated tasks go into the target agent's chat; the `task_id` is the reply's message id, and a busy agent can't take a second task.

**Compaction:** before each reply, if the last reply's context was over `min(60% of the model's window, 100k)` tokens (32k window assumed for unknown models), everything except the last ~6 messages (cut at a user message) is summarized by one extra model call (thinking off) into `threads.summary`, with `summary_until` marking what it covers. Those messages stay in the database and the app (with a "compacted" divider) but are no longer sent; the summary goes into the system prompt instead.

**Memory files:** `agents/<id>/SOUL.md` mirrors the agent's instructions (edited by hand → wins on the next start), `agents/<id>/MEMORY.md` holds its own notes (≤3000 chars) and `memory/USER.md` what all agents know about the user (≤2000 chars). Every agent has the `memory` tool (add / replace / remove, target `memory` or `user`), and both files go into every system prompt. Editable in the agent editor and Settings → Personality & memory (`GET/PUT /v1/agents/{id}/memory`, `/v1/memory/user`).

**Right-click menus:** orbs (open, edit, compact, clear, delete) and anywhere else (home, new agent, settings, theme, refresh credit, reload); text fields and selected text keep the normal menu.

**Designing agents:** `POST /v1/agents/design {request}` asks the default model (thinking off, one short call) for a name, description, tools (web_search, web_fetch, get_time, files, shell) and 60–120-word instructions, then creates the agent. The app's "+" opens a one-sentence dialog for this; the full form is behind "Set it up by hand".

**Token efficiency:** system prompts put stable text first and what changes (memory, summary, the clock) last, so DeepSeek's prefix cache hits (logged as "N prompt tokens, M from cache"; ~93% on a second message). Every agent is told to be brief and think only as much as needed. Tool results from earlier replies are re-sent at most 1,500 characters. Chats compact at 64k tokens. DeepSeek effort: Low → `low`, Medium/High → `high`, Max → `max`.

**Connectivity:** the engine pings the default provider every 30 s; `/v1/status` has `online` and a `connectivity` event fires on change. The app shows a red bar at the top while the engine or the provider can't be reached.

**This computer (`system` toolset, `system.rs`):** `system_status`, `app_updates`, `search_apps`, `windows_updates` (check) run freely; `update_apps` and `windows_updates` with install ask unless Full auto; `install_app`, `uninstall_app` and `power` (restart / shutdown / cancel, with a delay) always ask. winget runs as the user unless the optional **IRIS Helper** exists: an elevated scheduled task (`scripts/remote/iris-helper.ps1`, installed by `scripts/remote/setup-host.ps1` into `C:\ProgramData\IRIS\helper`) that only performs these named actions from request files in its queue. `setup-host.ps1` also registers **IRIS Engine (boot)** (S4U, at startup) so agents run before anyone logs in. Neither is installed by default: it needs the user's OK.

**Browser (`browser.rs`):** one headless Edge (already installed) over the DevTools protocol, profile in `browser/` so logins persist, one tab per agent (max 3), closed when the last tab closes or after 5 idle minutes (~500 MB while open). Tools: `browser_open/read/click/type/scroll/back/close`; pages come back as text plus numbered elements (`data-iris-n`). Local/LAN addresses are blocked, password fields are never typed into, and clicking anything labelled like pay/buy/order/checkout (English or Greek) always asks.

**Attachments:** the chat box's "+" (or paste / drop) uploads files with `POST /v1/threads/{id}/attachments {name, data(base64)}` (≤20 MB, stored in `attachments/<thread>/`), then sends them with the message (`attachments` in `POST …/messages`). Images go to the model as `image_url` parts (DeepSeek V4.1 Flash sees them; only the last 3 messages with files re-send images), text/HTML/PDF go in as text (full for the last 2, 2,000 chars for older). `GET /v1/attachments/{thread}/{id}?token=` serves them to the app. Agents can also look at images on disk with `view_image` (files toolset; not the app's data folder).

**Settings by chat (`settings.rs`, toolset `settings`, Iris has it):** `get_settings {section}` returns every setting as compact JSON (never secrets: the API doesn't return keys or passwords). `change_setting {setting, value, agent?}` covers style, reasoning, token limits, search, remote access, each agent's name / description / instructions / reasoning / auto mode / tools, revoking devices and removing providers (models and the fallback stay with `switch_model`). Both go through the engine's own router as this computer's device, so validation and events match the app. Remote access, devices, providers, instructions, Full auto, adding shell/files/system/browser/settings and removing any tool always ask, even in Full auto.

**Abilities (`tools::effective_tools`):** every agent has the built-in abilities without a toggle (`BUILT_IN`: web search, web pages, browser, time, routines, team, models, settings). Only two can be switched on in the editor: **Files** (`files`) and **Computer control** (`shell` + `system`). Iris (role `main`) has every toolset and no toggles. Team for non-Iris agents means talking to others (`list_agents`, `delegate_task`, `check_task`, including to Iris); creating, keeping and dismissing agents (`COORDINATOR_ONLY`), the specialist roster and the hand-off guard (now only for the system/shell family) stay with Iris. **Asking for tools:** every agent but Iris has `request_tools {tools, reason}` (files / shell / system); Iris reviews it (`design::review_request`, same review as for new orbs), granted tools are saved and offered from the next step of the same reply.

**Default auto mode for new agents:** `[defaults] auto_mode` (off / careful / full; `GET/PUT /v1/settings/auto`, Settings → Approvals, which also lists every agent's own mode). Every agent created through the API ("+", designed) or by a coordinator starts with it (`AppState::apply_default_auto`); its own auto picker changes it later. Iris can change it by chat (`change_setting default_auto`; full asks first).

**Summoning an orb ("+"):** `POST /v1/agents/blank` creates "New agent" (role `new`, no tools) and the app opens its chat. Its first message decides what it becomes (`design::shape`): the designer picks name, description, instructions and the tools it wants; Iris then reviews the tools (`design::audit`, one short model call; shell / system / browser / files only if the job needs them, never more than asked; without a review only the safe ones stay). The agent is saved, a `[I.R.I.S. setup]` note (shown as a small line) tells it and the user what it got, and it answers the first message as the new specialist. Further messages queue meanwhile. Right-click menus: an agent's card on Home has the agent menu; the app menu no longer has Settings or Refresh credit.

**Specialists first:** a coordinator (an agent with `team`) gets its specialists (top-level agents, not Iris) listed right after its own instructions, with the rule to delegate anything in a specialist's area, even simple things. Prompts alone weren't enough, so the engine also checks: the first time in a reply that a coordinator calls a system / shell / files / browser tool that an idle specialist also has, the call isn't run and the coordinator is told to delegate; calling it again (when the user asked it to do it itself) goes through. The app's dropdowns are `Select.svelte` (no native lists).

**Left bar and orbs (app):** drag orbs to reorder (pointer events; Iris stays first; the order is per device in `localStorage` `iris.orbOrder`, also used on Home). Drag the bar's edge to resize it (60–150 px, `iris.railWidth`; double-click resets); orbs scale with it. Orbs look around and turn their head now and then; working they frown and squint, thinking they look away, team tools make them turn all the way round. The credit sits top right (chat header / Home) and its 30 s refresh pauses after 10 minutes with no input and no agent working.

**Queue and Steer (`queue.rs`):** a message sent to a busy chat waits in that chat's queue (`POST /threads/{id}/messages` returns `queued` instead of `assistant_message`; up to 20). When a reply ends, the next queued message goes out first, then any team updates. `POST /threads/{id}/queue/{item}/steer` puts one first and stops the current reply; `DELETE /threads/{id}/queue/{item}` removes one. Queues are in memory (`GET /v1/status` → `queues`, event `queue_changed`). The app shows them above the chat box; typing while an agent works queues the message (Stop shows only while the box is empty).

**Delegation doesn't block:** after `delegate_task` the coordinator tells the user who's on it and ends its reply, so the user can keep chatting; the result comes back as an `[I.R.I.S. update]`. `check_task` looks right away by default (`wait_seconds` max 60).

**Routines (`routines.rs`):** a scheduler checks every 20 s. A "prompt" routine posts `[Routine: name] task` into the agent's chat and starts a reply (the app shows it as a small note); a "command" routine runs PowerShell without the AI and only wakes the agent on failure (or every run with notify "always"). Schedules: `every 30m A `once` routine removes itself after it runs (a failed command stays, switched off); finished one-offs left by older versions are removed when the engine starts.|2h|1d` (≥5 min), `daily HH:MM`, `weekdays|weekends HH:MM`, `weekly mon HH:MM`, `once YYYY-MM-DD HH:MM`, local time. Agents use `create_routine/list_routines/delete_routine` (creating a command routine asks unless Full auto and it isn't catastrophic); the app has Settings → Routines (`/v1/routines`; `PATCH` edits any field, `POST …/run` runs now). With the boot-time engine, routines run even before anyone logs in.

**Account vault (`vault.rs`):** Settings → Saved logins saves logins (name, site, username, password) in `secrets/vault.bin`, encrypted with Windows DPAPI (machine scope + app entropy, so the boot-time engine can read it). The API never returns passwords (`GET /v1/vault` gives `has_password`). Agents see only names and sites (`list_accounts`); `browser_login {account}` fills the username/password inside the agent's tab, only if the page's host is the account's site or a subdomain (look-alikes fail), marks the fields so page listings never show their values, and asks first unless Full auto. Paying/ordering clicks always ask.

**I.R.I.S. Link (remote access, `link.rs`):** when Settings → Remote access is on (`[link] url, enabled` in iris.toml; `GET/PUT /v1/link`), the engine keeps an outgoing WebSocket to the relay (`relay/`, `<url>/link/server` with the key from `secrets/relay.key`; reconnects with backoff, `link_changed` event). Each phone/browser on the relay is a session:
- Handshake: client `{"t":"hello","v":1,"epk"}` (P-256, base64url SEC1), engine `{"t":"welcome","v":1,"epk"}`. Keys = HKDF-SHA256(salt `iris-link-v1`, ECDH(client, engine identity) ‖ ECDH(client, engine ephemeral), info = client pub ‖ engine ephemeral pub) → 32 bytes client→engine, 32 bytes engine→client. The identity key is `secrets/link-identity.key`; clients pin its public half (in the QR / pairing link).
- Then only sealed frames `{"s":counter,"b":base64}`: AES-256-GCM, nonce = 4 zero bytes + counter (big endian), AAD `c2s`/`s2c`; counters must increase (replays are dropped and the client kicked).
- Sign-in (sealed): `auth {token}`, `pair {code, name}` (codes from `POST /v1/link/pair`: 8 characters, 10 minutes, once) or `login {user, password, name}` (`PUT/DELETE /v1/link/login`; Argon2id in `secrets/link-login.json`; after 5 wrong tries in a row each try waits 5 s, doubling up to 60 s, so a stranger can't keep the owner out for long). A client must sign in within 20 s of connecting and gets 3 tries per connection, then it's kicked; the relay allows 5 connections per IP (20 in all). Pairing and login create a device (Settings → Paired devices, `device_added` event) and return its token once → `{"t":"signed_in","device","token"?}`.
- After sign-in: `{"t":"req","id","method","path","body"}` runs through the engine's own router as that device → `{"t":"res","id","status","ct","body"|"b64"}`; live events arrive as `{"t":"ev","e":…}`. `/v1/events` isn't reachable over the link (events come anyway).
- Web app (R2): the relay serves `app/` built with `npm run build:web` (mode `link`, `VITE_IRIS_LINK=1`, output `relay/public/`, ignored by git; `npm run deploy` in `relay/` builds and deploys). In that build `IrisApi` sends every request through `LinkConnection` (`app/src/lib/link.ts`: WebCrypto handshake, sealed frames, reconnect with backoff, keep-alive pings, GETs wait up to 20 s through a reconnect). The sign-in page (`LinkSignIn.svelte`) pairs from the `#pair=…&k=…` link, a typed code, or username + password (the engine key then comes from `GET /link/key`, which the relay stores from the engine's `X-Iris-Public-Key` header). The pairing is kept in `localStorage` (`iris.link`); a revoked device lands back on the sign-in page. The relay sends `{"relay":"offline"|"online"}` notices; on "online" clients start a fresh session.
- Layout: below `md` (768 px) the rail shows only on Home, chats are full width with a back button, modals are full screen, and Settings tabs scroll sideways.
- Test: `node relay/test/link-test.mjs '<pairing link>'` pairs, calls the API, waits for an event, signs in again with the token, removes its device.

## Recall, skills, heartbeats, research, connections, voice

- **Chat search** (`recall.rs`, store migration v13): `messages_fts` follows `messages` through triggers (content is written once per message, so it's cheap). Cleared chats leave the index; compacted ones stay searchable. Agents use `search_chats`; the app has Ctrl+K (and the search button on the rail), which opens the chat at the message.
- **Skills** (`skills.rs`): every agent gets `use_skill` / `save_skill`; names and descriptions (up to 40) go into the system prompt, the instructions are loaded on demand. agentskills.io format, so skills copied into `skills/` by hand work too. Settings → Skills.
- **Heartbeat routines** (`routines.rs`, kind `heartbeat`): the agent goes through a checklist; a reply of exactly `HEARTBEAT_OK` removes the trigger and the reply from the chat (`last_result` "all clear"), anything else stays for the user.
- **Deep research**: the composer's Research switch prefixes the message with `[Deep research]` (`chat::RESEARCH_PREFIX`); that reply gets research instructions in its system prompt and 45 steps instead of 25.
- **Connections / MCP** (`mcp.rs`): servers in `secrets/mcp.json` (`mcpServers`, same format as other apps, plus `enabled` and `autoApprove`), stdio or Streamable HTTP, connected at start and on every change. Tools appear as `mcp__<server>__<tool>` for Iris and agents with the Connections switch (`mcp` toolset). A call asks unless the server marks the tool read-only or it's in `autoApprove` (`"*"` = all). A program that exits is reconnected by the next refresh.
- **Voice** (`voice.rs`, `app/src/lib/voice.svelte.ts`): the mic records with MediaRecorder and `/v1/voice/transcribe` sends it to the first Groq (`whisper-large-v3`) or OpenAI (`gpt-4o-mini-transcribe`) provider with a key. Replies are read aloud with `speechSynthesis` (Windows voices, free, offline): the speaker button on a reply, and automatically after a dictated message. Phone calls are in [Voice calls](#voice-calls-phone--engine--groq) below.
- **Self-development** (`develop.rs`, the `dev` toolset, the **Developer** agent, created once at start): the agent works in a git repo on the server, `C:\IRIS\repo` (`IRIS_REPO` overrides), on a branch `iris/<time>`. `code_status/search/read/edit/write/diff/discard` are free (only the branch changes). `code_apply` always asks, with the full diff in the approval card (`PendingApproval.detail`), then commits and runs `cargo test`, svelte-check and an AI review of the diff. `code_install` always asks, builds the installer from the repo, keeps the running build's installer as `C:\IRIS\releases\previous-setup.exe`, and starts `scripts/remote/self-update.ps1` through Task Scheduler: install, wait up to 2 min for `/health`, roll back if it doesn't answer, write `C:\IRIS\releases\update-result.json`. The new engine (`develop::resume`, via `dev-update.json` in the data dir) merges the branch into `main`, posts a `[I.R.I.S. release]` note, and has a one-off **Release Checker** (read-only code tools) run `code_verify` and report to the Developer. Iris has no code tools; she hands code requests to the Developer.
- **Prompt-injection guards** (`chat.rs`): after a reply has read web pages or a connection's output (`WEB_CONTENT_FUNCTIONS`, `mcp__*`), lasting changes (`LASTING_FUNCTIONS`: settings, models, routines, tool requests, skills, memory) ask the user (in Full auto, model switches, tool requests and memory don't: `LASTING_OK_IN_FULL`), and the auto-mode reviewer is told to watch for steered commands. Commands that fetched URLs don't count as the user's private text for the leak check. Replies never load remote images (Markdown images become links).

## Notifications, question cards, problems inbox

- **Notifications:** only the phone app notifies. Its background link connection turns engine events into notifications (see the Android section). The engine has no push service of its own.
- **Question cards**: every agent has `ask_user {question, options[2-6], allow_other}`. The reply ends right after it (`chat.rs`), the app draws the options (`QuestionCard.svelte`), and the user's pick goes out as their next message.
- **Problems inbox** (`problems.rs`, store migration v14): a failed reply or a failed command routine is recorded, notified, and shown in the rail's red badge, on Home and in `ProblemsPanel.svelte`. Retry re-asks the reply (only while it's still the chat's last message) or runs the routine again; a good reply in that chat clears its problems by itself. API: `/v1/problems`, `/v1/problems/{id}/retry`, `/v1/problems/{id}/dismiss`, `/v1/problems/dismiss`.

## Models

Every agent's system prompt says which model and provider it is running on right now (and the fallback), so "what model are you?" gets a true answer. The main agent also has the `models` toolset:

| Function | What it does |
|---|---|
| `list_models` | Default model, fallback, agents with their own model, and each provider's live model list (≤60 ids each, 10 s timeout) |
| `switch_model` | `{model, provider?, target?}`. `target`: `all` (default model), an agent name or `me` (its own model; `model: "default"` resets it), `fallback`, `fallback_off`. Without `provider` it uses the provider whose model list has that id (the default's first). Takes effect from the next reply |

Providers: any OpenAI-compatible API. The app's "Add provider" offers presets (DeepSeek, OpenAI, Anthropic via its OpenAI-compatible endpoint, Gemini, OpenRouter, Groq (speech to text only, never a chat model), Mistral, xAI, Together, Ollama, LM Studio, custom). Earlier reasoning (`reasoning_content`) is only sent back to DeepSeek. Local providers (localhost) work without a key.

**Limits:**
- At most **10 agents** (`MAX_AGENTS`), enforced in the API and in `create_agent`.
- **Token limits** (`[limits] daily_tokens / weekly_tokens` in iris.toml, 0 = off; `GET/PUT /v1/settings/limits`) are checked before every model call. Usage counts all replies started today (local midnight) and this week (since Monday). Note: one research-style team task can use ~100k+ tokens, because every step re-sends the history including fetched pages.

**Context:** every finished reply stores `context_tokens` (prompt + completion of its last model call), i.e. how full the conversation is. `GET /v1/settings/model` returns `context_window` (`[defaults] context_window` in iris.toml, else a known value: 1M for the DeepSeek models). The composer's ring shows that ratio; clicking it shows the context plus the daily and weekly limit bars.

**Live activity:** each run records its `activity` ("thinking", "waiting", or the tool name). It's included in `/v1/status`, and every change is pushed as `run_activity`. `run_finished` carries the final `status`. The app uses these for the orbs' activity bubbles and moods:
- happy after `complete`
- sad after `error`
- a spin when delegating
- a pop-in for new agents


## Android app (`android/`)

A native app in Kotlin + Jetpack Compose (Foundation only, no Material), built on the PC. It only talks to the engine through I.R.I.S. Link, like the web app, and looks like the web app's phone layout. Release APK ≈ 3 MB.

```bash
cd android
./gradlew assembleDebug        # app/build/outputs/apk/debug/app-debug.apk
./gradlew assembleRelease      # shrunk (R8); signed with keystore.properties if present, else the debug key
adb install -r app/build/outputs/apk/release/app-release.apk
```

- **Link** (`link/LinkCrypto.kt`, `link/Link.kt`): the same handshake, sealed frames and reconnect logic as `app/src/lib/link.ts`, using the platform's own crypto (ECDH P-256, HKDF-SHA256, AES-GCM) and OkHttp's WebSocket.
- **Pairing:** scan the QR with Google Play services' scanner (no camera permission), paste the pairing link, or type the relay address with a code or username + password. The pairing (relay, engine key, device token) is stored sealed with an Android Keystore key.
- **State** (`data/IrisViewModel.kt`): the counterpart of `app.svelte.ts`: a full refetch on every (re)connect, then live events (deltas, progress, approvals, runs, queues). The app reconnects right away when it comes back to the front.
- **UI** (`ui/`): Home is a chat list (greeting, approvals, then each agent's live orb, last line from `/v1/previews`, time and an unread dot; long-press for the agent menu). Chat follows the desktop refresh: violet bubbles for you with the time in the corner, soft grey ones for agents, day dividers, a pill message box (Research, Stop), and the model/thinking and auto pickers in the header. Every agent's tool steps fold into one work box on the phone; inline approvals, queue with Steer, Markdown with inline-code chips and copyable code blocks. New messages rise into place; approvals from other chats pop in at the top. Screens slide in and out along a back stack (`IrisViewModel.stack`); menus and pickers are bottom sheets drawn in the app's own window (`Menu.kt`, `LocalOverlays`). Lucide icons are vectors in `Icons.kt`.
- **Orbs** (`Orb.kt`, `data/Orbs.kt` = `app/src/lib/orbs.ts`): the desktop's moods (thinking, searching, reading, working, team, reviewing, waiting, done, failed, unread, asleep) with faces that morph between expressions, body motions and effects (thought dots, z's, sweat drop, sparkle burst, reply ping), a step chip in the chat list and a speech bubble under the chat header with the step in words. Unread and asleep are client-side: a reply that lands while its chat isn't on screen nudges until opened; ten minutes untouched or in the background puts idle orbs to sleep, and a touch wakes them with a stretch. All of it is drawn from one shared clock that runs only while an orb is on screen: busy orbs at the full frame rate, idle ones at ~30 fps, still when Android's "Remove animations" is on.
- **Settings** (gear on Home): Models (default model from the live list, reasoning, auto mode for new agents, fallback), Personality & memory (style, own words, USER.md), Routines (add, edit, on/off, run now, delete), Skills (edit), Connections (on/off, reconnect), Limits, Search (incl. the Tavily key), Activity, Devices (revoke), Remote access (status, a one-time pairing code for another device), About (unpair). Providers and Accounts are read-only here: API keys and vault passwords are entered on the server. **Agent settings** (agent menu → Edit agent): name, description, color, model, reasoning, auto mode, abilities, SOUL.md, MEMORY.md.
- **Background:** the link closes a minute after the app leaves the screen (no pings all day) and reconnects when it's back.
- **Questions, problems, notifications:** an agent's `ask_user` question shows as a card with answers to tap (or your own words), sent as your next message. Failed replies and routines show as a banner on Home and an inbox (Retry, Open chat, Dismiss). Notifications (`data/Push.kt`): while the app is in the background, `LinkService` keeps the link to the server open (a foreground service) and turns its events into notifications in per-kind channels (replies, questions, approvals, problems); a tap opens that agent's chat. No push service is involved. Notifications stay quiet while the app is on screen. Which kinds to show is chosen in Settings → Notifications and kept on the phone. Relay addresses (typed or from a pairing link) are always used over https.
- **Attachments:** the + in the message box offers Photos (Android's photo picker: only the chosen photos are shared, no permission), Take a photo (the camera app, through a FileProvider in the cache) and Files (any file). Up to 10 per message; photos are re-encoded as JPEG at most 2048 px on the long side, other files may be up to 8 MB (a file travels base64 inside a sealed, base64 link frame, and OkHttp won't queue a frame over 16 MB). Each is uploaded with `POST /v1/threads/{id}/attachments`, then the message carries them.
- **Voice:** Settings → Voice & calls and calls from the chat header's phone button (see [Voice calls](#voice-calls-phone--engine--groq)).
- **Not yet:** fingerprint lock, share target (sending from other apps).

## Voice calls (phone → engine → Groq)

Plan and reasoning: [VOICE-PLAN.md](VOICE-PLAN.md). A call is the agent's normal chat; only the edges are new.

```
phone: mic → echo cancel → WebRTC VAD → one WAV clip per spoken turn
   → POST /v1/voice/transcribe (through I.R.I.S. Link, sealed) → engine → Groq Whisper → text
   → POST message "[Voice call] text" → agent replies, streamed as message_delta
   → phone: sentence splitter → Android TextToSpeech
```

The Groq key never leaves the server; the phone never talks to Groq. **Groq is speech to text only**: `providers::is_voice_only` (any `groq.com` address) keeps it out of the default model, the fallback, an agent's own model, `switch_model` and the model pickers (`voice_only` on `GET /v1/providers`).

**Engine:**
- `chat.rs`: `VOICE_PREFIX` (`[Voice call]`). When the latest user message starts with it, `VOICE_INSTRUCTIONS` is added to the system prompt (short, speakable answers in the user's language; code, tables and long reports go after a line that is exactly `[details]`; approvals can't be given by voice).
- `voice.rs`: Groq model `whisper-large-v3`; the agents' names are sent as Whisper's `prompt` so they are spelled right; transcripts that are only one of Whisper's silence phrases ("Thank you.", "Ευχαριστώ.", subtitle credits) become empty text; errors are short because the phone speaks them (429 → "Groq's free limit is used up for now; try again in a minute").
- Whisper's own doubt is used (`response_format=verbose_json`): a stretch with `no_speech_prob > 0.8` (or > 0.5 with unsure words), a `compression_ratio > 2.4` loop or an `avg_logprob < -1.6` guess is dropped, which removes the invented "Thank you." on silence and noise. Repeated silence phrases are dropped too.
- English and Greek only: if Whisper detects another language, the clip is asked again for `en` and for `el` and the surer answer is kept. The call instructions tell the agent never to answer in another language.
- Free tier (Nov 2026): 20 requests/min, 2,000/day, 7,200 audio s/hour; each request counts as at least 10 s. The remaining count comes from `x-ratelimit-*-requests` headers and is shown as a bar in Settings → Voice & calls.

**Both UIs:** a user message starting with `[Voice call]` shows a phone chip; a `[details]` line in a reply shows as a divider (`app/src/lib/details.ts`, `data/Details.kt`).

**Android files** (`android/app/src/main/java/dev/iris/app/`):

| File | What |
|---|---|
| `data/Recorder.kt` | `AudioRecord` (VOICE_COMMUNICATION, 16 kHz mono) with echo canceller and noise suppressor; `Wav` wraps PCM in a WAV header |
| `data/VoicePrefs.kt` | Settings → Voice & calls choices (SharedPreferences file `iris`) |
| `data/Speaker.kt` | `TextToSpeech` wrapper: voices, availability, a queue of sentences, `stop()` |
| `data/SpeechText.kt` | `SpeechStream`: streamed reply text → sentences (splits at `. ! ? … ; ·` and line breaks followed by a space; not inside numbers, URLs or abbreviations; skips code; stops at a `[details]` line); `languageOf` (any Greek letter → Greek voice) |
| `data/VoiceDetector.kt` | `VoiceDetector` interface and the WebRTC VAD behind it (`com.github.gkonovalov.android-vad:webrtc:2.0.10`, from JitPack, taken only for that group), so Silero can replace it later |
| `data/CallLogic.kt` | Pure, unit-tested parts: `TurnDetector`, `callPhase`, `ProgressGate`, `ReplySpeech`, error classes, route choice |
| `data/CallAudio.kt` | Audio mode `MODE_IN_COMMUNICATION`, audio focus, routing (earpiece, loudspeaker, Bluetooth), proximity wake lock |
| `data/CallController.kt` | The call: audio loop, turn-taking, transcription, posting, speaking, events, errors |
| `data/CallService.kt` | Foreground service of type `microphone` (ongoing "Call with Iris" notification with a timer and Hang up) |
| `ui/VoicePage.kt`, `ui/CallScreen.kt` | Settings → Voice & calls (status, language, voices, call options, "Try it") and the call screen |

**Call flow.** The microphone runs as 20 ms frames through a `TurnDetector`:
- A turn starts after 0.25 s of speech (a few quiet frames inside it are forgiven) and ends after the "Wait before answering" silence (0.5 / 0.8 / 1.2 s) or at 30 s. 0.3 s from before the speech is kept.
- While the agent speaks (and 0.3 s after), the rule is stricter against echo: 0.4 s of steady speech. That also interrupts: the speaker stops at once, the rest of that reply isn't spoken, and the agent's work goes on. "Interrupt by talking" off ignores the microphone while it speaks.
- Each turn is sent to `/v1/voice/transcribe` in order. Results shorter than three letters or digits are dropped. On 429 the call says "I've hit the free speech limit; give me a minute." and sends the same clip again after 30 s; if the link is down it says "I lost the connection to the server." and retries.
- The text goes out as `[Voice call] …` with the normal send, so a busy agent's queue takes it. Phases: Listening → You speaking → Thinking → Speaking.
- Replies: `message_delta` content (never reasoning) goes through `ReplySpeech`/`SpeechStream` into the speaker; a new round flushes the last unfinished sentence. While working, a short phrase from `Orbs.kt` is spoken when the step changes (at most one every 8 s, never over a reply sentence, and only if "Say what I'm doing" is on).
- Approvals: "I need your OK on the screen." and the card on the call screen. They are never given by voice. An `ask_user` question is spoken, its buttons show on the screen, and a spoken answer is simply the next turn.
- Keeping the call: `IrisViewModel.pause()` doesn't close the link while a call is on; `Push.inCall` stops the background notification link from posting a notification per reply; hanging up (screen, notification button, swiping the app away) restores the audio mode and stops the service.

**Desktop and web app** (`app/src/lib/call/`): the same call in the Svelte app (so in the desktop app and the relay's web app). `logic.ts` is a one-to-one port of the phone's pure parts (sentence splitter, turn detector, filters, WAV) with `logic.test.ts` (`npm --prefix app test`, Node's own test runner). `call.svelte.ts` records with `getUserMedia` (the browser's echo cancelling, noise suppression and gain control on), resamples to 16 kHz in a ScriptProcessor (a worklet would need a script URL the strict content policy blocks), and detects speech with `EnergyDetector` (an adaptive noise floor; there is no WebRTC VAD in the browser). Replies are spoken with `speechSynthesis`. Because the computer's own voices often play outside the browser's echo canceller, an interruption while it talks must also be loud (`INTERRUPT_LEVEL`). UI: the chat header's phone button, `CallView.svelte` (Space stops, M mutes, Esc goes back to the chat), `CallBar.svelte` while it goes on behind, and Settings → Voice & calls (`VoiceSettings.svelte`; choices in `localStorage` `iris.voice`).

**Phone-side guards** (`CallLogic.kt`): a turn is only sent if it has at least 0.3 s of speech making up a fair part of the clip and its loudest frames reach about -41 dBFS (`isWorthTranscribing`); closing lines like "Thank you." are dropped (`isPhantomPhrase`, the same list as the server, so it works with an older server); text in a script other than Latin or Greek is dropped both ways (`hasForeignScript`); and what the phone itself said a moment ago is not taken as you (`looksLikeEcho`).

**Call screen layout** (`ui/CallScreen.kt`): `CallContent` has no app state. The orb gets the height left after the header, status line, captions and buttons and is drawn to fit it, so it is never clipped; approvals sit above the captions, which scroll in their own area. The glow is `CallBackdropLayer`, a layer behind the whole window (under the status bar). Debug builds include `CallPreviewActivity`, which shows `CallContent` with made-up data: `adb shell am start -n dev.iris.app/.CallPreviewActivity --es phase speaking --ez long true --ez approval true` (phase: listening, you, thinking, speaking; also `muted`, `trouble`).

**Testing.** `./gradlew testDebugUnitTest` covers WAV, settings, the splitter, turn detection (with a scripted detector), call phases, progress rate limits and reply speech. On the emulator the microphone works (Extended controls → Microphone → use the host's), the WebRTC VAD and the whole path to Groq can be exercised, but **test echo cancellation, Bluetooth and the proximity sensor on a real phone** (the emulator has none of them). Engine side: `cargo test` has the prefix, silence-phrase, error and Groq-is-voice-only tests; check the real thing by pressing the mic in the desktop message box, then **Try it** in Settings → Voice & calls on the phone.
