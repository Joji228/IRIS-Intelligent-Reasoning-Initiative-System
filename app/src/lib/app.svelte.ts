// Global app state (Svelte 5 runes). One instance, `app`, used everywhere.
//
// The engine is the source of truth: we load snapshots over HTTP and then
// keep them current from the live event stream. Every (re)connect refetches,
// so a dropped connection never leaves stale state behind.

import { orbState, phrases } from './orbs';
import {
  Archive,
  Eraser,
  House,
  MessageSquare,
  Moon,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Sun,
  Trash2,
} from '@lucide/svelte';
import { ApiError, IrisApi, type Connection } from './api';
import { voice } from './voice.svelte';
import { inDesktopApp, inWebLink, resolveConnection, saveConnection } from './connection';
import { forgetCredentials, LinkConnection, loadCredentials, type LinkCredentials, type LinkState } from './link';
import {
  reasoningLabel,
  type Agent,
  type Balance,
  type DefaultModel,
  type IrisEvent,
  type Limits,
  type QueuedMessage,
  type Message,
  type Mood,
  type ChatPreview,
  type Problem,
  type NewAgent,
  type PendingApproval,
  type Provider,
  type Round,
  type RunInfo,
  type Thread,
  type Toolset,
} from './types';

export type View =
  | { kind: 'home' }
  | { kind: 'agent'; agentId: string; threadId: string | null };

export type Modal =
  | { kind: 'agent'; agentId: string | null }
  | { kind: 'newAgent' }
  | { kind: 'settings'; tab?: SettingsTab }
  | { kind: 'search' }
  | { kind: 'problems' };

/** One entry in the right-click menu. `confirm` asks again before acting. */
export interface MenuItem {
  label: string;
  action: () => unknown;
  icon?: typeof Pencil;
  hint?: string;
  /** What the item does, shown when hovering a small info icon. */
  info?: string;
  danger?: boolean;
  disabled?: boolean;
  confirm?: string;
  separator?: boolean;
}

export type AutoMode = 'off' | 'careful' | 'full';

export type SettingsTab = 'models' | 'providers' | 'routines' | 'skills' | 'connections' | 'accounts' | 'responses' | 'limits' | 'search' | 'activity' | 'devices' | 'remote' | 'appearance' | 'about';

export type ConnState = 'starting' | 'online' | 'reconnecting' | 'offline' | 'setup' | 'signin';

interface Toast {
  id: number;
  text: string;
  kind: 'info' | 'error';
}

const byUpdatedDesc = (a: Thread, b: Thread) => b.updated_at - a.updated_at;

/** A per-device preference from localStorage (unavailable storage → fallback). */
function stored<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

function store(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* not remembered on this device */
  }
}

/** The left bar's width range (px); orbs scale with it. */
export const RAIL_MIN = 60;
export const RAIL_MAX = 340;
/** From this width the bar becomes a chat list (names, last message, time). */
export const RAIL_LIST = 180;
/** The list's width when opened with the button. */
export const RAIL_LIST_DEFAULT = 280;
export const RAIL_DEFAULT = 68;

class IrisApp {
  api: IrisApi | null = null;
  private stopEvents: (() => void) | null = null;

  conn = $state<ConnState>('starting');
  /** Web app only: the link's state, and why the last sign-in ended. */
  linkState = $state<LinkState>('connecting');
  signinError = $state('');
  /** Web app only: the name this device paired as. */
  deviceName = $state('');
  engineVersion = $state('');
  agents = $state<Agent[]>([]);
  providers = $state<Provider[]>([]);
  /** Threads per agent id, most recent first. */
  threads = $state<Record<string, Thread[]>>({});
  recent = $state<Thread[]>([]);
  /** Replies being generated, by thread id. */
  running = $state<Record<string, RunInfo>>({});
  /** Tool calls waiting for the user, by approval id. */
  approvals = $state<Record<string, PendingApproval>>({});
  toolsets = $state<Toolset[]>([]);
  /** The one model every agent uses. */
  defaultModel = $state<DefaultModel | null>(null);
  /** How hard agents think unless they have their own level ("" = auto). */
  reasoningDefault = $state('');
  /** Model ids per provider id, for the chat box's model picker. */
  modelOptions = $state<Record<string, string[]>>({});
  /** API credit left at the default provider (DeepSeek), if it reports one. */
  balance = $state<Balance | null>(null);
  balanceError = $state('');
  balanceUpdated = $state(0);
  balanceLoading = $state(false);
  private balanceTimer: ReturnType<typeof setTimeout> | undefined;
  /** Last time the user did something here (or an agent was working). */
  private lastActive = Date.now();
  /** The credit stops refreshing after 10 idle minutes, until the user is back. */
  balancePaused = $state(false);
  /** Daily / weekly token caps and today's / this week's usage. */
  limits = $state<Limits | null>(null);
  /** Short-lived orb expressions (happy after a task, sad after a failure...). */
  moods = $state<Record<string, Mood>>({});
  private moodTimers: Record<string, ReturnType<typeof setTimeout>> = {};
  /** Open problems: failed replies and routines (the inbox). */
  problems = $state<Problem[]>([]);
  /** Each agent's latest chat line (the chat list shows it). */
  previews = $state<Record<string, ChatPreview>>({});
  private previewTimer: ReturnType<typeof setTimeout> | undefined;
  /** Agents with a reply the user hasn't opened yet (their orbs nudge). */
  unread = $state<Record<string, boolean>>({});

