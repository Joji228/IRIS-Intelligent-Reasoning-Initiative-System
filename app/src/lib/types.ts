// Mirrors the engine's JSON (crates/core/src/model.rs, crates/engine/src/*).

export interface Agent {
  id: string;
  name: string;
  description: string;
  avatar: string;
  color: string;
  system_prompt: string;
  provider: string;
  model: string;
  /** How hard it thinks; "" follows the default in Settings. */
  reasoning_effort: string;
  /** Enabled toolset ids, e.g. "web_search", "files". */
  tools: string[];
  /** "main" for the boss agent (Chief of Staff). */
  role: string;
  /** Id of the agent that created this one ("" if the user did). */
  created_by: string;
  /** Auto mode: a safety check approves safe commands instead of asking. */
  auto_approve: boolean;
  /** With auto_approve: no review, only the dangerous-command rules ask. */
  auto_full: boolean;
  created_at: number;
  updated_at: number;
}

export const MAX_AGENTS = 10;

/** Token caps (0 = off) and what has been used. */
export interface Limits {
  daily_tokens: number;
  weekly_tokens: number;
  used_today: number;
  used_this_week: number;
}

/** Short-lived expressions of an orb. */
export type Mood = 'happy' | 'sad' | 'spawn' | 'spin';

export type NewAgent = Pick<
  Agent,
  'name' | 'description' | 'avatar' | 'color' | 'system_prompt' | 'provider' | 'model' | 'tools'
>;

export interface Toolset {
  id: string;
  name: string;
  description: string;
  needs_approval: boolean;
}

export type ToolStatus = 'awaiting_approval' | 'running' | 'done' | 'error' | 'denied' | 'cancelled';

export interface ToolStep {
  id: string;
  name: string;
  arguments: string;
  status: ToolStatus;
  summary: string;
  output: string;
  approval_id?: string;
  /** Why auto mode ran it without asking. */
  auto_approved?: string;
  started_at: number;
  finished_at: number | null;
}

export interface Round {
  reasoning: string;
  content: string;
  tools: ToolStep[];
}

export interface PendingApproval {
  id: string;
  agent_id: string;
  thread_id: string;
  message_id: string;
  tool: string;
  arguments: string;
  summary: string;
  created_at: number;
  /** More to review before deciding: a code change's diff. */
  detail?: string;
}

export interface ToolLogEntry {
  id: string;
  agent_id: string;
  thread_id: string;
  message_id: string;
  tool: string;
  arguments: string;
  status: ToolStatus;
  summary: string;
  started_at: number;
  duration_ms: number;
}

/** The model every agent uses (chosen in Settings). */
/** The model used when the main provider fails. */
export interface Fallback {
  enabled: boolean;
  provider: string;
  model: string;
}

/** Reasoning levels, least to most thinking. "" is "auto": the model decides. */
export const REASONING_LEVELS = [
  { id: 'off', label: 'Off' },
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'max', label: 'Max' },
] as const;

export const reasoningLabel = (id: string) =>
  id ? (REASONING_LEVELS.find((l) => l.id === id)?.label ?? id) : 'Auto';

/** How hard agents think unless they have their own level. */
export interface ReasoningDefault {
  effort: string;
}

/** How agents talk to the user. */
export interface ResponseStyle {
  preset: 'balanced' | 'concise' | 'analyst' | 'witty' | 'teacher' | 'professional';
  /** Extra instructions in the user's own words. */
  custom: string;
}

export interface DefaultModel {
  provider: string;
  provider_name: string;
  model: string;
  /** Context window in tokens, when known (DeepSeek V4: 1M). */
  context_window: number | null;
}

/** Remaining API credit (DeepSeek). Amounts are strings as the provider sends them. */
export interface Balance {
  currency: string;
  total: string;
  granted: string;
  topped_up: string;
  available: boolean;
}

export interface SearchStatus {
  provider: 'auto' | 'tavily' | 'duckduckgo' | 'deepseek';
  has_tavily_key: boolean;
  /** A DeepSeek provider with a key exists (Settings → Providers). */
  has_deepseek_key: boolean;
  active: 'deepseek' | 'tavily' | 'duckduckgo';
}

/** A memory file's text (MEMORY.md / USER.md) and its size cap. */
export interface MemoryText {
  text: string;
  limit: number;
}

