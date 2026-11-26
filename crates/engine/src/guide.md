# I.R.I.S. guide

## Overview
I.R.I.S. (Intelligent Reasoning & Initiative System) is a self-hosted team of AI agents. The engine runs on the user's own always-on Windows server; the desktop app, the web app (through the relay) and the Android phone app all talk to that engine. Iris is the coordinator ("Chief of Staff"); the user can add up to 10 agents in total, each a specialist with its own chat, instructions, memory, model and abilities. Everything (chats, keys, settings) stays on the server. The project is open source (MIT) on GitHub: Joji228/IRIS-Intelligent-Reasoning-Initiative-System.

## Iris and the agents
- Iris handles anything and hands work to the right specialist (delegate_task), then reports the result when it's done. She can create agents: one-off helpers that are removed after their task, or kept ones for recurring work.
- New agent: the "+" (sidebar or home screen) makes a blank orb that becomes a specialist from its first message (it picks its own name, specialty and color). At most 10 agents.
- Edit an agent: the pencil in its chat header, or right-click / long-press it → Edit agent. Name, description, color, model, abilities, personality (SOUL.md) and long-term memory (MEMORY.md).
- Reorder agents by dragging them in the sidebar or on the home screen cards (desktop); Iris always stays first.
- Abilities that can be switched on per agent: Files (its own workspace folder on the server), Computer control (PowerShell commands, server health, app updates/installs, Windows Update, restart), Develop I.R.I.S. (change the app's own code; every change shows a diff and asks), Connections (MCP services). Always on for every agent: web search and pages, a browser, date and time, routines, the team, models and settings, memory, chat search and skills. An agent that lacks a tool can ask Iris for it (request_tools).
- Agent menu (right-click on desktop, long-press on the phone): open, edit, compact the chat, clear the chat, delete.

## Chats
- One endless chat per agent. Attach photos or files with "+" (or paste / drop them). Deep research (the telescope button) makes the agent search and read much more before answering.
- While an agent is busy, new messages wait in a queue; "Steer" sends one right away to redirect it.
- Each reply can be copied, read aloud, or regenerated (the last one).
- An agent's tool steps (commands, files, searches) are folded into one card per reply with a tally; click it to see every step. Tasks Iris hands an agent appear as a "Task from Iris" card.
- Search all chats: the search button in the sidebar, or Ctrl+K on desktop.
- Questions from an agent appear as a card with choices; picking one answers it.
- Visuals (Intelligent UI): when a chart, comparison, plan or small tool explains something better than text, a reply can include an interactive visual card (charts, stat tiles, tabs, checklists, sliders). Answer buttons and small forms in a card send your pick as your reply, only when you tap them. It runs in a sandbox with no internet access. The code button shows its HTML, the expand button opens it full screen. Voice calls stay text only.

## Thinking effort
- How hard an agent thinks: a slider with Off, Low, Medium, High and Max, plus an Auto pill (the model decides). Higher levels answer more carefully but are slower and use more tokens.
- The default for all agents is in Settings → Models, and the same slider is in the chat's model menu (the "· High" button in the chat header). An agent can have its own level; then the pill reads "Default" and goes back to the shared level.
- On DeepSeek, Medium works like High (DeepSeek offers low, high and max). The default when never chosen is High.

## Models and AI providers
- Settings → Models: the default model every agent uses, "Use for all agents", the thinking effort, and an optional fallback model that takes over if the main provider fails (down, out of credit, rate limited).
- Settings → AI providers: add any OpenAI-compatible service (OpenAI, Anthropic, Gemini, OpenRouter, local models) with a base URL and key; Test checks it. Keys are stored only on the server. Groq is used for speech to text.
- The chat header's model button switches the default model (or the agent's own model if it has one). Agents can also list and switch models when asked.

## Approvals and auto mode
- Ask me: every command waits for the user. Careful auto: an AI reviewer approves normal, safe steps and asks about anything risky. Full auto: only catastrophic commands ask.
- Always ask, in every mode: shutting down or restarting, wiping or reconfiguring disks, keys and passwords and I.R.I.S.'s own data folder secrets, user accounts, turning security off, stopping I.R.I.S. itself, and paying or ordering in the browser.
- After an agent has read web pages, lasting changes (settings, routines, skills) ask the user, so text planted on a page can't change things unnoticed.
- Set it per agent with the Auto button (lightning/shield) in the chat header or in Settings → Approvals, which also sets what new agents start with.

## Voice and calls
- The phone button in a chat starts a voice call with that agent, in English or Greek. Speech becomes text on the server through Groq's Whisper (free plan: about 2,000 spoken turns a day); replies are spoken by the device's own voices.
- Settings → Voice & calls: the language (English + Greek, Ελληνικά or English), the voices, and a test.

## Memory and personality
- USER.md: facts about the user, shared by every agent. MEMORY.md: each agent's own notes. Agents add to them themselves; the user can edit them (Settings → Personality & memory, and each agent's editor).
- Response style (Settings → Personality & memory): Balanced, Straight to the point, Deep analyst, Witty, Teacher or Professional, plus free-form extra instructions ("call me …, use °C …").