  view = $state<View>({ kind: 'home' });
  /** Bumped when routines change, so an open Routines list reloads. */
  routinesVersion = $state(0);
  /** The order of the orbs in the left bar (agent ids; Iris always first). */
  orbOrder = $state<string[]>(stored('iris.orbOrder', []));
  /** The left bar's width; the orbs grow and shrink with it. */
  railWidth = $state<number>(stored('iris.railWidth', RAIL_DEFAULT));
  /** Messages waiting for busy agents, by thread. */
  queues = $state<Record<string, QueuedMessage[]>>({});
  /** Whether the engine is connected to the I.R.I.S. Link relay. */
  linkConnected = $state(false);
  /** Bumped when a device pairs, so open lists reload. */
  devicesVersion = $state(0);
  /** The last device that paired or signed in remotely. */
  lastDeviceAdded = $state<string | null>(null);
  /** Whether the engine can reach the AI provider (false: red bar). */
  online = $state(true);
  /** The open right-click menu, if any. */
  menu = $state<{ x: number; y: number; title?: string; items: MenuItem[] } | null>(null);
  modal = $state<Modal | null>(null);
  thread = $state<Thread | null>(null);
  messages = $state<Message[]>([]);
  loadingThread = $state(false);
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light'>('dark');
  /**
   * Orb animations. "always" ignores Windows' "reduce motion" signal, which
   * remote-desktop sessions often switch on by themselves.
   */
  motion = $state<'always' | 'system' | 'off'>('always');

  // ---- derived helpers ---------------------------------------------------

  get currentAgent(): Agent | undefined {
    const v = this.view;
    return v.kind === 'agent' ? this.agents.find((a) => a.id === v.agentId) : undefined;
  }

  agent(id: string) {
    return this.agents.find((a) => a.id === id);
  }

  isWorking(agentId: string) {
    return Object.values(this.running).some((r) => r.agent_id === agentId);
  }

  isRunning(threadId: string | null | undefined) {
    return !!threadId && threadId in this.running;
  }

  /** The boss agent (Chief of Staff) the user mainly talks to. */
  get mainAgent(): Agent | undefined {
    return this.agents.find((a) => a.role === 'main');
  }

  /** Agents in rail order: the main one first, then by creation. */
  /** Iris first, then the user's own order (new agents at the end, oldest first). */
  get orderedAgents(): Agent[] {
    const rank = (a: Agent) => {
      const i = this.orbOrder.indexOf(a.id);
      return i < 0 ? Number.MAX_SAFE_INTEGER : i;
    };
    return [...this.agents].sort((a, b) =>
      a.role === 'main' ? -1 : b.role === 'main' ? 1 : rank(a) - rank(b) || a.created_at - b.created_at,
    );
  }

  setOrbOrder(ids: string[]) {
    this.orbOrder = ids;
    store('iris.orbOrder', ids);
  }

  setRailWidth(px: number, save = true) {
    this.railWidth = Math.round(Math.min(RAIL_MAX, Math.max(RAIL_MIN, px)));
    if (save) store('iris.railWidth', this.railWidth);
  }

  /** What an agent is busy with, if anything (for its activity bubble). */
  activityOf(agentId: string): string | undefined {
    if (this.isWaiting(agentId)) return 'waiting';
    const run = Object.values(this.running).find((r) => r.agent_id === agentId);
    return run ? run.activity || 'thinking' : undefined;
  }

  /** The step an agent is on, in words ("Searching the web for “…”"). */
  detailOf(agentId: string): string {
    return Object.values(this.running).find((r) => r.agent_id === agentId)?.detail ?? '';
  }

  /** What a busy agent is doing, in plain words ("Searching the web…"). */
  statusLine(agentId: string): string {
    const activity = this.activityOf(agentId);
    return phrases(orbState({ activity, working: !!activity }), activity)[0] ?? 'Working…';
  }

  async loadProblems() {
    try {
      this.problems = (await this.api?.problems()) ?? [];
    } catch {
      // an older engine without the inbox
    }
  }

  async retryProblem(p: Problem) {
    try {
      await this.api?.retryProblem(p.id);
      this.problems = this.problems.filter((x) => x.id !== p.id);
      if (p.kind === 'reply') await this.openAgent(p.agent_id);
      this.toast(p.kind === 'reply' ? 'Trying again…' : 'Running the routine again…');
    } catch (e) {
      this.error(e);
    }
  }