/** A failed reply or routine in the problems inbox. */
export interface Problem {
  id: string;
  kind: 'reply' | 'routine';
  agent_id: string;
  thread_id: string;
  message_id: string;
  routine_id: string;
  title: string;
  detail: string;
  created_at: number;
  resolved_at: number | null;
}

/** An agent's latest chat line, for the chat list. */
export interface ChatPreview {
  agent_id: string;
  role: 'user' | 'assistant';
  text: string;
  created_at: number;
}

export interface Thread {
  id: string;
  agent_id: string;
  title: string;
  created_at: number;
  updated_at: number;
  /** Older messages compacted into this summary (those created at or before summary_until). */
  summary: string;
  summary_until: number;
}

export type MessageStatus = 'streaming' | 'complete' | 'error' | 'cancelled';

export interface Message {
  id: string;
  thread_id: string;
  role: 'system' | 'user' | 'assistant';
  content: string;
  reasoning: string;
  /** How a reply came together: thinking, text and tool calls per model call. */
  rounds: Round[];
  status: MessageStatus;
  error: string | null;
  model: string | null;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  /** How full the model's context was at the end of this reply (tokens). */
  context_tokens: number | null;
  created_at: number;
  /** Files the user attached. */
  attachments?: Attachment[];
}

/** A saved login for agents (the password never comes back from the engine). */
export interface VaultAccount {
  id: string;
  name: string;
  site: string;
  username: string;
  has_password: boolean;
  notes: string;
}

/** A message found by chat search; the matching words are marked «like this». */
export interface SearchHit {
  message_id: string;
  thread_id: string;
  agent_id: string;
  agent_name: string;
  role: 'user' | 'assistant' | 'system';
  created_at: number;
  snippet: string;
}

/** A how-to the team saved (skills/<name>/SKILL.md on the server). */
export interface Skill {
  name: string;
  description: string;
  body: string;
}

/** A connected service (MCP server). Secret values never come back, only their names. */
export interface McpServer {
  name: string;
  transport: 'stdio' | 'http';
  command: string;
  args: string[];
  url: string;
  env_keys: string[];
  header_keys: string[];
  enabled: boolean;
  auto_approve: string[];
  connected: boolean;
  error: string | null;
  tools: { name: string; description: string; read_only: boolean }[];
}

export interface McpUpdate {
  command: string;
  args: string[];
  url: string;
  /** Empty values keep what's stored. */
  env?: Record<string, string>;
  headers?: Record<string, string>;
  enabled: boolean;
  auto_approve: string[];
}

/** Whether voice input works, and which provider turns speech into text. */
export interface VoiceStatus {
  available: boolean;
  provider: string | null;
  model: string | null;
}

/** Something an agent does on a schedule. */
export interface Routine {
  id: string;
  agent_id: string;
  name: string;
  schedule: string;
  kind: 'prompt' | 'command' | 'heartbeat';
  body: string;
  notify: 'failure' | 'always';
  enabled: boolean;
  next_run: number | null;
  last_run: number | null;
  last_result: string;
  created_at: number;
}

/** A file attached to a message (stored by the engine). */
export interface Attachment {
  id: string;
  name: string;
  mime: string;
  size: number;
  kind: 'image' | 'text' | 'pdf' | 'file';
}

export interface Provider {
  id: string;
  name: string;
  base_url: string;
  default_model: string;
  has_key: boolean;
  key_source: 'inline' | 'env' | 'file' | 'none';
}

export interface ProviderUpdate {
  name: string;
  base_url: string;
  default_model: string;
  /** Omit to keep the current key, '' to remove it. */
  api_key?: string;
}

/** Remote access through the relay (Settings → Remote access). */
export interface LinkStatus {
  enabled: boolean;
  url: string;
  connected: boolean;
  /** The relay's server key is on the server. */
  has_relay_key: boolean;
  /** The engine's public key, which phones and browsers pin. */
  public_key: string;
  login_user: string | null;
}

export interface Pairing {
  code: string;
  expires_at: number;
  /** What the QR code holds. */
  url: string;
}

/** Abilities every agent has without a toggle (matches the engine's BUILT_IN). */
export const BUILT_IN_ABILITIES = 'web search and pages, a browser, date and time, routines, the team (talking to other agents and Iris), models and settings';