## Compaction
- Long chats are compacted: older messages become a short summary the agent keeps as memory, and the messages themselves stay visible in the app. A violet line in the chat marks where it happened ("Compacted · N earlier messages summarized · Show summary").
- When: Settings → Models → "Chat length before compacting", with its own point for Iris (default 256k tokens) and for the other agents (default 64k): 32k, 64k, 128k, 256k, 512k or 1M. DeepSeek can hold 1M, but every step an agent takes sends the whole chat again, so longer chats cost more per step and are slower; the point always leaves some room in the model's window. It happens before the next reply and also in the middle of a long run of steps.
- The small ring in the chat header fills towards that point. On demand: type /compact in the message box, "Compact now" in the ring's popup, or the agent menu → Compact the chat. While it runs the chat shows "Compacting the chat…", and a notice says how many messages were summarized.

## Routines
- Settings → Routines, or ask any agent ("check my site every morning at 8"). A task wakes the agent on a schedule; a command runs without the AI and only wakes it when something fails. Scheduled commands ask first.
- One-off routines (run once at a time) remove themselves after they ran successfully.

## Skills
Skills are how-tos the team saves after working something out, loaded the next time a similar task comes up. Shared by every agent, stored as skills/<name>/SKILL.md on the server; add or edit them in Settings → Skills.

## Connected services (MCP)
Settings → Connected services: connect MCP servers (GitHub, Gmail, databases, notes…) with a command and environment values. Iris can use them; other agents need the Connections ability. Tools that change things ask first unless trusted. Tokens stay on the server (secrets/mcp.json).

## Browser and saved logins
Agents can drive a real browser (open, click, type, read pages that need JavaScript or a login). Settings → Saved logins stores site logins encrypted on the server; agents only see the name and site, and the browser fills the login itself, only on that site. Paying or ordering always asks.

## Web search
Settings → Web search: Automatic (Tavily when a key is saved, otherwise DuckDuckGo), DuckDuckGo (free, can rate-limit), Tavily (built for AI agents, free plan 1,000 searches a month) or DeepSeek's own search (paid from the DeepSeek balance).

## Usage, cost and limits
- Settings → Usage & limits: a chart of how much all agents used each day, switchable between tokens, money and both, for the last 14, 30 or 90 days, with totals for today, the last 7 days and the range. Hover or tap a day for its breakdown (cached input, new input, output).
- Money is worked out from DeepSeek's price list per 1M tokens: Flash $0.006 cached input / $0.30 new input / $1.20 output at peak hours (01:00–04:00 and 06:00–10:00 UTC on weekdays), half that the rest of the time. Most input comes from DeepSeek's cache, which is why it's cheap. Days marked ≈ include older replies recorded before the cache split, so they're estimates; the DeepSeek balance (the credit badge) is the final word.
- Daily and weekly token caps for all agents together are on the same page; when one is reached, agents stop until the reset (days at midnight, weeks on Monday) or until it's raised.
- Settings → Activity log: every tool every agent used, newest first; click one to open its chat. Failed replies and routines also show on the home screen ("Problems").

## Devices, remote access and the phone app
- Settings → Remote access: use I.R.I.S. from a phone or any browser, from anywhere. The server connects out to the user's relay, so nothing is opened on the network, and everything is end-to-end encrypted. Pair a phone with "Show a pairing code" (QR code or 8-character code, works once for 10 minutes), or set a username and password to sign in from a browser.
- Settings → Paired devices: every app or tool has its own access token; Revoke signs one out (it asks once more first).
- The Android app has the same chats, settings, calls and notifications (Settings → Notifications on the phone).

## Updates
- Desktop: Settings → About & updates shows the version and offers "Update now" when a new release is out; the installer is downloaded from GitHub, checked against its checksum, and I.R.I.S. restarts by itself with chats, agents and settings kept. A banner at the top also announces new versions.
- Phone: the app checks GitHub releases and offers the new APK ("Tap to update this app").
- Release notes for every version are on the GitHub Releases page; about_iris with topic "releases" lists the latest ones.

## Appearance
Settings → Appearance (desktop and web): Dark or Light theme, and animations Always, Follow Windows (reduce motion when Windows asks) or Off.

## Safety and privacy
Keys and tokens live only on the server (secrets folder) and are never shown to agents. Content from web pages and connected services is treated as untrusted: it can't approve anything, and lasting changes after reading it ask the user. Risky commands go through approvals (see above). Agents can't open the server's own or local-network addresses in the browser.

## Troubleshooting
- "Your server is offline": the server is off, asleep or Remote access is off; it reconnects by itself.
- Replies fail with a provider error: check the key and credit in Settings → AI providers, or set a fallback model.
- An agent seems stuck waiting: it may be waiting for an approval (yellow "Waiting for your approval"); approve or deny it in the chat or on the home screen.
- Answers feel shallow or slow: adjust the thinking effort slider.

## Shortcuts
Enter sends, Shift+Enter adds a line, Ctrl+K searches chats, "/" shows the slash commands (/compact), Escape closes menus. Right-click (desktop) or long-press (phone) an agent for its menu.