  async dismissProblem(id?: string) {
    try {
      if (id) await this.api?.dismissProblem(id);
      else await this.api?.dismissAllProblems();
      this.problems = id ? this.problems.filter((x) => x.id !== id) : [];
    } catch (e) {
      this.error(e);
    }
  }

  /** Reloads the chat list's last lines (soon, after a burst of replies). */
  refreshPreviews(delayMs = 0) {
    clearTimeout(this.previewTimer);
    this.previewTimer = setTimeout(async () => {
      try {
        const list = (await this.api?.previews()) ?? [];
        this.previews = Object.fromEntries(list.map((p) => [p.agent_id, p]));
      } catch {
        // an older engine without previews: the list shows descriptions
      }
    }, delayMs);
  }

  /** The user has been away a while: idle orbs doze off until they're back. */
  get sleepy(): boolean {
    return this.balancePaused;
  }

  setMood(agentId: string, mood: Mood, ms: number) {
    clearTimeout(this.moodTimers[agentId]);
    this.moods[agentId] = mood;
    this.moodTimers[agentId] = setTimeout(() => delete this.moods[agentId], ms);
  }

  /** "DeepSeek · deepseek-flash" for the model an agent actually uses. */
  modelLabel(agent?: Agent): string {
    if (agent?.provider && agent.model) {
      const p = this.providers.find((p) => p.id === agent.provider);
      return `${p?.name ?? agent.provider} · ${agent.model}`;
    }
    const d = this.defaultModel;
    return d ? `${d.provider_name || d.provider} · ${d.model}` : 'No model';
  }

  /** The user did something: resume the credit refresh if it was paused. */
  markActive() {
    this.lastActive = Date.now();
    if (this.balancePaused) {
      this.balancePaused = false;
      void this.refreshBalance(true);
    }
  }

  /**
   * Refreshes the credit balance, then again every 30 s while someone is
   * around: after 10 minutes with no input and no agent working it pauses
   * until the user is back. Replies finishing also refresh it, but never more
   * than once per 10 s unless `force`d (the refresh button), so a busy team
   * doesn't hammer the provider.
   */
  async refreshBalance(force = false) {
    clearTimeout(this.balanceTimer);
    if (Object.keys(this.running).length) this.lastActive = Date.now();
    if (!force && Date.now() - this.lastActive > 10 * 60_000) {
      this.balancePaused = true;
      return;
    }
    this.balanceTimer = setTimeout(() => void this.refreshBalance(), 30_000);
    const provider = this.defaultModel?.provider;
    if (!this.api || !provider || this.balanceLoading) return;
    if (!force && Date.now() - this.balanceUpdated < 10_000) return;
    this.balanceLoading = true;
    try {
      this.balance = await this.api.balance(provider);
      this.balanceError = '';
      this.balanceUpdated = Date.now();
    } catch (e) {
      this.balance = null;
      this.balanceError = e instanceof ApiError && e.status === 412 ? 'Add an API key' : e instanceof Error ? e.message : String(e);
    } finally {
      this.balanceLoading = false;
    }
  }

  async refreshLimits() {
    if (!this.api) return;
    try {
      this.limits = await this.api.limits();
    } catch {
      /* shown as unknown */
    }
  }

  async setDefaultModel(provider: string, model: string) {
    if (!this.api) return false;
    try {
      this.defaultModel = await this.api.putDefaultModel(provider, model);
      this.toast(`All agents now use ${model}`);
      void this.refreshBalance();
      return true;
    } catch (e) {
      this.error(e);
      return false;
    }
  }

  async setReasoningDefault(effort: string) {
    if (!this.api) return;
    try {
      this.reasoningDefault = (await this.api.putReasoning(effort)).effort;
      this.toast(`Agents think: ${reasoningLabel(effort)}`);
    } catch (e) {
      this.error(e);
    }
  }

  /** Gives one agent its own model (`provider` "" follows the default again). */
  async setAgentModel(agentId: string, provider: string, model: string) {
    if (!this.api) return;
    try {
      this.upsertAgent(await this.api.patchAgent(agentId, { provider, model }));
    } catch (e) {
      this.error(e);
    }
  }

  /** "+": a new orb right away; its chat opens and the first message shapes it. */
  async summonAgent() {
    if (!this.api) return;
    try {
      const agent = await this.api.summonAgent();
      this.upsertAgent(agent);
      this.setMood(agent.id, 'spawn', 1200);
      await this.openAgent(agent.id);
    } catch (e) {
      this.error(e);
    }
  }

  /** Designs and creates an agent from one sentence. */
  async designAgent(request: string): Promise<Agent | null> {
    if (!this.api) return null;
    try {
      const agent = await this.api.designAgent(request);
      this.upsertAgent(agent);
      return agent;
    } catch (e) {
      this.error(e);
      return null;
    }
  }