/** The only switchable abilities; Iris has everything. */
export const ABILITY_GROUPS = [
  { id: 'files', name: 'Files', description: 'Read and write files in its own folder on the server.', tools: ['files'], asks: false },
  {
    id: 'computer',
    name: 'Computer control',
    description: 'Run PowerShell commands, check updates, install or remove apps, restart.',
    tools: ['shell', 'system'],
    asks: true,
  },
  {
    id: 'code',
    name: 'Develop I.R.I.S.',
    description: "Read and change the app's own code. Every change shows you the diff and asks first.",
    tools: ['dev'],
    asks: true,
  },
  {
    id: 'connections',
    name: 'Connections',
    description: 'Tools from services connected in Settings → Connections (MCP servers).',
    tools: ['mcp'],
    asks: true,
  },
] as const;

/** Which ability groups an agent has switched on. */
export function abilitiesOf(tools: string[]): string[] {
  return ABILITY_GROUPS.filter((g) => g.tools.some((t) => tools.includes(t))).map((g) => g.name);
}

export interface Device {
  id: string;
  name: string;
  created_at: number;
  last_seen_at: number | null;
  /** The desktop app's own access on the engine's computer (can't be revoked). */
  local: boolean;
}

export interface RunInfo {
  agent_id: string;
  thread_id: string;
  message_id: string;
  /** "thinking", "waiting", or the tool being used (e.g. "web_search"). */
  activity?: string;
  /** The step in words ("Searching the web for “…”"). */
  detail?: string;
}

export interface ThreadDetail {
  thread: Thread;
  messages: Message[];
  running: boolean;
}

export interface Posted {
  user_message?: Message;
  /** Missing when the agent was busy and the message was queued. */
  assistant_message?: Message;
  queued?: QueuedMessage;
}

/** A message waiting for a busy agent; it goes out when the current reply ends. */
export interface QueuedMessage {
  id: string;
  content: string;
  attachments: Attachment[];
  created_at: number;
}

export type IrisEvent =
  | { type: 'agent_updated'; agent: Agent }
  | { type: 'agent_deleted'; agent_id: string }
  | { type: 'thread_updated'; thread: Thread }
  | { type: 'thread_deleted'; thread_id: string; agent_id: string }
  | { type: 'message_created'; message: Message }
  | { type: 'message_deleted'; thread_id: string; message_id: string }
  | { type: 'message_delta'; thread_id: string; message_id: string; round: number; content?: string; reasoning?: string }
  | { type: 'message_progress'; thread_id: string; message_id: string; rounds: Round[] }
  | { type: 'message_completed'; message: Message }
  | ({ type: 'run_started' } & RunInfo)
  | ({ type: 'run_finished'; status: MessageStatus } & RunInfo)
  | { type: 'run_activity'; agent_id: string; thread_id: string; activity: string; detail?: string }
  | { type: 'approval_requested'; approval: PendingApproval }
  | { type: 'problems_changed' }
  | { type: 'approval_resolved'; approval_id: string; agent_id: string; approved: boolean }
  | { type: 'providers_changed' }
  | { type: 'connectivity'; online: boolean }
  | { type: 'routines_changed' }
  | { type: 'link_changed'; connected: boolean }
  | { type: 'queue_changed'; thread_id: string; queue: QueuedMessage[] }
  | { type: 'device_added'; name: string }
  | { type: 'lagged'; missed: number };

/** A file on a release (`IRIS-Setup-x64-v….exe` or `IRIS-Android-v….apk`). */
export interface UpdateAsset {
  name: string;
  url: string;
  size: number;
  /** GitHub's checksum of the file; without it the file is never installed. */
  sha256: string | null;
}

export interface UpdateRelease {
  version: string;
  notes: string;
  published_at: string;
  page: string;
  windows: UpdateAsset | null;
  android: UpdateAsset | null;
}

/** `GET /v1/update`: is a newer version out, and how is an update going. */
export interface UpdateStatus {
  current: string;
  latest: UpdateRelease | null;
  available: boolean;
  checked_at: number;
  error: string | null;
  auto_check: boolean;
  install: { phase: '' | 'downloading' | 'installing' | 'failed'; percent: number; error: string | null };
  last_update: { ok: boolean; version: string; log: string; finished_at: number } | null;
}
