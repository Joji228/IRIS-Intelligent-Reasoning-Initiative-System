// HTTP + WebSocket client for the I.R.I.S. engine.

import type {
  Agent,
  Attachment,
  Balance,
  Routine,
  VaultAccount,
  DefaultModel,
  Fallback,
  MemoryText,
  ResponseStyle,
  Device,
  Limits,
  UpdateStatus,
  IrisEvent,
  Message,
  NewAgent,
  PendingApproval,
  Posted,
  Provider,
  ProviderUpdate,
  ReasoningDefault,
  RunInfo,
  McpServer,
  McpUpdate,
  SearchHit,
  SearchStatus,
  Skill,
  VoiceStatus,
  Thread,
  ThreadDetail,
  ToolLogEntry,
  Toolset,
  LinkStatus,
  Pairing,
  QueuedMessage,
  ChatPreview,
  Problem,
} from './types';
import type { LinkConnection, LinkResponse } from './link';

export interface Connection {
  url: string;
  token: string;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export class IrisApi {
  readonly base: string;

  /** With `link`, every request goes through I.R.I.S. Link (web and phone app). */
  constructor(
    public readonly conn: Connection,
    readonly link: LinkConnection | null = null,
  ) {
    this.base = conn.url.replace(/\/+$/, '');
  }

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (this.link) return this.linkReq<T>(await this.link.request(method, path, body));
    let res: Response;
    try {
      res = await fetch(this.base + path, {
        method,
        headers: {
          authorization: `Bearer ${this.conn.token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      throw new ApiError(0, 'Cannot reach the I.R.I.S. engine');
    }
    const text = await res.text();
    let data: any = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch {
      data = text;
    }
    if (!res.ok) {
      throw new ApiError(res.status, (data && data.error) || (typeof data === 'string' && data) || res.statusText);
    }
    return data as T;
  }

  private linkReq<T>(res: LinkResponse): T {
    if (res.status === 0) throw new ApiError(0, 'Cannot reach the I.R.I.S. engine');
    let data: any = null;
    try {
      data = res.body ? JSON.parse(res.body) : null;
    } catch {
      data = res.body;
    }
    if (res.status >= 400) {
      throw new ApiError(res.status, (data && data.error) || (typeof data === 'string' && data) || `error ${res.status}`);
    }
    return data as T;
  }

  async health(): Promise<{ status: string; version: string; uptime_secs: number }> {
    if (this.link) {
      await this.link.start();
      return { status: 'ok', version: '', uptime_secs: 0 };
    }
    return fetch(`${this.base}/health`, { signal: AbortSignal.timeout(4000) }).then((r) => {
      if (!r.ok) throw new ApiError(r.status, 'unhealthy');
      return r.json() as Promise<{ status: string; version: string; uptime_secs: number }>;
    });
  }

  status = () =>
    this.req<{ running: RunInfo[]; online?: boolean; queues?: Record<string, QueuedMessage[]> }>('GET', '/v1/status');
  removeQueued = (threadId: string, id: string) => this.req<null>('DELETE', `/v1/threads/${threadId}/queue/${id}`);
  /** Sends a queued message next, stopping the reply in progress. */
  steerQueued = (threadId: string, id: string) => this.req<null>('POST', `/v1/threads/${threadId}/queue/${id}/steer`);
  designAgent = (request: string) => this.req<Agent>('POST', '/v1/agents/design', { request });
  /** A blank orb; the first message it gets decides what it becomes. */
  summonAgent = () => this.req<Agent>('POST', '/v1/agents/blank');

  providers = () => this.req<Provider[]>('GET', '/v1/providers');
  putProvider = (id: string, update: ProviderUpdate) => this.req<Provider>('PUT', `/v1/providers/${id}`, update);
  deleteProvider = (id: string) => this.req<null>('DELETE', `/v1/providers/${id}`);
  models = (id: string) => this.req<string[]>('GET', `/v1/providers/${id}/models`);
  balance = (id: string) => this.req<Balance>('GET', `/v1/providers/${id}/balance`);
  limits = () => this.req<Limits>('GET', '/v1/settings/limits');
  update = (refresh = false) => this.req<UpdateStatus>('GET', `/v1/update${refresh ? '?refresh=true' : ''}`);
  installUpdate = (force = false) => this.req<null>('POST', '/v1/update/install', { force });
  setUpdateAutoCheck = (autoCheck: boolean) => this.req<UpdateStatus>('PUT', '/v1/update/settings', { auto_check: autoCheck });
  putLimits = (daily_tokens: number, weekly_tokens: number) =>
    this.req<Limits>('PUT', '/v1/settings/limits', { daily_tokens, weekly_tokens });
  fallback = () => this.req<Fallback>('GET', '/v1/settings/fallback');
  putFallback = (f: Fallback) => this.req<Fallback>('PUT', '/v1/settings/fallback', f);
  style = () => this.req<ResponseStyle>('GET', '/v1/settings/style');
  putStyle = (s: ResponseStyle) => this.req<ResponseStyle>('PUT', '/v1/settings/style', s);
  defaultModel = () => this.req<DefaultModel | null>('GET', '/v1/settings/model');
  putDefaultModel = (provider: string, model: string) =>
    this.req<DefaultModel | null>('PUT', '/v1/settings/model', { provider, model });
  reasoning = () => this.req<ReasoningDefault>('GET', '/v1/settings/reasoning');
  putReasoning = (effort: string) => this.req<ReasoningDefault>('PUT', '/v1/settings/reasoning', { effort });
  /** The auto mode new agents start with. */
  autoDefault = () => this.req<{ mode: 'off' | 'careful' | 'full' }>('GET', '/v1/settings/auto');
  putAutoDefault = (mode: 'off' | 'careful' | 'full') => this.req<{ mode: 'off' | 'careful' | 'full' }>('PUT', '/v1/settings/auto', { mode });

  agents = () => this.req<Agent[]>('GET', '/v1/agents');
  createAgent = (a: NewAgent) => this.req<Agent>('POST', '/v1/agents', a);
  patchAgent = (id: string, patch: Partial<NewAgent> & Partial<Pick<Agent, 'reasoning_effort' | 'auto_approve' | 'auto_full'>>) =>
    this.req<Agent>('PATCH', `/v1/agents/${id}`, patch);
  deleteAgent = (id: string) => this.req<null>('DELETE', `/v1/agents/${id}`);

  threads = (agentId: string) => this.req<Thread[]>('GET', `/v1/agents/${agentId}/threads`);
  /** An agent's one chat (created on first use). */
  chat = (agentId: string) => this.req<ThreadDetail>('GET', `/v1/agents/${agentId}/chat`);
  clearChat = (agentId: string) => this.req<ThreadDetail>('POST', `/v1/agents/${agentId}/chat/clear`);
  compactChat = (agentId: string) => this.req<ThreadDetail>('POST', `/v1/agents/${agentId}/chat/compact`);
  agentMemory = (agentId: string) => this.req<MemoryText>('GET', `/v1/agents/${agentId}/memory`);
  putAgentMemory = (agentId: string, text: string) =>
    this.req<MemoryText>('PUT', `/v1/agents/${agentId}/memory`, { text });
  userMemory = () => this.req<MemoryText>('GET', '/v1/memory/user');
  putUserMemory = (text: string) => this.req<MemoryText>('PUT', '/v1/memory/user', { text });
  recentThreads = (limit = 20) => this.req<Thread[]>('GET', `/v1/threads?limit=${limit}`);
  previews = () => this.req<ChatPreview[]>('GET', '/v1/previews');

  // Problems inbox
  problems = () => this.req<Problem[]>('GET', '/v1/problems');
  retryProblem = (id: string) => this.req<void>('POST', `/v1/problems/${id}/retry`);
  dismissProblem = (id: string) => this.req<void>('POST', `/v1/problems/${id}/dismiss`);
  dismissAllProblems = () => this.req<void>('POST', '/v1/problems/dismiss');
  createThread = (agentId: string, title = '') => this.req<Thread>('POST', `/v1/agents/${agentId}/threads`, { title });
  thread = (id: string) => this.req<ThreadDetail>('GET', `/v1/threads/${id}`);
  renameThread = (id: string, title: string) => this.req<Thread>('PATCH', `/v1/threads/${id}`, { title });
  deleteThread = (id: string) => this.req<null>('DELETE', `/v1/threads/${id}`);

  postMessage = (threadId: string, content: string, attachments: Attachment[] = []) =>
    this.req<Posted>('POST', `/v1/threads/${threadId}/messages`, { content, attachments });
  vault = () => this.req<VaultAccount[]>('GET', '/v1/vault');
  addAccount = (a: { name: string; site: string; username: string; password: string; notes?: string }) =>
    this.req<VaultAccount>('POST', '/v1/vault', a);
  editAccount = (id: string, a: Partial<{ name: string; site: string; username: string; password: string; notes: string }>) =>
    this.req<VaultAccount>('PATCH', `/v1/vault/${id}`, a);
  deleteAccount = (id: string) => this.req<null>('DELETE', `/v1/vault/${id}`);
  routines = () => this.req<Routine[]>('GET', '/v1/routines');
  createRoutine = (r: Pick<Routine, 'agent_id' | 'name' | 'schedule' | 'kind' | 'body' | 'notify'>) =>
    this.req<Routine>('POST', '/v1/routines', r);
  setRoutineEnabled = (id: string, enabled: boolean) => this.req<Routine>('PATCH', `/v1/routines/${id}`, { enabled });
  editRoutine = (id: string, r: Partial<Pick<Routine, 'agent_id' | 'name' | 'schedule' | 'kind' | 'body' | 'notify'>>) =>
    this.req<Routine>('PATCH', `/v1/routines/${id}`, r);
  deleteRoutine = (id: string) => this.req<null>('DELETE', `/v1/routines/${id}`);
  runRoutine = (id: string) => this.req<null>('POST', `/v1/routines/${id}/run`);
  /** Uploads a file (base64) for a message about to be sent. */
  uploadAttachment = (threadId: string, name: string, data: string) =>
    this.req<Attachment>('POST', `/v1/threads/${threadId}/attachments`, { name, data });
  /** Where the app can show an attached file (token in the URL, for <img>). */
  attachmentUrl = (threadId: string, id: string) =>
    `${this.base}/v1/attachments/${threadId}/${id}?token=${encodeURIComponent(this.conn.token)}`;
  /** A URL an <img> can show: direct, or (over the link) a local copy. */
  async attachmentSrc(threadId: string, id: string): Promise<string> {
    if (!this.link) return this.attachmentUrl(threadId, id);
    const res = await this.link.request('GET', `/v1/attachments/${threadId}/${id}`);
    if (res.status !== 200) throw new ApiError(res.status, res.body ?? 'not found');
    const bytes = res.b64 ? Uint8Array.from(atob(res.b64), (c) => c.charCodeAt(0)) : new TextEncoder().encode(res.body ?? '');
    return URL.createObjectURL(new Blob([bytes], { type: res.ct || 'application/octet-stream' }));
  }
  regenerate = (threadId: string) => this.req<Posted>('POST', `/v1/threads/${threadId}/regenerate`);
  stop = (threadId: string) => this.req<{ stopped: boolean }>('POST', `/v1/threads/${threadId}/stop`);

  tools = () => this.req<Toolset[]>('GET', '/v1/tools');
  approvals = () => this.req<PendingApproval[]>('GET', '/v1/approvals');
  decide = (id: string, approve: boolean) => this.req<null>('POST', `/v1/approvals/${id}`, { approve });
  audit = (limit = 100) => this.req<ToolLogEntry[]>('GET', `/v1/audit?limit=${limit}`);
  /** Full-text search over every conversation. */
  searchChats = (q: string, agentId = '', limit = 30) =>
    this.req<SearchHit[]>('GET', `/v1/search?q=${encodeURIComponent(q)}&agent_id=${encodeURIComponent(agentId)}&limit=${limit}`);
  skills = () => this.req<Skill[]>('GET', '/v1/skills');
  putSkill = (name: string, description: string, body: string) =>
    this.req<Skill>('PUT', `/v1/skills/${encodeURIComponent(name)}`, { description, body });
  deleteSkill = (name: string) => this.req<null>('DELETE', `/v1/skills/${encodeURIComponent(name)}`);
  mcp = () => this.req<McpServer[]>('GET', '/v1/mcp');
  putMcp = (name: string, u: McpUpdate) => this.req<McpServer[]>('PUT', `/v1/mcp/${encodeURIComponent(name)}`, u);
  deleteMcp = (name: string) => this.req<null>('DELETE', `/v1/mcp/${encodeURIComponent(name)}`);
  refreshMcp = () => this.req<McpServer[]>('POST', '/v1/mcp/refresh');
  voice = () => this.req<VoiceStatus>('GET', '/v1/voice');
  /** Speech to text: a recording (base64) in, its text out. */
  transcribe = (data: string, mime: string, language = '') =>
    this.req<{ text: string }>('POST', '/v1/voice/transcribe', { data, mime, language });
  search = () => this.req<SearchStatus>('GET', '/v1/settings/search');
  putSearch = (provider: SearchStatus['provider'], tavily_api_key?: string) =>
    this.req<SearchStatus>('PUT', '/v1/settings/search', { provider, tavily_api_key });

  devices = () => this.req<Device[]>('GET', '/v1/devices');
  createDevice = (name: string) => this.req<{ device: Device; token: string }>('POST', '/v1/devices', { name });
  deleteDevice = (id: string) => this.req<null>('DELETE', `/v1/devices/${id}`);
  linkStatus = () => this.req<LinkStatus>('GET', '/v1/link');
  putLink = (enabled: boolean, url: string) => this.req<LinkStatus>('PUT', '/v1/link', { enabled, url });
  newPairing = () => this.req<Pairing>('POST', '/v1/link/pair');
  setLinkLogin = (user: string, password: string) => this.req<LinkStatus>('PUT', '/v1/link/login', { user, password });
  clearLinkLogin = () => this.req<LinkStatus>('DELETE', '/v1/link/login');

  /**
   * Live event stream with automatic reconnect. `onOpen` fires on every
   * (re)connect, which is when the caller should refetch its state.
   */
  connectEvents(handlers: { onEvent: (e: IrisEvent) => void; onOpen: () => void; onClose: () => void }) {
    if (this.link) {
      const link = this.link;
      link.onEvent = handlers.onEvent;
      link.onOpen = handlers.onOpen;
      const prev = link.onState;
      link.onState = (s) => {
        prev(s);
        if (s !== 'online') handlers.onClose();
      };
      if (link.state === 'online') handlers.onOpen();
      void link.start();
      return () => link.stop();
    }
    const wsBase = this.base.replace(/^http/, 'ws');
    let ws: WebSocket | null = null;
    let closed = false;
    let delay = 500;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const open = () => {
      ws = new WebSocket(`${wsBase}/v1/events?token=${encodeURIComponent(this.conn.token)}`);
      ws.onmessage = (e) => {
        const ev = JSON.parse(e.data);
        if (ev.type === 'hello') {
          delay = 500;
          handlers.onOpen();
        } else {
          handlers.onEvent(ev as IrisEvent);
        }
      };
      ws.onclose = () => {
        if (closed) return;
        handlers.onClose();
        timer = setTimeout(open, delay);
        delay = Math.min(delay * 2, 8000);
      };
    };
    open();
    return () => {
      closed = true;
      clearTimeout(timer);
      ws?.close();
    };
  }
}

export type { Message };