  /**
   * Auto mode for an agent (and the agents it creates): 'off' asks for every
   * command, 'careful' lets a quick AI check approve safe ones, 'full' runs
   * everything except a short list of dangerous commands.
   */
  async setAgentAuto(agentId: string, mode: AutoMode) {
    if (!this.api) return;
    try {
      this.upsertAgent(await this.api.patchAgent(agentId, { auto_approve: mode !== 'off', auto_full: mode === 'full' }));
      this.toast(
        mode === 'full'
          ? 'Full auto: only dangerous commands ask you'
          : mode === 'careful'
            ? 'Careful auto: an AI check approves safe commands'
            : 'Auto off: every command asks you first',
      );
    } catch (e) {
      this.error(e);
    }
  }

  /** An agent's own auto mode. */
  autoModeOf(agent: Agent): AutoMode {
    return !agent.auto_approve ? 'off' : agent.auto_full ? 'full' : 'careful';
  }

  /** Whether auto mode applies to an agent: its own switch or its creator's. */
  autoFor(agent: Agent): 'own' | 'inherited' | null {
    if (agent.auto_approve) return 'own';
    const creator = agent.created_by ? this.agent(agent.created_by) : undefined;
    return creator?.auto_approve ? 'inherited' : null;
  }

  /** One agent's reasoning level ("" follows the default again). */
  async setAgentReasoning(agentId: string, effort: string) {
    if (!this.api) return;
    try {
      this.upsertAgent(await this.api.patchAgent(agentId, { reasoning_effort: effort }));
    } catch (e) {
      this.error(e);
    }
  }

  /** Loads the model lists of all providers that can answer (once per change). */
  async loadModelOptions() {
    const api = this.api;
    if (!api) return;
    const loaded = await Promise.all(
      this.providers.map(async (p) => [p.id, await api.models(p.id).catch(() => [] as string[])] as const),
    );
    this.modelOptions = Object.fromEntries(loaded);
  }

  /** The agent is paused until the user approves or denies something. */
  isWaiting(agentId: string) {
    return Object.values(this.approvals).some((a) => a.agent_id === agentId);
  }

  get pendingApprovals(): PendingApproval[] {
    return Object.values(this.approvals).sort((a, b) => a.created_at - b.created_at);
  }

  async decide(approvalId: string, approve: boolean) {
    if (!this.api) return;
    // Optimistic: the reply resumes right away; events confirm it.
    delete this.approvals[approvalId];
    try {
      await this.api.decide(approvalId, approve);
    } catch (e) {
      this.error(e);
      void this.refreshAll();
    }
  }

  // ---- lifecycle -----------------------------------------------------------

  async start() {
    this.loadTheme();
    // Browser notifications are gone (the phone app notifies): drop the
    // service worker an older version registered, and its subscription.
    if ('serviceWorker' in navigator) {
      void navigator.serviceWorker
        .getRegistrations()
        .then((list) => list.forEach((r) => void r.unregister()))
        .catch(() => {});
    }
    for (const type of ['pointerdown', 'keydown', 'wheel', 'touchstart'] as const) {
      window.addEventListener(type, () => this.markActive(), { passive: true, capture: true });
    }
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') this.markActive();
    });
    if (inWebLink) {
      const creds = loadCredentials();
      if (creds && !location.hash.includes('pair=')) await this.connectLink(creds);
      else this.conn = 'signin';
      // Phones pause pages in the background: reconnect when it's back.
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') this.api?.link?.nudge();
      });
      return;
    }
    const conn = await resolveConnection();
    if (!conn) {
      // In the desktop app the engine may still be starting: keep retrying
      // instead of asking for an address.
      this.conn = inDesktopApp ? 'offline' : 'setup';
      return;
    }
    await this.connect(conn);
  }

  /** Web app: connect through the relay with a saved pairing. */
  async connectLink(creds: LinkCredentials) {
    const link = new LinkConnection(creds);
    this.deviceName = creds.device;
    link.onState = (s) => (this.linkState = s);
    link.onSignedOut = (reason) => {
      forgetCredentials();
      this.stopEvents?.();
      this.signinError = reason;
      this.modal = null;
      this.conn = 'signin';
    };
    this.stopEvents?.();
    this.api = new IrisApi({ url: creds.relay, token: creds.token }, link);
    this.conn = 'starting';
    // The link reconnects by itself; every (re)connect refetches everything.
    this.stopEvents = this.api.connectEvents({
      onEvent: (e) => this.onEvent(e),
      onOpen: () => void this.refreshAll(),
      onClose: () => {
        if (this.conn === 'online') this.conn = 'reconnecting';
        else if (this.conn === 'starting') this.conn = 'offline';
      },
    });
    return true;
  }

  /** Web app: forget this device's pairing on this browser. */
  signOut() {
    this.stopEvents?.();
    this.stopEvents = null;
    this.api = null;
    forgetCredentials();
    this.signinError = '';
    this.modal = null;
    this.conn = 'signin';
  }

  async connect(conn: Connection, remember = false) {
    this.stopEvents?.();
    this.api = new IrisApi(conn);
    this.conn = 'starting';
    try {
      const health = await this.api.health();
      this.engineVersion = health.version;
    } catch {
      this.conn = 'offline';
      return false;
    }
    if (remember) saveConnection(conn);
    this.stopEvents = this.api.connectEvents({
      onEvent: (e) => this.onEvent(e),
      onOpen: () => void this.refreshAll(),
      onClose: () => {
        if (this.conn === 'online') this.conn = 'reconnecting';
      },
    });
    return true;
  }

  async retry() {
    if (this.api?.link) this.api.link.nudge();
    else if (this.api) await this.connect(this.api.conn);
    else await this.start();
  }

  /** Full refetch: on first connect and after every reconnect. */
  async refreshAll() {
    const api = this.api;
    if (!api) return;
    try {
      const [agents, providers, status, recent, approvals, toolsets, defaultModel, reasoning] = await Promise.all([
        api.agents(),
        api.providers(),
        api.status(),
        api.recentThreads(30),
        api.approvals(),
        api.tools(),
        api.defaultModel(),
        api.reasoning(),
      ]);
      this.agents = agents;
      this.providers = providers;
      this.defaultModel = defaultModel;
      this.reasoningDefault = reasoning.effort;
      void this.refreshBalance();
      void this.refreshLimits();
      this.running = Object.fromEntries(status.running.map((r) => [r.thread_id, r]));
      this.queues = status.queues ?? {};
      this.online = status.online ?? true;
      this.recent = recent;
      void this.refreshPreviews();
      void this.loadProblems();
      this.approvals = Object.fromEntries(approvals.map((a) => [a.id, a]));
      this.toolsets = toolsets;
      this.conn = 'online';

      const v = this.view;
      if (v.kind === 'agent') {
        if (!agents.some((a) => a.id === v.agentId)) this.goHome();
        else await this.loadChat(v.agentId);
      }
    } catch (e) {
      this.error(e);
    }
  }

  // ---- navigation ------------------------------------------------------------

  goHome() {
    this.view = { kind: 'home' };
    this.thread = null;
    this.messages = [];
  }

  /** Bumped on every navigation, so a slow load can't overwrite a newer one. */
  private openSeq = 0;

  /**
   * Opens an agent's one chat. The switch is immediate; the messages follow,
   * and a reply that arrives after the user has moved on is dropped.
   */
  async openAgent(agentId: string) {
    const seq = ++this.openSeq;
    const v = this.view;
    const same = v.kind === 'agent' && v.agentId === agentId;
    this.view = { kind: 'agent', agentId, threadId: same ? v.threadId : null };
    delete this.unread[agentId];
    if (!same) {
      this.thread = null;
      this.messages = [];
    }
    await this.loadChat(agentId, seq);
  }

  /** Opens an agent's chat and scrolls to one message (from chat search). */
  async openMessage(agentId: string, messageId: string) {
    await this.openAgent(agentId);
    const el = await waitFor(() => document.getElementById(`msg-${messageId}`));
    if (!el) {
      this.toast('That message is no longer in the chat');
      return;
    }
    el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    el.classList.add('search-hit');
    setTimeout(() => el.classList.remove('search-hit'), 2400);
  }

  /** Agents have one chat, so this just opens the agent. */
  openThread(agentId: string, _threadId?: string) {
    return this.openAgent(agentId);
  }

  newChat(agentId: string) {
    return this.openAgent(agentId);
  }

  private async loadChat(agentId: string, seq = this.openSeq) {
    if (!this.api) return;
    this.loadingThread = true;
    try {
      const detail = await this.api.chat(agentId);
      const v = this.view;
      if (seq !== this.openSeq || v.kind !== 'agent' || v.agentId !== agentId) return;
      this.view = { kind: 'agent', agentId, threadId: detail.thread.id };
      this.thread = detail.thread;
      this.messages = detail.messages;
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        if (seq === this.openSeq) this.goHome();
      } else {
        this.error(e);
      }
    } finally {
      if (seq === this.openSeq) this.loadingThread = false;
    }
  }

  // ---- right-click menus ----------------------------------------------------------

  /** Right-click on an orb (or the chat header's menu button). */
  openAgentMenu(agent: Agent, x: number, y: number) {
    const busy = this.isWorking(agent.id);
    this.menu = {
      x,
      y,
      title: agent.name,
      items: [
        { label: 'Open chat', icon: MessageSquare, action: () => this.openAgent(agent.id) },
        { label: 'Edit agent', icon: Pencil, action: () => (this.modal = { kind: 'agent', agentId: agent.id }) },
        {
          label: 'Compacting',
          icon: Archive,
          info: 'Shrinks the chat: older messages become a short summary, so the agent still remembers them while using less of its memory (context).',
          disabled: busy,
          action: () => this.compactChat(agent.id),
        },
        {
          label: 'Clear chat',
          icon: Eraser,
          danger: true,
          disabled: busy,
          confirm: 'Click again to clear the chat',
          action: () => this.clearChat(agent.id),
        },
        ...(agent.role === 'main'
          ? []
          : [
              {
                label: 'Delete agent',
                icon: Trash2,
                danger: true,
                separator: true,
                confirm: `Click again to delete ${agent.name}`,
                action: () => this.deleteAgent(agent.id),
              },
            ]),
      ],
    };
  }

  /** Right-click anywhere else in the app. */
  openAppMenu(x: number, y: number) {
    this.menu = {
      x,
      y,
      items: [
        { label: 'Home', icon: House, hint: 'Ctrl H', action: () => this.goHome() },
        { label: 'Search chats', icon: Search, hint: 'Ctrl K', action: () => (this.modal = { kind: 'search' }) },
        { label: 'New agent', icon: Plus, action: () => this.summonAgent() },
        {
          label: this.theme === 'dark' ? 'Light theme' : 'Dark theme',
          icon: this.theme === 'dark' ? Sun : Moon,
          action: () => this.setTheme(this.theme === 'dark' ? 'light' : 'dark'),
        },
        { label: 'Reload app', icon: RefreshCw, hint: 'Ctrl R', separator: true, action: () => location.reload() },
      ],
    };
  }

  /** Empties an agent's chat (its memory stays). */
  async clearChat(agentId: string) {
    if (!this.api) return;
    try {
      const detail = await this.api.clearChat(agentId);
      if (this.view.kind === 'agent' && this.view.agentId === agentId) {
        this.thread = detail.thread;
        this.messages = [];
      }
      this.toast(`${this.agent(agentId)?.name ?? 'The'} chat cleared`);
    } catch (e) {
      this.error(e);
    }
  }

  /** Summarizes the older part of an agent's chat now. */
  async compactChat(agentId: string) {
    if (!this.api) return;
    try {
      this.toast('Compacting…');
      const detail = await this.api.compactChat(agentId);
      if (this.view.kind === 'agent' && this.view.agentId === agentId) {
        this.thread = detail.thread;
        this.messages = detail.messages;
      }
      this.toast('Chat compacted: older messages are summarized');
    } catch (e) {
      this.error(e);
    }
  }

  async loadThreads(agentId: string): Promise<Thread[]> {
    if (!this.api) return [];
    try {
      const list = await this.api.threads(agentId);
      this.threads[agentId] = list;
      return list;
    } catch (e) {
      this.error(e);
      return [];
    }
  }

  private async loadThread(threadId: string) {
    if (!this.api) return;
    this.loadingThread = true;
    try {
      const detail = await this.api.thread(threadId);
      // The user may have navigated elsewhere while this loaded.
      if (this.view.kind === 'agent' && this.view.threadId === threadId) {
        this.thread = detail.thread;
        this.messages = detail.messages;
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        const v = this.view;
        if (v.kind === 'agent') this.newChat(v.agentId);
      } else {
        this.error(e);
      }
    } finally {
      this.loadingThread = false;
    }
  }

  // ---- chat actions ------------------------------------------------------------

  async send(text: string, files: File[] = []) {
    const api = this.api;
    const agent = this.currentAgent;
    const v = this.view;
    if (!api || !agent || v.kind !== 'agent' || (!text.trim() && files.length === 0)) return;
    try {
      let threadId = v.threadId;
      if (!threadId) {
        await this.loadChat(agent.id);
        const now = this.view;
        threadId = now.kind === 'agent' && now.agentId === agent.id ? now.threadId : null;
        if (!threadId) return false;
      }
      const tid = threadId;
      const attachments = await Promise.all(files.map(async (f) => api.uploadAttachment(tid, f.name, await toBase64(f))));
      const posted = await api.postMessage(threadId, text, attachments);
      if (posted.user_message) this.upsertMessage(posted.user_message);
      if (posted.assistant_message) this.upsertMessage(posted.assistant_message);
      if (text.trim()) this.previews[agent.id] = { agent_id: agent.id, role: 'user', text: text.trim().slice(0, 160), created_at: Date.now() };
      // Queued: it shows above the chat box until the agent gets to it.
      if (posted.queued) {
        const q = this.queues[tid] ?? [];
        if (!q.some((m) => m.id === posted.queued!.id)) this.queues[tid] = [...q, posted.queued];
      }
      return true;
    } catch (e) {
      this.error(e);
      return false;
    }
  }

  async removeQueued(threadId: string, id: string) {
    try {
      await this.api?.removeQueued(threadId, id);
    } catch (e) {
      this.error(e);
    }
  }

  /** Stops what the agent is doing and sends this queued message now. */
  async steerQueued(threadId: string, id: string) {
    try {
      await this.api?.steerQueued(threadId, id);
    } catch (e) {
      this.error(e);
    }
  }

  async stop() {
    const v = this.view;
    if (this.api && v.kind === 'agent' && v.threadId) {
      await this.api.stop(v.threadId).catch((e) => this.error(e));
    }
  }

  async regenerate() {
    const v = this.view;
    if (!this.api || v.kind !== 'agent' || !v.threadId) return;
    try {
      const posted = await this.api.regenerate(v.threadId);
      if (posted.assistant_message) this.upsertMessage(posted.assistant_message);
    } catch (e) {
      this.error(e);
    }
  }

  async renameThread(threadId: string, title: string) {
    if (!this.api || !title.trim()) return;
    try {
      this.upsertThread(await this.api.renameThread(threadId, title.trim()));
    } catch (e) {
      this.error(e);
    }
  }

  async deleteThread(thread: Thread) {
    if (!this.api) return;
    try {
      await this.api.deleteThread(thread.id);
      this.removeThread(thread.id, thread.agent_id);
    } catch (e) {
      this.error(e);
    }
  }

  // ---- agents & providers ----------------------------------------------------------

  async saveAgent(id: string | null, data: NewAgent): Promise<Agent | null> {
    if (!this.api) return null;
    try {
      const agent = id ? await this.api.patchAgent(id, data) : await this.api.createAgent(data);
      this.upsertAgent(agent);
      return agent;
    } catch (e) {
      this.error(e);
      return null;
    }
  }

  async deleteAgent(id: string) {
    if (!this.api) return;
    try {
      await this.api.deleteAgent(id);
      this.removeAgent(id);
    } catch (e) {
      this.error(e);
    }
  }

  async reloadProviders() {
    if (!this.api) return;
    const [providers, defaultModel, reasoning] = await Promise.all([
      this.api.providers(),
      this.api.defaultModel(),
      this.api.reasoning(),
    ]);
    this.providers = providers;
    this.defaultModel = defaultModel;
    this.reasoningDefault = reasoning.effort;
    this.modelOptions = {};
    void this.refreshBalance();
  }

  // ---- live events -----------------------------------------------------------------

  private onEvent(e: IrisEvent) {
    switch (e.type) {
      case 'agent_updated': {
        const isNew = !this.agents.some((a) => a.id === e.agent.id);
        this.upsertAgent(e.agent);
        // A new orb pops into the rail.
        if (isNew) this.setMood(e.agent.id, 'spawn', 1600);
        break;
      }
      case 'agent_deleted':
        this.removeAgent(e.agent_id);
        break;
      case 'thread_updated':
        this.upsertThread(e.thread);
        break;
      case 'thread_deleted':
        this.removeThread(e.thread_id, e.agent_id);
        break;
      case 'message_created':
        this.upsertMessage(e.message);
        break;
      case 'message_completed':
        this.upsertMessage(e.message);
        // A dictated message gets its answer read aloud.
        if (voice.autoSpeak && e.message.role === 'assistant' && this.thread?.id === e.message.thread_id) {
          voice.autoSpeak = false;
          if (e.message.status === 'complete') voice.speak(e.message.id, e.message.content);
        }
        break;
      case 'message_deleted':
        this.messages = this.messages.filter((m) => m.id !== e.message_id);
        break;
      case 'message_delta': {
        if (this.thread?.id !== e.thread_id) break;
        const m = this.messages.find((m) => m.id === e.message_id);
        if (m) {
          if (!m.rounds) m.rounds = [];
          while (m.rounds.length <= e.round) m.rounds.push({ reasoning: '', content: '', tools: [] } as Round);
          const r = m.rounds[e.round];
          if (e.content) {
            r.content += e.content;
            m.content += e.content;
          }
          if (e.reasoning) {
            r.reasoning += e.reasoning;
            m.reasoning += e.reasoning;
          }
        }
        break;
      }
      case 'message_progress': {
        if (this.thread?.id !== e.thread_id) break;
        const m = this.messages.find((m) => m.id === e.message_id);
        if (m) m.rounds = e.rounds;
        break;
      }
      case 'approval_requested':
        this.approvals[e.approval.id] = e.approval;
        break;
      case 'approval_resolved':
        delete this.approvals[e.approval_id];
        break;
      case 'run_started':
        this.running[e.thread_id] = {
          agent_id: e.agent_id,
          thread_id: e.thread_id,
          message_id: e.message_id,
          activity: 'thinking',
        };
        break;
      case 'run_activity': {
        const run = this.running[e.thread_id];
        if (run) {
          run.activity = e.activity;
          run.detail = e.detail;
        }
        // Handing out work: a happy little spin.
        if (e.activity === 'delegate_task' || e.activity === 'create_agent') this.setMood(e.agent_id, 'spin', 1100);
        break;
      }
      case 'problems_changed':
        void this.loadProblems();
        break;
      case 'run_finished':
        this.refreshPreviews(600);
        delete this.running[e.thread_id];
        if (e.status === 'complete') {
          this.setMood(e.agent_id, 'happy', 2600);
          // A reply in a chat that isn't open: the orb nudges until it's read.
          const v = this.view;
          if (!(v.kind === 'agent' && v.agentId === e.agent_id && document.visibilityState === 'visible')) {
            this.unread[e.agent_id] = true;
          }
        }
        else if (e.status === 'error') this.setMood(e.agent_id, 'sad', 4500);
        // A reply just spent credit; DeepSeek updates the balance shortly after.
        setTimeout(() => void this.refreshBalance(), 2500);
        void this.refreshLimits();
        break;
      case 'connectivity':
        this.online = e.online;
        break;
      case 'routines_changed':
        this.routinesVersion++;
        break;
      case 'queue_changed':
        if (e.queue.length) this.queues[e.thread_id] = e.queue;
        else delete this.queues[e.thread_id];
        break;
      case 'link_changed':
        this.linkConnected = e.connected;
        break;
      case 'device_added':
        this.devicesVersion++;
        this.lastDeviceAdded = e.name;
        break;
      case 'providers_changed':
        void this.reloadProviders();
        break;
      case 'lagged':
        void this.refreshAll();
        break;
    }
  }

  private upsertAgent(agent: Agent) {
    const i = this.agents.findIndex((a) => a.id === agent.id);
    if (i >= 0) this.agents[i] = agent;
    else this.agents.push(agent);
  }

  private removeAgent(id: string) {
    this.agents = this.agents.filter((a) => a.id !== id);
    delete this.threads[id];
    this.recent = this.recent.filter((t) => t.agent_id !== id);
    if (this.view.kind === 'agent' && this.view.agentId === id) this.goHome();
  }

  private upsertThread(thread: Thread) {
    const list = this.threads[thread.agent_id];
    if (list) {
      const i = list.findIndex((t) => t.id === thread.id);
      if (i >= 0) list[i] = thread;
      else list.push(thread);
      list.sort(byUpdatedDesc);
    }
    const r = this.recent.findIndex((t) => t.id === thread.id);
    if (r >= 0) this.recent[r] = thread;
    else this.recent.unshift(thread);
    this.recent.sort(byUpdatedDesc);
    if (this.thread?.id === thread.id) this.thread = thread;
  }

  private removeThread(threadId: string, agentId: string) {
    const list = this.threads[agentId];
    if (list) this.threads[agentId] = list.filter((t) => t.id !== threadId);
    this.recent = this.recent.filter((t) => t.id !== threadId);
    delete this.running[threadId];
    if (this.view.kind === 'agent' && this.view.threadId === threadId) void this.openAgent(agentId);
  }

  private upsertMessage(message: Message) {
    if (this.thread?.id !== message.thread_id) return;
    const i = this.messages.findIndex((m) => m.id === message.id);
    if (i >= 0) this.messages[i] = message;
    else this.messages.push(message);
  }

  // ---- toasts & theme ------------------------------------------------------------------

  private toastSeq = 0;

  toast(text: string, kind: Toast['kind'] = 'info') {
    const id = ++this.toastSeq;
    this.toasts.push({ id, text, kind });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === 'error' ? 6000 : 3000);
  }

  error(e: unknown) {
    console.error(e);
    this.toast(e instanceof Error ? e.message : String(e), 'error');
  }

  setTheme(theme: 'dark' | 'light') {
    this.theme = theme;
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem('iris.theme', theme);
    } catch {
      /* storage unavailable */
    }
  }

  setMotion(motion: 'always' | 'system' | 'off') {
    this.motion = motion;
    document.documentElement.dataset.motion = motion;
    try {
      localStorage.setItem('iris.motion', motion);
    } catch {
      /* storage unavailable */
    }
  }

  private loadTheme() {
    let saved: string | null = null;
    let motion: string | null = null;
    try {
      saved = localStorage.getItem('iris.theme');
      motion = localStorage.getItem('iris.motion');
    } catch {
      /* storage unavailable */
    }
    this.setTheme(saved === 'light' ? 'light' : 'dark');
    this.setMotion(motion === 'system' || motion === 'off' ? motion : 'always');
  }
}

/** Polls (each frame, up to ~3 s) until `find` returns an element. */
function waitFor(find: () => HTMLElement | null): Promise<HTMLElement | null> {
  return new Promise((resolve) => {
    const started = performance.now();
    const tick = () => {
      const el = find();
      if (el || performance.now() - started > 3000) resolve(el);
      else requestAnimationFrame(tick);
    };
    tick();
  });
}

/** A file's bytes as base64 (without the data: prefix). */
function toBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split(',')[1] ?? '');
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}

export const app = new IrisApp();
