<script lang="ts">
  import {
    Activity,
    Mic,
    BookOpen,
    Briefcase,
    CalendarClock,
    Play,
    Cpu,
    Gauge,
    Globe,
    GraduationCap,
    Laugh,
    MessageSquareText,
    Microscope,
    Scale,
    Zap,
    Check,
    Copy,
    Info,
    KeyRound,
    LoaderCircle,
    Moon,
    Palette,
    Pencil,
    Plug,
    Plus,
    RefreshCw,
    Search,
    Server,
    Smartphone,
    Sun,
    Trash2,
  } from '@lucide/svelte';
  import { untrack } from 'svelte';
  import { renderSVG } from 'uqr';
  import { app, type SettingsTab } from '../app.svelte';
  import { modelName, modelVersion } from '../format';
  import { clockTime, relativeTime } from '../time';
  import { tip } from '../tooltip';
  import { REASONING_LEVELS, type Device, type LinkStatus, type Pairing, type Limits, type Provider, type ResponseStyle, type Routine, type SearchStatus, type Skill, type McpServer, type VaultAccount, type ToolLogEntry } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import VoiceSettings from './VoiceSettings.svelte';
  import Modal from './Modal.svelte';
  import Select from './Select.svelte';
  import UsageBar from './UsageBar.svelte';

  let { tab: initialTab }: { tab?: SettingsTab } = $props();
  let tab = $state<SettingsTab>(untrack(() => initialTab) ?? 'models');

  const tabs = [
    { id: 'models', label: 'Models', icon: Cpu },
    { id: 'providers', label: 'Providers', icon: Server },
    { id: 'responses', label: 'Personality & memory', icon: MessageSquareText },
    { id: 'voice', label: 'Voice & calls', icon: Mic },
    { id: 'routines', label: 'Routines', icon: CalendarClock },
    { id: 'skills', label: 'Skills', icon: BookOpen },
    { id: 'connections', label: 'Connections', icon: Plug },
    { id: 'accounts', label: 'Accounts', icon: KeyRound },
    { id: 'limits', label: 'Limits', icon: Gauge },
    { id: 'search', label: 'Search', icon: Search },
    { id: 'activity', label: 'Activity', icon: Activity },
    { id: 'devices', label: 'Devices', icon: Smartphone },
    { id: 'remote', label: 'Remote access', icon: Globe },
    { id: 'appearance', label: 'Appearance', icon: Palette },
    { id: 'about', label: 'About', icon: Info },
  ] as const;

  // ---- default model -----------------------------------------------------------

  let dmProvider = $state(untrack(() => app.defaultModel?.provider ?? app.chatProviders[0]?.id ?? ''));
  let dmModel = $state(untrack(() => app.defaultModel?.model ?? ''));
  let dmModels = $state<string[]>([]);
  let dmLoading = $state(false);
  let dmError = $state('');
  let dmSaving = $state(false);

  async function loadDefaultModels(provider: string) {
    if (!app.api || !provider) return;
    dmLoading = true;
    dmError = '';
    try {
      dmModels = await app.api.models(provider);
      if (!dmModels.includes(dmModel) && dmModels.length) dmModel = dmModels[0];
    } catch (e) {
      dmModels = [];
      const p = app.providers.find((x) => x.id === provider);
      dmError = p && !p.has_key ? 'Add an API key below to load the models.' : e instanceof Error ? e.message : String(e);
    } finally {
      dmLoading = false;
    }
  }

  $effect(() => {
    if (tab === 'models') void loadDefaultModels(dmProvider);
  });

  async function saveDefaultModel() {
    dmSaving = true;
    await app.setDefaultModel(dmProvider, dmModel);
    dmSaving = false;
  }

  // ---- fallback model ----------------------------------------------------------

  let fbEnabled = $state(false);
  let fbSaved = $state<{ enabled: boolean; provider: string; model: string } | null>(null);
  let fbProvider = $state('');
  let fbModel = $state('');
  let fbModels = $state<string[]>([]);
  let fbSaving = $state(false);

  // The auto mode new agents (orbs from "+", designed or created by Iris) start with.
  let autoDefault = $state<'off' | 'careful' | 'full'>('off');
  async function loadAutoDefault() {
    if (app.api) autoDefault = (await app.api.autoDefault().catch(() => ({ mode: autoDefault }))).mode;
  }
  async function setAutoDefault(mode: 'off' | 'careful' | 'full') {
    if (!app.api || mode === autoDefault) return;
    const before = autoDefault;
    autoDefault = mode;
    try {
      await app.api.putAutoDefault(mode);
      app.toast(mode === 'off' ? 'New agents will ask before every command' : mode === 'careful' ? 'New agents start in Careful auto' : 'New agents start in Full auto');
    } catch (e) {
      autoDefault = before;
      app.error(e);
    }
  }

  async function loadFallback() {
    if (!app.api) return;
    try {
      const f = await app.api.fallback();
      fbSaved = f;
      fbEnabled = f.enabled;
      // Suggest a provider other than the default one.
      fbProvider = f.provider || app.chatProviders.find((p) => p.id !== app.defaultModel?.provider)?.id || app.chatProviders[0]?.id || '';
      fbModel = f.model;
      void loadFallbackModels();
    } catch (e) {
      app.error(e);
    }
  }

  async function loadFallbackModels() {
    if (!app.api || !fbProvider) return;
    try {
      fbModels = await app.api.models(fbProvider);
      if (!fbModels.includes(fbModel) && fbModels.length) {
        const preset = app.providers.find((p) => p.id === fbProvider)?.default_model ?? '';
        fbModel = fbModels.includes(preset) ? preset : fbModels[0];
      }
    } catch {
      fbModels = [];
      if (!fbModel) fbModel = app.providers.find((p) => p.id === fbProvider)?.default_model ?? '';
    }
  }

  async function saveFallback(enabled = fbEnabled) {
    if (!app.api) return;
    fbSaving = true;
    try {
      fbSaved = await app.api.putFallback({ enabled, provider: fbProvider, model: fbModel.trim() });
      fbEnabled = fbSaved.enabled;
      app.toast(enabled ? `Fallback on: ${fbModel}` : 'Fallback off');
    } catch (e) {
      app.error(e);
    } finally {
      fbSaving = false;
    }
  }

  function toggleFallback() {
    fbEnabled = !fbEnabled;
    // Turning it off takes effect at once; turning it on waits for Save.
    if (!fbEnabled && fbSaved?.enabled) void saveFallback(false);
  }

  const fbDirty = $derived(
    !!fbSaved && fbEnabled && (!fbSaved.enabled || fbSaved.provider !== fbProvider || fbSaved.model !== fbModel.trim()),
  );

  // ---- response style ----------------------------------------------------------

  const styles: { id: ResponseStyle['preset']; label: string; description: string; icon: typeof Scale }[] = [
    { id: 'balanced', label: 'Balanced', description: 'Clear and friendly, with detail when it helps.', icon: Scale },
    { id: 'concise', label: 'Straight to the point', description: 'The answer first. No fluff, no recap.', icon: Zap },
    { id: 'analyst', label: 'Deep analyst', description: 'Breaks it down, weighs options, risks and unknowns.', icon: Microscope },
    { id: 'witty', label: 'Witty', description: 'Banter and dry humor, still useful and correct.', icon: Laugh },
    { id: 'teacher', label: 'Teacher', description: 'Step by step in plain words, with examples.', icon: GraduationCap },
    { id: 'professional', label: 'Professional', description: 'Polished and structured, ready to forward.', icon: Briefcase },
  ];
  let style = $state<ResponseStyle>({ preset: 'balanced', custom: '' });
  let savedStyle = $state<ResponseStyle | null>(null);
  let savingStyle = $state(false);

  async function loadStyle() {
    if (!app.api) return;
    try {
      savedStyle = await app.api.style();
      style = { ...savedStyle };
    } catch (e) {
      app.error(e);
    }
  }

  // What every agent knows about the user (USER.md).
  let userMemory = $state('');
  let savedUserMemory = $state('');
  let userLimit = $state(2000);
  async function loadUserMemory() {
    if (!app.api) return;
    try {
      const m = await app.api.userMemory();
      userMemory = savedUserMemory = m.text;
      userLimit = m.limit;
    } catch (e) {
      app.error(e);
    }
  }
  async function saveUserMemory() {
    if (!app.api) return;
    try {
      const m = await app.api.putUserMemory(userMemory);
      userMemory = savedUserMemory = m.text;
      app.toast('Saved what your agents know about you');
    } catch (e) {
      app.error(e);
    }
  }

  async function saveStyle() {
    if (!app.api) return;
    savingStyle = true;
    try {
      savedStyle = await app.api.putStyle({ preset: style.preset, custom: style.custom.trim() });
      style = { ...savedStyle };
      app.toast('Response style saved');
    } catch (e) {
      app.error(e);
    } finally {
      savingStyle = false;
    }
  }

  // ---- account vault -------------------------------------------------------------

  let accounts = $state<VaultAccount[]>([]);
  let acEdit = $state<null | { id: string | null; name: string; site: string; username: string; password: string }>(null);
  let acDelete = $state<string | null>(null);

  async function loadAccounts() {
    if (!app.api) return;
    try {
      accounts = await app.api.vault();
    } catch (e) {
      app.error(e);
    }
  }

  $effect(() => {
    if (tab === 'accounts') void loadAccounts();
  });

  async function saveAccount() {
    if (!app.api || !acEdit) return;
    const { id, ...fields } = acEdit;
    try {
      if (id) await app.api.editAccount(id, fields);
      else await app.api.addAccount(fields);
      app.toast(`${fields.name} saved`);
      acEdit = null;
      void loadAccounts();
    } catch (e) {
      app.error(e);
    }
  }

  // ---- skills ------------------------------------------------------------------

  let skills = $state<Skill[]>([]);
  // The skill being added or edited (`original` is its name before editing).
  let skEdit = $state<{ original: string | null; name: string; description: string; body: string } | null>(null);
  let skOpen = $state<string | null>(null);
  let skDelete = $state<string | null>(null);

  async function loadSkills() {
    if (!app.api) return;
    try {
      skills = await app.api.skills();
    } catch (e) {
      app.error(e);
    }
  }

  $effect(() => {
    if (tab === 'skills') void loadSkills();
  });

  async function saveSkill() {
    if (!app.api || !skEdit) return;
    try {
      const saved = await app.api.putSkill(skEdit.name, skEdit.description, skEdit.body);
      // Renamed: the old one goes.
      if (skEdit.original && skEdit.original !== saved.name) await app.api.deleteSkill(skEdit.original);
      app.toast(`Skill “${saved.name}” saved`);
      skEdit = null;
      void loadSkills();
    } catch (e) {
      app.error(e);
    }
  }

  // ---- connections (MCP servers) ------------------------------------------------

  let mcpServers = $state<McpServer[]>([]);
  let mcpBusy = $state(false);
  let mcpDelete = $state<string | null>(null);
  type McpForm = {
    original: string | null;
    name: string;
    kind: 'command' | 'url';
    commandLine: string;
    url: string;
    /** KEY=value lines; a KEY= line keeps the stored value. */
    secrets: string;
    trustAll: boolean;
    autoApprove: string[];
  };
  let mcpForm = $state<McpForm | null>(null);

  async function loadMcp() {
    if (!app.api) return;
    try {
      mcpServers = await app.api.mcp();
    } catch (e) {
      app.error(e);
    }
  }

  $effect(() => {
    if (tab === 'connections') void loadMcp();
  });

  /** `npx -y "@scope/server" --flag` → ["npx", "-y", "@scope/server", "--flag"]. */
  function splitCommand(line: string): string[] {
    return (line.match(/"[^"]*"|'[^']*'|\S+/g) ?? []).map((p) => p.replace(/^(["'])(.*)\1$/, '$2'));
  }

  function editMcp(s: McpServer) {
    const keys = s.transport === 'stdio' ? s.env_keys : s.header_keys;
    mcpForm = {
      original: s.name,
      name: s.name,
      kind: s.transport === 'stdio' ? 'command' : 'url',
      commandLine: [s.command, ...s.args].map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(' '),
      url: s.url,
      secrets: keys.map((k) => `${k}=`).join('\n'),
      trustAll: s.auto_approve.includes('*'),
      autoApprove: s.auto_approve.filter((a) => a !== '*'),
    };
  }

  async function saveMcp() {
    if (!app.api || !mcpForm) return;
    const f = mcpForm;
    const secrets: Record<string, string> = {};
    for (const line of f.secrets.split('\n')) {
      const i = line.indexOf('=');
      if (i > 0) secrets[line.slice(0, i).trim()] = line.slice(i + 1).trim();
    }
    const [command = '', ...args] = f.kind === 'command' ? splitCommand(f.commandLine) : [];
    mcpBusy = true;
    try {
      if (f.original && f.original !== f.name) await app.api.deleteMcp(f.original);
      mcpServers = await app.api.putMcp(f.name.trim(), {
        command,
        args,
        url: f.kind === 'url' ? f.url.trim() : '',
        env: f.kind === 'command' ? secrets : {},
        headers: f.kind === 'url' ? secrets : {},
        enabled: true,
        auto_approve: f.trustAll ? ['*'] : f.autoApprove,
      });
      const saved = mcpServers.find((s) => s.name === f.name.trim());
      if (saved?.connected) app.toast(`${saved.name} connected · ${saved.tools.length} tools`);
      else app.toast(`${f.name} saved, but it didn't connect: ${saved?.error ?? 'unknown error'}`, 'error');
      mcpForm = null;
    } catch (e) {
      app.error(e);
    } finally {
      mcpBusy = false;
    }
  }

  async function setMcpEnabled(s: McpServer, enabled: boolean) {
    if (!app.api) return;
    mcpBusy = true;
    try {
      mcpServers = await app.api.putMcp(s.name, { command: s.command, args: s.args, url: s.url, enabled, auto_approve: s.auto_approve });
    } catch (e) {
      app.error(e);
    } finally {
      mcpBusy = false;
    }
  }

  async function refreshMcp() {
    if (!app.api) return;
    mcpBusy = true;
    try {
      mcpServers = await app.api.refreshMcp();
    } catch (e) {
      app.error(e);
    } finally {
      mcpBusy = false;
    }
  }

  // ---- routines ----------------------------------------------------------------

  let routines = $state<Routine[]>([]);
  type RoutineForm = { id: string | null; agent_id: string; name: string; schedule: string; kind: Routine['kind']; body: string; notify: Routine['notify'] };
  let rtForm = $state<RoutineForm | null>(null);
  let rtDelete = $state<string | null>(null);
  const rtExamples = ['every 30m', 'every 2h', 'daily 08:00', 'weekdays 09:00', 'weekly mon 09:00'];

  async function loadRoutines() {
    if (!app.api) return;
    try {
      routines = await app.api.routines();
    } catch (e) {
      app.error(e);
    }
  }

  $effect(() => {
    void app.routinesVersion;
    if (tab === 'routines') void loadRoutines();
  });

  function newRoutine() {
    rtForm = { id: null, agent_id: app.mainAgent?.id ?? app.agents[0]?.id ?? '', name: '', schedule: 'daily 08:00', kind: 'prompt', body: '', notify: 'failure' };
  }

  function editRoutine(r: Routine) {
    rtForm = { id: r.id, agent_id: r.agent_id, name: r.name, schedule: r.schedule, kind: r.kind, body: r.body, notify: r.notify };
  }

  async function saveRoutine() {
    if (!app.api || !rtForm) return;
    const { id, ...fields } = rtForm;
    try {
      if (id) await app.api.editRoutine(id, fields);
      else await app.api.createRoutine(fields);
      app.toast(id ? `“${fields.name}” updated` : `Routine “${fields.name}” scheduled`);
      rtForm = null;
      void loadRoutines();
    } catch (e) {
      app.error(e);
    }
  }

  const when = (ms: number | null) =>
    ms ? new Date(ms).toLocaleString(undefined, { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : '—';

  // ---- limits ------------------------------------------------------------------

  const dailyPresets = [0, 100_000, 250_000, 500_000, 1_000_000, 2_000_000];
  const weeklyPresets = [0, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000];
  let limits = $state<Limits | null>(null);
  let daily = $state(0);
  let weekly = $state(0);
  let savingLimits = $state(false);

  const tokens = (n: number) =>
    n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(2)}M` : n >= 1_000 ? `${+(n / 1_000).toFixed(1)}k` : String(n);

  async function loadLimits() {
    if (!app.api) return;
    limits = await app.api.limits().catch(() => null);
    if (limits) {
      daily = limits.daily_tokens;
      weekly = limits.weekly_tokens;
    }
  }

  async function saveLimits() {
    if (!app.api) return;
    savingLimits = true;
    try {
      limits = await app.api.putLimits(Math.max(0, Math.round(daily)), Math.max(0, Math.round(weekly)));
      app.limits = limits;
      app.toast('Token limits saved');
    } catch (e) {
      app.error(e);
    } finally {
      savingLimits = false;
    }
  }

  // ---- search ----------------------------------------------------------------

  let search = $state<SearchStatus | null>(null);
  let searchProvider = $state<SearchStatus['provider']>('auto');
  let tavilyKey = $state('');
  let savingSearch = $state(false);

  const searchName = (id: SearchStatus['active']) => ({ deepseek: 'DeepSeek', tavily: 'Tavily', duckduckgo: 'DuckDuckGo' })[id];

  async function loadSearch() {
    if (!app.api) return;
    search = await app.api.search().catch(() => null);
    if (search) searchProvider = search.provider;
  }

  async function saveSearch(removeKey = false) {
    if (!app.api) return;
    savingSearch = true;
    try {
      search = await app.api.putSearch(searchProvider, removeKey ? '' : tavilyKey.trim() || undefined);
      tavilyKey = '';
      app.toast(`Web search now uses ${searchName(search.active)}`);
    } catch (e) {
      app.error(e);
    } finally {
      savingSearch = false;
    }
  }

  // ---- activity ----------------------------------------------------------------

  let audit = $state<ToolLogEntry[]>([]);
  let loadingAudit = $state(false);

  async function loadAudit() {
    if (!app.api) return;
    loadingAudit = true;
    audit = await app.api.audit(150).catch(() => []);
    loadingAudit = false;
  }

  // What a log entry's chip says. A task check that found the task still going
  // (or waiting for you) ran fine, but the task itself isn't done.
  function chipFor(e: ToolLogEntry): { label: string; style: string } {
    if (e.status === 'done' && e.tool === 'check_task') {
      if (e.summary.includes('needs your approval')) return { label: 'waiting', style: 'bg-warn/10 text-warn' };
      if (e.summary.includes('still working')) return { label: 'in progress', style: 'bg-accent-soft text-accent-hi' };
    }
    return (
      {
        done: { label: 'ok', style: 'bg-success/10 text-success' },
        error: { label: 'failed', style: 'bg-danger-soft text-danger' },
        denied: { label: 'denied', style: 'bg-warn/10 text-warn' },
        cancelled: { label: 'stopped', style: 'bg-hover text-faint' },
      } as Record<string, { label: string; style: string }>
    )[e.status] ?? { label: e.status, style: 'text-faint' };
  }

  // ---- providers -----------------------------------------------------------

  // Providers people commonly use; all speak the OpenAI chat API.
  const presets = [
    { id: 'deepseek', name: 'DeepSeek', base_url: 'https://api.deepseek.com', default_model: 'deepseek-flash' },
    { id: 'openai', name: 'OpenAI', base_url: 'https://api.openai.com/v1', default_model: 'gpt-5-mini' },
    { id: 'anthropic', name: 'Anthropic (Claude)', base_url: 'https://api.anthropic.com/v1', default_model: 'claude-haiku-5-5' },
    { id: 'gemini', name: 'Google Gemini', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai', default_model: 'gemini-2.5-flash' },
    { id: 'openrouter', name: 'OpenRouter', base_url: 'https://openrouter.ai/api/v1', default_model: 'openrouter/auto' },
    { id: 'groq', name: 'Groq', base_url: 'https://api.groq.com/openai/v1', default_model: 'llama-3.3-70b-versatile' },
    { id: 'mistral', name: 'Mistral', base_url: 'https://api.mistral.ai/v1', default_model: 'mistral-small-latest' },
    { id: 'xai', name: 'xAI (Grok)', base_url: 'https://api.x.ai/v1', default_model: 'grok-3-mini' },
    { id: 'together', name: 'Together AI', base_url: 'https://api.together.xyz/v1', default_model: 'meta-llama/Llama-3.3-70B-Instruct-Turbo' },
    { id: 'ollama', name: 'Ollama (local)', base_url: 'http://localhost:11434/v1', default_model: 'llama3.2' },
    { id: 'lmstudio', name: 'LM Studio (local)', base_url: 'http://localhost:1234/v1', default_model: 'local-model' },
    { id: 'custom', name: 'Custom (OpenAI-compatible)', base_url: '', default_model: '' },
  ];

  let editing = $state<null | { isNew: boolean; id: string; name: string; base_url: string; default_model: string; api_key: string }>(null);
  let savingProvider = $state(false);
  let testing = $state<Record<string, string>>({});
  let deleteId = $state<string | null>(null);

  function editProvider(p: Provider) {
    editing = { isNew: false, id: p.id, name: p.name, base_url: p.base_url, default_model: p.default_model, api_key: '' };
  }

  // A free id: "openai", then "openai-2", ...
  function freeId(id: string) {
    let candidate = id;
    for (let n = 2; app.providers.some((p) => p.id === candidate); n++) candidate = `${id}-${n}`;
    return candidate;
  }

  function newProvider(presetId?: string) {
    const preset =
      presets.find((p) => p.id === presetId) ?? presets.find((p) => !app.providers.some((x) => x.id === p.id)) ?? presets[0];
    editing = { isNew: true, ...preset, id: freeId(preset.id), api_key: '' };
  }

  async function saveProvider() {
    if (!editing || !app.api) return;
    savingProvider = true;
    try {
      const { id, name, base_url, default_model, api_key } = editing;
      await app.api.putProvider(id.trim(), {
        name,
        base_url,
        default_model,
        // Blank while editing means "keep the current key".
        ...(api_key.trim() || editing.isNew ? { api_key: api_key.trim() } : {}),
      });
      await app.reloadProviders();
      app.toast(`${name} saved`);
      editing = null;
      // A new key unlocks the provider's model list.
      void loadDefaultModels(dmProvider);
    } catch (e) {
      app.error(e);
    } finally {
      savingProvider = false;
    }
  }

  async function testProvider(id: string) {
    if (!app.api) return;
    testing[id] = '…';
    try {
      const models = await app.api.models(id);
      testing[id] = `✓ Connected · ${models.length} models`;
    } catch (e) {
      testing[id] = `✗ ${e instanceof Error ? e.message : e}`;
    }
  }

  async function removeProvider(id: string) {
    if (!app.api) return;
    try {
      await app.api.deleteProvider(id);
      await app.reloadProviders();
    } catch (e) {
      app.error(e);
    }
    deleteId = null;
  }

  const keyLabel = (p: Provider) =>
    ({ file: 'Key stored', env: 'Key from environment', inline: 'Key in config', none: 'No API key' })[p.key_source];

  // ---- devices -------------------------------------------------------------

  let devices = $state<Device[]>([]);
  let newDeviceName = $state('');
  let newToken = $state<{ name: string; token: string } | null>(null);
  let tokenCopied = $state(false);

  async function loadDevices() {
    if (app.api) devices = await app.api.devices().catch(() => []);
  }

  async function createDevice() {
    if (!app.api || !newDeviceName.trim()) return;
    try {
      const created = await app.api.createDevice(newDeviceName.trim());
      newToken = { name: created.device.name, token: created.token };
      newDeviceName = '';
      await loadDevices();
    } catch (e) {
      app.error(e);
    }
  }

  async function revoke(d: Device) {
    if (!app.api) return;
    try {
      await app.api.deleteDevice(d.id);
      await loadDevices();
    } catch (e) {
      app.error(e);
    }
  }

  // ---- remote access (I.R.I.S. Link) ----
  let link = $state<LinkStatus | null>(null);
  let linkUrl = $state('');
  let linkSaving = $state(false);
  let pairing = $state<Pairing | null>(null);
  let pairQr = $state('');
  let pairedName = $state<string | null>(null);
  let loginUser = $state('');
  let loginPass = $state('');
  let loginSaving = $state(false);

  async function loadLink() {
    if (!app.api) return;
    link = await app.api.linkStatus().catch(() => null);
    if (link) {
      linkUrl = link.url;
      app.linkConnected = link.connected;
      loginUser = link.login_user ?? loginUser;
    }
  }

  async function saveLink(enabled: boolean) {
    if (!app.api) return;
    linkSaving = true;
    try {
      link = await app.api.putLink(enabled, linkUrl.trim());
      linkUrl = link.url;
    } catch (e) {
      app.error(e);
    } finally {
      linkSaving = false;
    }
  }

  async function makePairing() {
    if (!app.api) return;
    try {
      pairing = await app.api.newPairing();
      pairQr = renderSVG(pairing.url, { border: 2 });
      pairedName = null;
    } catch (e) {
      app.error(e);
    }
  }

  async function saveLogin() {
    if (!app.api) return;
    loginSaving = true;
    try {
      link = await app.api.setLinkLogin(loginUser.trim(), loginPass);
      loginPass = '';
    } catch (e) {
      app.error(e);
    } finally {
      loginSaving = false;
    }
  }

  async function removeLogin() {
    if (!app.api) return;
    try {
      link = await app.api.clearLinkLogin();
    } catch (e) {
      app.error(e);
    }
  }

  // A device paired while the code was on screen: hide the code.
  $effect(() => {
    if (app.devicesVersion && app.lastDeviceAdded && untrack(() => pairing)) {
      pairedName = app.lastDeviceAdded;
      pairing = null;
    }
  });

  $effect(() => {
    if (tab === 'devices') {
      void app.devicesVersion;
      void loadDevices();
    }
    if (tab === 'remote') void loadLink();
    if (tab === 'models') {
      void loadFallback();
      void loadAutoDefault();
    }
    if (tab === 'responses') {
      void loadStyle();
      void loadUserMemory();
    }
    if (tab === 'search') void loadSearch();
    if (tab === 'limits') void loadLimits();
    if (tab === 'activity') void loadAudit();
  });
</script>

<Modal title="Settings" onclose={() => (app.modal = null)} width={820}>
  <!-- A fixed height, so the window doesn't jump when switching tabs -->
  <div class="flex h-full flex-col md:h-[min(620px,calc(100vh-9rem))] md:flex-row">
    <nav class="flex shrink-0 gap-1 overflow-x-auto border-b border-line p-2 scroll-none md:block md:w-56 md:space-y-0.5 md:overflow-y-auto md:border-r md:border-b-0 md:p-3">
      {#each tabs as t (t.id)}
        <button
          class="flex shrink-0 items-center gap-2.5 rounded-lg px-3 py-2 text-[13px] whitespace-nowrap transition md:w-full {tab === t.id
            ? 'bg-elevated font-medium text-text'
            : 'text-muted hover:bg-hover hover:text-text'}"
          onclick={() => (tab = t.id)}
        >
          <t.icon size={15} />
          {t.label}
        </button>
      {/each}
    </nav>

    <div class="min-w-0 flex-1 overflow-y-auto p-4 scroll-thin md:p-6">
      {#if tab === 'models'}
          <!-- Default model: one choice for every agent -->
          <div class="mb-7 rounded-xl border border-accent/30 bg-accent-soft/40 p-4">
            <div class="text-[14px] font-semibold">Default model</div>
            <p class="mb-3 text-[12.5px] text-faint">Every agent uses this model unless you give it its own (in the agent's settings).</p>
            <div class="flex flex-wrap items-end gap-2">
              {#if app.chatProviders.length > 1}
                <div class="w-full shrink-0 sm:w-40">
                  <label class="label" for="dm-provider">Provider</label>
                  <Select
                    id="dm-provider"
                    bind:value={dmProvider}
                    options={app.chatProviders.map((p) => ({ value: p.id, label: p.name }))}
                    onchange={() => loadDefaultModels(dmProvider)}
                  />
                </div>
              {/if}
              <div class="min-w-0 flex-1 basis-52">
                <label class="label" for="dm-model">Model</label>
                {#if dmModels.length}
                  <Select id="dm-model" class="text-[13px]" bind:value={dmModel} options={dmModels.map((m) => ({ value: m, label: modelName(m) }))} />
                {:else}
                  <input id="dm-model" class="input font-mono text-[13px]" bind:value={dmModel} placeholder="model id" />
                {/if}
              </div>
              <button
                class="btn-primary shrink-0 py-2.5!"
                onclick={saveDefaultModel}
                disabled={dmSaving || !dmModel.trim() ||
                  (dmModel === app.defaultModel?.model && dmProvider === app.defaultModel?.provider)}
              >
                {#if dmSaving || dmLoading}<LoaderCircle size={14} class="animate-spin" />{/if} Use for all agents
              </button>
            </div>
            {#if dmError}
              <p class="hint text-warn!">{dmError}</p>
            {:else if app.defaultModel}
              <p class="hint">Now: <b class="text-text">{modelVersion(app.defaultModel.model) ?? app.modelLabel()}</b>{dmModels.length ? ` · ${dmModels.length} models available` : ''}</p>
            {/if}

            <!-- Reasoning effort: how hard every agent thinks unless it has its own level -->
            <div class="mt-4 flex flex-wrap items-end gap-x-4 gap-y-2 border-t border-accent/20 pt-4">
              <div class="w-full shrink-0 sm:w-56">
                <label class="label" for="dr-level">Reasoning effort</label>
                <Select
                  id="dr-level"
                  value={app.reasoningDefault}
                  options={[{ value: '', label: 'Auto · the model decides' }, ...REASONING_LEVELS.map((l) => ({ value: l.id as string, label: l.label }))]}
                  onchange={(v) => app.setReasoningDefault(v)}
                />
              </div>
              <p class="hint">
                How hard all agents think, including Iris. Agents can change their own level from the chat box. Higher
                levels answer more carefully but are slower and use more tokens.
              </p>
            </div>

            <!-- The auto mode new agents start with -->
            <div class="mt-4 border-t border-accent/20 pt-4">
              <div class="label">Auto mode for new agents</div>
              <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
                {#each [{ id: 'off', title: 'Off', text: 'Every command asks you first.' }, { id: 'careful', title: 'Careful', text: 'An AI check approves safe commands.' }, { id: 'full', title: 'Full auto', text: 'Only catastrophic commands ask you.' }] as const as m (m.id)}
                  <button
                    class="rounded-xl border px-3 py-2.5 text-left transition {autoDefault === m.id
                      ? 'border-accent/70 bg-accent-soft'
                      : 'border-line-strong hover:bg-hover'}"
                    onclick={() => setAutoDefault(m.id)}
                  >
                    <div class="text-[13px] font-medium">{m.title}</div>
                    <div class="text-[11.5px] text-faint">{m.text}</div>
                  </button>
                {/each}
              </div>
              <p class="hint">
                Orbs from "+", designed agents and agents Iris creates start like this; change one later from its chat
                box. Paying, installing and other risky steps always ask.
              </p>
            </div>
          </div>

          <!-- Fallback: retried here when the main provider fails -->
          <div class="mb-7 rounded-xl border border-line bg-surface/60 p-4">
            <div class="flex items-start gap-3">
              <div class="min-w-0 flex-1">
                <div class="text-[14px] font-semibold">Fallback model</div>
                <p class="text-[12.5px] text-faint">
                  If the main provider fails (down, out of credit, rate limited), the reply is retried on this one.
                </p>
              </div>
              <button
                class="mt-1 flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 transition {fbEnabled ? 'bg-accent' : 'bg-line-strong'}"
                onclick={toggleFallback}
                role="switch"
                aria-checked={fbEnabled}
                aria-label="Use a fallback model"
              >
                <span class="h-4 w-4 rounded-full bg-white transition {fbEnabled ? 'translate-x-4' : ''}"></span>
              </button>
            </div>
            {#if fbEnabled}
              <div class="mt-3 flex flex-wrap items-end gap-2">
                <div class="w-full shrink-0 sm:w-40">
                  <label class="label" for="fb-provider">Provider</label>
                  <Select
                    id="fb-provider"
                    bind:value={fbProvider}
                    options={app.chatProviders.map((p) => ({ value: p.id, label: p.name }))}
                    onchange={() => {
                      fbModel = '';
                      void loadFallbackModels();
                    }}
                  />
                </div>
                <div class="min-w-0 flex-1 basis-52">
                  <label class="label" for="fb-model">Model</label>
                  {#if fbModels.length}
                    <Select id="fb-model" class="text-[13px]" bind:value={fbModel} options={fbModels.map((m) => ({ value: m, label: modelName(m) }))} />
                  {:else}
                    <input id="fb-model" class="input font-mono text-[13px]" bind:value={fbModel} placeholder="model id" />
                  {/if}
                </div>
                <button class="btn-primary shrink-0 py-2.5!" onclick={() => saveFallback(true)} disabled={fbSaving || !fbModel.trim() || !fbDirty}>
                  {#if fbSaving}<LoaderCircle size={14} class="animate-spin" />{/if} Save
                </button>
              </div>
              {#if fbProvider === app.defaultModel?.provider}
                <p class="hint text-warn!">Same provider as the default model: this only helps if that one model fails. Add another provider (Providers tab) for real backup.</p>
              {:else if fbSaved?.enabled && !fbDirty}
                <p class="hint">On: <b class="text-text">{fbSaved.model}</b> ({app.providers.find((p) => p.id === fbSaved?.provider)?.name ?? fbSaved.provider})</p>
              {/if}
            {/if}
          </div>

      {:else if tab === 'providers'}
        {#if editing}
          <h3 class="mb-4 text-[15px] font-semibold">{editing.isNew ? 'Add provider' : `Edit ${editing.name}`}</h3>
          {#if editing.isNew}
            <div class="mb-4 max-w-xs">
              <label class="label" for="p-preset">Provider</label>
              <Select
                id="p-preset"
                value={presets.find((p) => editing && freeId(p.id) === editing.id)?.id ?? 'custom'}
                options={presets.map((p) => ({ value: p.id, label: p.name }))}
                onchange={(v) => newProvider(v)}
              />
            </div>
          {/if}
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="label" for="p-name">Name</label>
              <input id="p-name" class="input" bind:value={editing.name} />
            </div>
            <div>
              <label class="label" for="p-id">ID</label>
              <input id="p-id" class="input font-mono text-[13px]" bind:value={editing.id} disabled={!editing.isNew} />
            </div>
            <div class="col-span-2">
              <label class="label" for="p-url">Base URL</label>
              <input id="p-url" class="input font-mono text-[13px]" bind:value={editing.base_url} />
              <p class="hint">Any OpenAI-compatible API (the part before <code>/chat/completions</code>).</p>
            </div>
            <div>
              <label class="label" for="p-model">Default model</label>
              <input id="p-model" class="input font-mono text-[13px]" bind:value={editing.default_model} />
            </div>
            <div>
              <label class="label" for="p-key">API key</label>
              <input
                id="p-key"
                class="input font-mono text-[13px]"
                type="password"
                bind:value={editing.api_key}
                placeholder={editing.isNew ? 'sk-…' : 'Leave blank to keep the current key'}
                autocomplete="off"
              />
            </div>
          </div>
          <p class="hint mt-3">Keys are stored only on this machine, outside the config file, and never sent to the app.</p>
          <div class="mt-5 flex justify-end gap-2">
            <button class="btn-ghost" onclick={() => (editing = null)}>Cancel</button>
            <button class="btn-primary" onclick={saveProvider} disabled={savingProvider}>
              {#if savingProvider}<LoaderCircle size={14} class="animate-spin" />{/if}Save provider
            </button>
          </div>
        {:else}
          <div class="mb-4 flex items-center justify-between">
            <div>
              <h3 class="text-[15px] font-semibold">AI providers</h3>
              <p class="text-[12.5px] text-faint">Add as many as you like: OpenAI, Anthropic, Gemini, OpenRouter, local models…</p>
            </div>
            <button class="btn-outline shrink-0 whitespace-nowrap" onclick={() => newProvider()}><Plus size={14} /> Add provider</button>
          </div>
          <div class="space-y-2.5">
            {#each app.providers as p (p.id)}
              <div class="rounded-xl border border-line bg-surface px-4 py-3.5">
                <div class="flex items-center gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="text-[14px] font-medium">{p.name}</span>
                      {#if p.voice_only}
                        <span class="rounded-full bg-hover px-2 py-0.5 text-[11px] text-faint" use:tip={'Used only to turn your voice into text. It is never a chat model.'}>Speech to text only</span>
                      {/if}
                      <span
                        class="flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] {p.has_key
                          ? 'bg-success/10 text-success'
                          : 'bg-warn/10 text-warn'}"
                      >
                        <KeyRound size={11} />{keyLabel(p)}
                      </span>
                    </div>
                    <div class="mt-0.5 truncate font-mono text-[12px] text-faint">{p.base_url} · {p.default_model}</div>
                  </div>
                  <button class="btn-outline px-3! py-1.5! text-[12.5px]!" onclick={() => testProvider(p.id)}>Test</button>
                  <button class="btn-ghost px-2.5!" onclick={() => editProvider(p)} aria-label="Edit"><Pencil size={14} /></button>
                  {#if deleteId === p.id}
                    <button class="btn-danger" onclick={() => removeProvider(p.id)}>Remove</button>
                  {:else}
                    <button class="btn-ghost px-2.5! hover:text-danger!" onclick={() => (deleteId = p.id)} aria-label="Remove">
                      <Trash2 size={14} />
                    </button>
                  {/if}
                </div>
                {#if testing[p.id]}
                  <div class="mt-2 text-[12px] {testing[p.id].startsWith('✓') ? 'text-success' : testing[p.id].startsWith('✗') ? 'text-danger' : 'text-faint'}">
                    {testing[p.id]}
                  </div>
                {/if}
              </div>
            {:else}
              <p class="text-[13px] text-faint">No providers yet. Add one to start chatting.</p>
            {/each}
          </div>
        {/if}
      {:else if tab === 'responses'}
        <h3 class="text-[15px] font-semibold">Response style</h3>
        <p class="mb-5 text-[12.5px] text-faint">
          How Iris and your agents talk to you. Agents reporting back to Iris keep it plain.
        </p>
        <div class="grid max-w-2xl grid-cols-2 gap-2.5">
          {#each styles as st (st.id)}
            {@const on = style.preset === st.id}
            <button
              class="flex items-start gap-3 rounded-xl border px-3.5 py-3 text-left transition {on
                ? 'border-accent/60 bg-accent-soft'
                : 'border-line-strong hover:bg-hover'}"
              onclick={() => (style.preset = st.id)}
              role="radio"
              aria-checked={on}
            >
              <st.icon size={17} class="mt-0.5 shrink-0 {on ? 'text-accent-hi' : 'text-faint'}" />
              <span class="min-w-0">
                <span class="block text-[13px] font-medium">{st.label}</span>
                <span class="mt-0.5 block text-[12px] leading-snug text-faint">{st.description}</span>
              </span>
            </button>
          {/each}
        </div>
        <div class="mt-5 max-w-2xl">
          <label class="label" for="style-custom">Anything else they should know?</label>
          <textarea
            id="style-custom"
            class="input min-h-24 resize-y font-[inherit] leading-relaxed"
            maxlength="2000"
            bind:value={style.custom}
            placeholder="For example: call me Sam · I live in Lisbon, use °C and euros · answer in Portuguese when I write in Portuguese"
          ></textarea>
        </div>
        <div class="mt-4 flex max-w-2xl justify-end">
          <button
            class="btn-primary"
            onclick={saveStyle}
            disabled={savingStyle || (!!savedStyle && style.preset === savedStyle.preset && style.custom.trim() === savedStyle.custom)}
          >
            {#if savingStyle}<LoaderCircle size={14} class="animate-spin" />{/if} Save style
          </button>
        </div>

        <h3 class="mt-8 text-[15px] font-semibold">What your agents know about you</h3>
        <p class="mb-3 text-[12.5px] text-faint">
          USER.md, shared by every agent and kept across chats. Agents add to it when they learn something worth
          remembering; you can edit it here. {userMemory.trim().length} / {userLimit} characters.
        </p>
        <textarea
          class="input min-h-32 max-w-2xl resize-y font-mono text-[12.5px] leading-relaxed"
          maxlength={userLimit}
          bind:value={userMemory}
          placeholder="- Lives in Lisbon, Portugal; prefers °C and euros"
        ></textarea>
        <div class="mt-3 flex max-w-2xl justify-end">
          <button class="btn-primary" onclick={saveUserMemory} disabled={userMemory.trim() === savedUserMemory.trim()}>
            Save
          </button>
        </div>
      {:else if tab === 'skills'}
        <div class="mb-4 flex items-start justify-between gap-4">
          <div>
            <h3 class="text-[15px] font-semibold">Skills</h3>
            <p class="text-[12.5px] text-faint">
              How-tos the team saves after working something out, and loads the next time a similar task comes up.
              Shared by every agent. Stored as <code>skills/&lt;name&gt;/SKILL.md</code> on the server, so you can also
              copy skills in from elsewhere.
            </p>
          </div>
          <button class="btn-outline shrink-0 whitespace-nowrap" onclick={() => (skEdit = { original: null, name: '', description: '', body: '' })}>
            <Plus size={14} /> Add skill
          </button>
        </div>
        {#if skEdit}
          <div class="mb-5 space-y-3 rounded-xl border border-accent/30 bg-accent-soft/30 p-4">
            <div class="grid grid-cols-2 gap-3 max-sm:grid-cols-1">
              <div>
                <label class="label" for="sk-name">Name</label>
                <input id="sk-name" class="input" bind:value={skEdit.name} placeholder="e.g. deploy-website" />
              </div>
              <div>
                <label class="label" for="sk-desc">When to use it</label>
                <input id="sk-desc" class="input" bind:value={skEdit.description} maxlength="200" placeholder="e.g. When asked to publish the site" />
              </div>
            </div>
            <div>
              <label class="label" for="sk-body">Instructions</label>
              <textarea id="sk-body" class="input min-h-40 resize-y font-mono text-[12.5px] leading-relaxed" bind:value={skEdit.body} placeholder={'1. …\n2. …'}></textarea>
            </div>
            <div class="flex justify-end gap-2">
              <button class="btn-ghost" onclick={() => (skEdit = null)}>Cancel</button>
              <button class="btn-primary" onclick={saveSkill} disabled={!skEdit.name.trim() || !skEdit.description.trim() || !skEdit.body.trim()}>Save</button>
            </div>
          </div>
        {/if}
        <div class="space-y-2">
          {#each skills as s (s.name)}
            <div class="rounded-xl border border-line bg-surface">
              <div class="flex items-center gap-3 px-3.5 py-3">
                <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-hi"><BookOpen size={15} /></span>
                <button class="min-w-0 flex-1 text-left" onclick={() => (skOpen = skOpen === s.name ? null : s.name)} aria-expanded={skOpen === s.name}>
                  <div class="font-mono text-[13px] font-medium">{s.name}</div>
                  <div class="truncate text-[11.5px] text-faint">{s.description || 'no description'}</div>
                </button>
                <button
                  class="btn-ghost px-2!"
                  onclick={() => (skEdit = { original: s.name, name: s.name, description: s.description, body: s.body })}
                  aria-label="Edit"><Pencil size={14} /></button
                >
                {#if skDelete === s.name}
                  <button class="btn-danger" onclick={() => app.api?.deleteSkill(s.name).then(loadSkills).catch((e) => app.error(e)).finally(() => (skDelete = null))}>Delete</button>
                {:else}
                  <button class="btn-ghost px-2! hover:text-danger!" onclick={() => (skDelete = s.name)} aria-label="Delete"><Trash2 size={14} /></button>
                {/if}
              </div>
              {#if skOpen === s.name}
                <pre class="selectable mx-3.5 mb-3 max-h-72 overflow-auto rounded-lg bg-bg px-3 py-2.5 text-[12px] leading-relaxed whitespace-pre-wrap text-muted scroll-thin">{s.body}</pre>
              {/if}
            </div>
          {:else}
            <p class="text-[13px] text-faint">No skills yet. Agents save them on their own after tricky multi-step tasks, or add one here.</p>
          {/each}
        </div>
      {:else if tab === 'connections'}
        <div class="mb-4 flex items-start justify-between gap-4">
          <div>
            <h3 class="text-[15px] font-semibold">Connections</h3>
            <p class="text-[12.5px] text-faint">
              Services agents can use through MCP servers: GitHub, Gmail, a database, your notes… Iris can use them;
              other agents need the Connections switch. Tools that change things ask you first unless you trust them.
              Tokens stay on the server (<code>secrets/mcp.json</code>) and are never shown to agents.
            </p>
          </div>
          <div class="flex shrink-0 gap-2">
            <button class="btn-ghost px-2!" onclick={refreshMcp} disabled={mcpBusy} use:tip={'Reconnect all'} aria-label="Reconnect all">
              <RefreshCw size={14} class={mcpBusy ? 'animate-spin' : ''} />
            </button>
            <button
              class="btn-outline whitespace-nowrap"
              onclick={() => (mcpForm = { original: null, name: '', kind: 'command', commandLine: '', url: '', secrets: '', trustAll: false, autoApprove: [] })}
            >
              <Plus size={14} /> Connect
            </button>
          </div>
        </div>
        {#if mcpForm}
          <div class="mb-5 space-y-3 rounded-xl border border-accent/30 bg-accent-soft/30 p-4">
            <div class="grid grid-cols-2 gap-3 max-sm:grid-cols-1">
              <div>
                <label class="label" for="mcp-name">Name</label>
                <input id="mcp-name" class="input" bind:value={mcpForm.name} placeholder="e.g. github" maxlength="24" />
              </div>
              <div>
                <span class="label">Runs as</span>
                <div class="grid grid-cols-2 gap-2">
                  {#each [{ id: 'command', label: 'Program' }, { id: 'url', label: 'Web address' }] as const as k (k.id)}
                    <button
                      class="rounded-lg border px-3 py-2 text-[12.5px] transition {mcpForm.kind === k.id ? 'border-accent/60 bg-accent-soft' : 'border-line-strong hover:bg-hover'}"
                      onclick={() => mcpForm && (mcpForm.kind = k.id)}>{k.label}</button
                    >
                  {/each}
                </div>
              </div>
            </div>
            {#if mcpForm.kind === 'command'}
              <div>
                <label class="label" for="mcp-cmd">Command on the server</label>
                <input id="mcp-cmd" class="input font-mono text-[12.5px]" bind:value={mcpForm.commandLine} placeholder="npx -y @modelcontextprotocol/server-github" />
              </div>
            {:else}
              <div>
                <label class="label" for="mcp-url">Server URL</label>
                <input id="mcp-url" class="input font-mono text-[12.5px]" bind:value={mcpForm.url} placeholder="https://example.com/mcp" />
              </div>
            {/if}
            <div>
              <label class="label" for="mcp-secrets">{mcpForm.kind === 'command' ? 'Environment variables' : 'Headers'} (one KEY=value per line)</label>
              <textarea
                id="mcp-secrets"
                class="input min-h-16 resize-y font-mono text-[12.5px]"
                bind:value={mcpForm.secrets}
                placeholder={mcpForm.kind === 'command' ? 'GITHUB_PERSONAL_ACCESS_TOKEN=ghp_…' : 'Authorization=Bearer …'}
                autocomplete="off"
                spellcheck="false"
              ></textarea>
              {#if mcpForm.original}<p class="hint">Saved values aren't shown. Leave a line as KEY= to keep its value.</p>{/if}
            </div>
            <label class="flex items-center gap-2 text-[12.5px] text-muted">
              <input type="checkbox" bind:checked={mcpForm.trustAll} />
              Trust every tool of this server (never ask). Leave off unless you're sure.
            </label>
            <div class="flex justify-end gap-2">
              <button class="btn-ghost" onclick={() => (mcpForm = null)}>Cancel</button>
              <button
                class="btn-primary"
                onclick={saveMcp}
                disabled={mcpBusy || !/^[A-Za-z0-9_-]{1,24}$/.test(mcpForm.name.trim()) || (mcpForm.kind === 'command' ? !mcpForm.commandLine.trim() : !mcpForm.url.trim())}
              >
                {#if mcpBusy}<LoaderCircle size={14} class="animate-spin" /> Connecting…{:else}Save & connect{/if}
              </button>
            </div>
          </div>
        {/if}
        <div class="space-y-2">
          {#each mcpServers as s (s.name)}
            <div class="rounded-xl border border-line bg-surface px-3.5 py-3 {s.enabled ? '' : 'opacity-60'}">
              <div class="flex items-center gap-3">
                <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-hi"><Plug size={15} /></span>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2 text-[13.5px] font-medium">
                    {s.name}
                    <span class="h-2 w-2 rounded-full {s.connected ? 'bg-success' : s.enabled ? 'bg-danger' : 'bg-line-strong'}"></span>
                  </div>
                  <div class="truncate font-mono text-[11.5px] text-faint">{s.transport === 'stdio' ? [s.command, ...s.args].join(' ') : s.url}</div>
                </div>
                <button
                  class="flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 transition {s.enabled ? 'bg-accent' : 'bg-line-strong'}"
                  onclick={() => setMcpEnabled(s, !s.enabled)}
                  disabled={mcpBusy}
                  role="switch"
                  aria-checked={s.enabled}
                  aria-label="On"
                >
                  <span class="h-4 w-4 rounded-full bg-white transition {s.enabled ? 'translate-x-4' : ''}"></span>
                </button>
                <button class="btn-ghost px-2!" onclick={() => editMcp(s)} aria-label="Edit"><Pencil size={14} /></button>
                {#if mcpDelete === s.name}
                  <button class="btn-danger" onclick={() => app.api?.deleteMcp(s.name).then(loadMcp).catch((e) => app.error(e)).finally(() => (mcpDelete = null))}>Remove</button>
                {:else}
                  <button class="btn-ghost px-2! hover:text-danger!" onclick={() => (mcpDelete = s.name)} aria-label="Remove"><Trash2 size={14} /></button>
                {/if}
              </div>
              {#if s.error}
                <p class="mt-2 text-[12px] text-danger">{s.error}</p>
              {:else if s.tools.length}
                <div class="mt-2 flex flex-wrap gap-1.5">
                  {#each s.tools as t (t.name)}
                    <span
                      class="rounded-full px-2 py-0.5 font-mono text-[11px] {t.read_only || s.auto_approve.includes('*') || s.auto_approve.includes(t.name) ? 'bg-hover text-muted' : 'bg-warn/15 text-warn'}"
                      use:tip={`${t.description || t.name}${t.read_only ? ' · read-only' : s.auto_approve.includes('*') || s.auto_approve.includes(t.name) ? ' · trusted' : ' · asks you first'}`}
                      >{t.name}</span
                    >
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            <p class="text-[13px] text-faint">
              Nothing connected yet. Example: name <code>github</code>, command
              <code>npx -y @modelcontextprotocol/server-github</code>, and <code>GITHUB_PERSONAL_ACCESS_TOKEN=…</code>. Programs
              started with npx need Node.js on the server.
            </p>
          {/each}
        </div>
      {:else if tab === 'accounts'}
        <div class="mb-4 flex items-start justify-between gap-4">
          <div>
            <h3 class="text-[15px] font-semibold">Accounts</h3>
            <p class="text-[12.5px] text-faint">
              Logins agents can use in their browser. Stored encrypted on this computer. Agents only see the name and
              site; the browser fills the login itself, and only on that site. Paying or ordering always asks you.
            </p>
          </div>
          <button class="btn-outline shrink-0 whitespace-nowrap" onclick={() => (acEdit = { id: null, name: '', site: '', username: '', password: '' })}>
            <Plus size={14} /> Add account
          </button>
        </div>
        {#if acEdit}
          <div class="mb-5 space-y-3 rounded-xl border border-accent/30 bg-accent-soft/30 p-4">
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="label" for="ac-name">Name</label>
                <input id="ac-name" class="input" bind:value={acEdit.name} placeholder="e.g. My shop account" />
              </div>
              <div>
                <label class="label" for="ac-site">Website</label>
                <input id="ac-site" class="input" bind:value={acEdit.site} placeholder="e.g. example.com" />
              </div>
              <div>
                <label class="label" for="ac-user">Username or email</label>
                <input id="ac-user" class="input" bind:value={acEdit.username} autocomplete="off" />
              </div>
              <div>
                <label class="label" for="ac-pass">Password</label>
                <input
                  id="ac-pass"
                  class="input"
                  type="password"
                  bind:value={acEdit.password}
                  autocomplete="new-password"
                  placeholder={acEdit.id ? 'Leave blank to keep it' : ''}
                />
              </div>
            </div>
            <p class="hint">Don't save card numbers here: keep cards on the website itself. Payments always need your OK.</p>
            <div class="flex justify-end gap-2">
              <button class="btn-ghost" onclick={() => (acEdit = null)}>Cancel</button>
              <button class="btn-primary" onclick={saveAccount} disabled={!acEdit.name.trim() || !acEdit.site.trim()}>Save</button>
            </div>
          </div>
        {/if}
        <div class="space-y-2">
          {#each accounts as a (a.id)}
            <div class="flex items-center gap-3 rounded-xl border border-line bg-surface px-3.5 py-3">
              <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-hi"><KeyRound size={15} /></span>
              <div class="min-w-0 flex-1">
                <div class="text-[13.5px] font-medium">{a.name}</div>
                <div class="truncate text-[11.5px] text-faint">{a.site} · {a.username || 'no username'} · {a.has_password ? 'password saved' : 'no password'}</div>
              </div>
              <button
                class="btn-ghost px-2!"
                onclick={() => (acEdit = { id: a.id, name: a.name, site: a.site, username: a.username, password: '' })}
                aria-label="Edit"><Pencil size={14} /></button
              >
              {#if acDelete === a.id}
                <button class="btn-danger" onclick={() => app.api?.deleteAccount(a.id).then(loadAccounts).finally(() => (acDelete = null))}>Delete</button>
              {:else}
                <button class="btn-ghost px-2! hover:text-danger!" onclick={() => (acDelete = a.id)} aria-label="Delete"><Trash2 size={14} /></button>
              {/if}
            </div>
          {:else}
            <p class="text-[13px] text-faint">No accounts yet. Add one, then ask an agent e.g. “log in to my shop account and show my last order”.</p>
          {/each}
        </div>
      {:else if tab === 'routines'}
        <div class="mb-5 flex items-start justify-between gap-6">
          <div>
            <h3 class="text-[15px] font-semibold">Routines</h3>
            <p class="mt-0.5 max-w-md text-[12.5px] leading-relaxed text-faint">
              Things agents do on a schedule. A <b class="font-medium text-muted">task</b> wakes the agent; a
              <b class="font-medium text-muted">command</b> runs without the AI and only wakes it when something fails.
              Agents can create these too.
            </p>
          </div>
          {#if !rtForm}
            <button class="btn-outline shrink-0 whitespace-nowrap" onclick={newRoutine}><Plus size={14} /> New routine</button>
          {/if}
        </div>

        {#if rtForm}
          <div class="mb-6 space-y-4 rounded-2xl border border-accent/30 bg-accent-soft/25 p-5">
            <div class="text-[13.5px] font-semibold">{rtForm.id ? 'Edit routine' : 'New routine'}</div>
            <div class="grid grid-cols-2 gap-4">
              <div>
                <label class="label" for="rt-name">Name</label>
                <input id="rt-name" class="input" bind:value={rtForm.name} placeholder="Morning site check" />
              </div>
              <div>
                <label class="label" for="rt-agent">Agent</label>
                <Select id="rt-agent" bind:value={rtForm.agent_id} options={app.orderedAgents.map((a) => ({ value: a.id, label: a.name }))} />
              </div>
            </div>
            <div>
              <label class="label" for="rt-schedule">When</label>
              <input id="rt-schedule" class="input font-mono text-[13px]" bind:value={rtForm.schedule} />
              <div class="mt-2 flex flex-wrap gap-1.5">
                {#each rtExamples as ex}
                  <button
                    class="rounded-full border px-2.5 py-1 font-mono text-[11.5px] transition {rtForm.schedule === ex
                      ? 'border-accent/60 bg-accent-soft text-text'
                      : 'border-line-strong text-muted hover:text-text'}"
                    onclick={() => rtForm && (rtForm.schedule = ex)}>{ex}</button
                  >
                {/each}
              </div>
              <p class="hint">Also: <code>weekends 10:00</code>, <code>every 1d</code>, <code>once 2026-11-05 18:00</code>. Local time; at most every 5 minutes.</p>
            </div>
            <div>
              <span class="label">Type</span>
              <div class="grid grid-cols-3 gap-2 max-sm:grid-cols-1">
                {#each [{ id: 'prompt', label: 'Task for the agent', hint: 'The agent gets this as a message and does it.' }, { id: 'heartbeat', label: 'Heartbeat', hint: 'The agent checks a list; you only hear about it when something needs you.' }, { id: 'command', label: 'Command (no AI)', hint: 'PowerShell runs on its own; the agent only hears about failures.' }] as const as k (k.id)}
                  <button
                    class="rounded-xl border px-3.5 py-2.5 text-left transition {rtForm.kind === k.id
                      ? 'border-accent/60 bg-accent-soft'
                      : 'border-line-strong hover:bg-hover'}"
                    onclick={() => rtForm && (rtForm.kind = k.id)}
                  >
                    <span class="block text-[13px] font-medium">{k.label}</span>
                    <span class="block text-[11.5px] leading-snug text-faint">{k.hint}</span>
                  </button>
                {/each}
              </div>
            </div>
            <div>
              <label class="label" for="rt-body">{rtForm.kind === 'prompt' ? 'Task' : rtForm.kind === 'heartbeat' ? 'Checklist' : 'PowerShell command'}</label>
              <textarea
                id="rt-body"
                class="input min-h-24 resize-y leading-relaxed {rtForm.kind === 'command' ? 'font-mono text-[12.5px]' : ''}"
                bind:value={rtForm.body}
                placeholder={rtForm.kind === 'prompt'
                  ? 'Check example.com is up and fast and tell me only if something is wrong.'
                  : rtForm.kind === 'heartbeat'
                    ? '- Is example.com up?\n- Is the disk more than 90% full?\n- Any unfinished tasks from earlier?'
                    : '(Invoke-WebRequest https://example.com -UseBasicParsing -TimeoutSec 15).StatusCode'}
              ></textarea>
              {#if rtForm.kind === 'command'}
                <label class="mt-2.5 flex items-center gap-2 text-[12.5px] text-muted">
                  <input
                    type="checkbox"
                    checked={rtForm.notify === 'always'}
                    onchange={(e) => rtForm && (rtForm.notify = (e.currentTarget as HTMLInputElement).checked ? 'always' : 'failure')}
                  />
                  Report every run, not just failures
                </label>
              {/if}
            </div>
            <div class="flex justify-end gap-2 pt-1">
              <button class="btn-ghost" onclick={() => (rtForm = null)}>Cancel</button>
              <button class="btn-primary" onclick={saveRoutine} disabled={!rtForm.name.trim() || !rtForm.body.trim() || !rtForm.schedule.trim()}>
                {rtForm.id ? 'Save changes' : 'Schedule'}
              </button>
            </div>
          </div>
        {/if}

        <div class="space-y-3">
          {#each routines as r (r.id)}
            {@const agent = app.agent(r.agent_id)}
            <div class="rounded-2xl border border-line bg-surface px-4 py-3.5 transition {r.enabled ? '' : 'opacity-60'}">
              <div class="flex items-start gap-3">
                <AgentAvatar {agent} size={30} />
                <div class="min-w-0 flex-1">
                  <div class="text-[14px] leading-snug font-medium">{r.name}</div>
                  <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11.5px]">
                    <span class="rounded-full bg-hover px-2 py-0.5 font-mono text-muted">{r.schedule}</span>
                    <span class="rounded-full px-2 py-0.5 {r.kind === 'command' ? 'bg-accent-soft text-accent-hi' : 'bg-hover text-muted'}">
                      {r.kind === 'command' ? 'Command' : r.kind === 'heartbeat' ? 'Heartbeat' : 'Task'}
                    </span>
                    <span class="text-faint">· {agent?.name ?? '?'}</span>
                  </div>
                </div>
                <button
                  class="mt-1 flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 transition {r.enabled ? 'bg-accent' : 'bg-line-strong'}"
                  onclick={() => app.api?.setRoutineEnabled(r.id, !r.enabled).then(loadRoutines).catch((e) => app.error(e))}
                  role="switch"
                  aria-checked={r.enabled}
                  aria-label={r.enabled ? 'Pause' : 'Turn on'}
                  use:tip={r.enabled ? 'On · click to pause' : 'Paused · click to turn on'}
                >
                  <span class="h-4 w-4 rounded-full bg-white transition {r.enabled ? 'translate-x-4' : ''}"></span>
                </button>
              </div>
              <div class="mt-2.5 ml-[42px] flex items-center gap-3 border-t border-line pt-2.5">
                <div class="min-w-0 flex-1 text-[11.5px] leading-relaxed text-faint">
                  <div>Next: <span class="text-muted">{r.enabled ? when(r.next_run) : 'paused'}</span></div>
                  {#if r.last_run}
                    <div class="truncate" title={r.last_result}>
                      Last: <span class={r.last_result.startsWith('failed') ? 'text-danger' : 'text-muted'}>{when(r.last_run)} · {r.last_result.startsWith('ok') ? 'ok' : r.last_result}</span>
                    </div>
                  {/if}
                </div>
                <button class="btn-ghost px-2.5! text-[12px]!" onclick={() => app.api?.runRoutine(r.id).then(() => app.toast(`Running “${r.name}”`))}>
                  <Play size={13} /> Run now
                </button>
                <button class="btn-ghost px-2.5! text-[12px]!" onclick={() => editRoutine(r)}><Pencil size={13} /> Edit</button>
                {#if rtDelete === r.id}
                  <button class="btn-danger text-[12px]!" onclick={() => app.api?.deleteRoutine(r.id).then(loadRoutines).finally(() => (rtDelete = null))}>Delete</button>
                {:else}
                  <button class="btn-ghost px-2! hover:text-danger!" onclick={() => (rtDelete = r.id)} aria-label="Delete" use:tip={'Delete'}><Trash2 size={14} /></button>
                {/if}
              </div>
            </div>
          {:else}
            {#if !rtForm}
              <p class="text-[13px] text-faint">No routines yet. Add one here, or ask an agent: “check my site every morning at 8”.</p>
            {/if}
          {/each}
        </div>
      {:else if tab === 'limits'}
        <h3 class="text-[15px] font-semibold">Token limits</h3>
        <p class="mb-5 text-[12.5px] text-faint">
          Caps for all agents together. When one is reached, agents stop until it resets (days at midnight, weeks
          on Monday) or you raise it.
        </p>
        {#each [
          { label: 'Daily limit', key: 'daily', presets: dailyPresets, used: limits?.used_today ?? 0, period: 'today' },
          { label: 'Weekly limit', key: 'weekly', presets: weeklyPresets, used: limits?.used_this_week ?? 0, period: 'this week' },
        ] as row (row.key)}
          {@const value = row.key === 'daily' ? daily : weekly}
          <div class="mb-5 max-w-xl rounded-xl border border-line bg-surface/60 p-4">
            <div class="mb-3.5">
              <UsageBar label={row.label} used={row.used} limit={value || null} hint={`Used ${row.period}`} />
            </div>
            <div class="flex flex-wrap gap-1.5">
              {#each row.presets as p}
                <button
                  class="rounded-full border px-3 py-1 text-[12px] transition {value === p
                    ? 'border-accent/70 bg-accent-soft text-text'
                    : 'border-line-strong text-muted hover:text-text'}"
                  onclick={() => (row.key === 'daily' ? (daily = p) : (weekly = p))}>{p ? tokens(p) : 'Off'}</button
                >
              {/each}
              <input
                type="number"
                min="0"
                step="10000"
                class="input w-32! py-1! text-[12px]"
                value={value || ''}
                placeholder="Custom"
                oninput={(e) => {
                  const n = Number((e.target as HTMLInputElement).value) || 0;
                  if (row.key === 'daily') daily = n;
                  else weekly = n;
                }}
              />
            </div>
          </div>
        {/each}
        <div class="flex max-w-xl justify-end">
          <button
            class="btn-primary"
            onclick={saveLimits}
            disabled={savingLimits || (!!limits && daily === limits.daily_tokens && weekly === limits.weekly_tokens)}
          >
            {#if savingLimits}<LoaderCircle size={14} class="animate-spin" />{/if} Save limits
          </button>
        </div>
      {:else if tab === 'search'}
        <h3 class="text-[15px] font-semibold">Web search</h3>
        <p class="mb-5 text-[12.5px] text-faint">Used by agents with the Web search tool.</p>
        <div class="max-w-xl space-y-2">
          {#each [
            { id: 'auto', title: 'Automatic', text: 'Tavily when a key is saved, otherwise DuckDuckGo.' },
            { id: 'duckduckgo', title: 'DuckDuckGo', text: 'Free, no key. Works, but can rate-limit busy agents.' },
            { id: 'tavily', title: 'Tavily', text: 'Built for AI agents. Free plan: 1,000 searches a month, no card.' },
            {
              id: 'deepseek',
              title: 'DeepSeek',
              text: search?.has_deepseek_key === false
                ? 'Needs a DeepSeek provider with a key (Settings → Providers).'
                : 'DeepSeek’s own search engine, with your DeepSeek key. About $0.75 per 1,000 searches from your DeepSeek balance; pages other readers can’t open are read through DeepSeek too.',
            },
          ] as const as o (o.id)}
            <label
              class="flex cursor-pointer items-start gap-3 rounded-xl border px-4 py-3 transition {searchProvider === o.id
                ? 'border-accent/60 bg-accent-soft'
                : 'border-line-strong hover:bg-hover'}"
            >
              <input type="radio" class="mt-1 accent-[var(--c-accent)]" value={o.id} bind:group={searchProvider} />
              <span>
                <span class="block text-[13.5px] font-medium">{o.title}</span>
                <span class="block text-[12.5px] text-faint">{o.text}</span>
              </span>
            </label>
          {/each}
        </div>
        <div class="mt-5 max-w-xl">
          <label class="label" for="tavily-key">Tavily API key</label>
          <input
            id="tavily-key"
            class="input font-mono text-[13px]"
            type="password"
            autocomplete="off"
            bind:value={tavilyKey}
            placeholder={search?.has_tavily_key ? 'Key saved. Leave blank to keep it.' : 'tvly-…'}
          />
          <p class="hint">Get a free key at tavily.com. It's stored only on this machine.</p>
        </div>
        <div class="mt-5 flex max-w-xl items-center gap-2">
          {#if search}
            <span class="text-[12.5px] text-muted">
              Currently using <b class="text-text">{searchName(search.active)}</b>
            </span>
          {/if}
          <div class="ml-auto flex gap-2">
            {#if search?.has_tavily_key}
              <button class="btn-danger" onclick={() => saveSearch(true)} disabled={savingSearch}>Remove key</button>
            {/if}
            <button class="btn-primary" onclick={() => saveSearch()} disabled={savingSearch}>
              {#if savingSearch}<LoaderCircle size={14} class="animate-spin" />{/if} Save
            </button>
          </div>
        </div>
      {:else if tab === 'activity'}
        <div class="mb-4 flex items-center justify-between">
          <div>
            <h3 class="text-[15px] font-semibold">Activity</h3>
            <p class="text-[12.5px] text-faint">Every tool your agents used, newest first.</p>
          </div>
          <button class="btn-ghost" onclick={loadAudit} aria-label="Refresh">
            <RefreshCw size={14} class={loadingAudit ? 'animate-spin' : ''} />
          </button>
        </div>
        {#if audit.length === 0}
          <p class="text-[13px] text-faint">{loadingAudit ? 'Loading…' : 'No tool activity yet.'}</p>
        {:else}
          <div class="max-h-[420px] overflow-y-auto rounded-xl border border-line scroll-thin">
            {#each audit as e, i (e.id)}
              {@const agent = app.agent(e.agent_id)}
              <button
                class="flex w-full items-center gap-3 px-3.5 py-2.5 text-left text-[12.5px] transition hover:bg-hover/60 {i > 0
                  ? 'border-t border-line'
                  : ''}"
                onclick={() => {
                  if (agent) {
                    app.modal = null;
                    app.openThread(e.agent_id, e.thread_id);
                  }
                }}
              >
                <AgentAvatar {agent} size={22} />
                <span class="min-w-0 flex-1 truncate text-muted">{e.summary}</span>
                <span class="shrink-0 rounded-full px-2 py-0.5 text-[11px] {chipFor(e).style}">{chipFor(e).label}</span>
                <span class="w-12 shrink-0 text-right text-[11px] text-faint">{(e.duration_ms / 1000).toFixed(1)}s</span>
                <span class="w-16 shrink-0 text-right text-[11px] text-faint">{clockTime(e.started_at)}</span>
              </button>
            {/each}
          </div>
        {/if}
      {:else if tab === 'devices'}
        <h3 class="text-[15px] font-semibold">Paired devices</h3>
        <p class="mb-4 text-[12.5px] text-faint">Every app or tool that talks to I.R.I.S. has its own access token. Revoke any you don't recognize.</p>
        <div class="space-y-2">
          {#each devices as d (d.id)}
            <div class="flex items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3">
              <Smartphone size={16} class="text-faint" />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2 text-[13.5px] font-medium">
                  {d.name}
                  {#if d.local}<span class="rounded-full bg-accent-soft px-2 py-0.5 text-[11px] text-accent-hi">This app</span>{/if}
                </div>
                <div class="text-[12px] text-faint">
                  Added {relativeTime(d.created_at)} · {d.last_seen_at ? `last seen ${relativeTime(d.last_seen_at)}` : 'never used'}
                </div>
              </div>
              {#if !d.local}
                <button class="btn-danger" onclick={() => revoke(d)}>Revoke</button>
              {/if}
            </div>
          {/each}
        </div>
        <div class="mt-5 rounded-xl border border-line p-4">
          <div class="label">Create an access token</div>
          <div class="flex gap-2">
            <input class="input" bind:value={newDeviceName} placeholder="Device name, e.g. My phone" />
            <button class="btn-outline shrink-0" onclick={createDevice} disabled={!newDeviceName.trim()}>Create</button>
          </div>
          {#if newToken}
            <div class="mt-3 rounded-lg bg-code p-3">
              <div class="mb-1.5 text-[12px] text-muted">Token for <b>{newToken.name}</b>. Copy it now; it won't be shown again.</div>
              <div class="flex items-center gap-2">
                <code class="selectable flex-1 truncate font-mono text-[12px]">{newToken.token}</code>
                <button
                  class="btn-ghost px-2!"
                  onclick={async () => {
                    await navigator.clipboard.writeText(newToken!.token);
                    tokenCopied = true;
                    setTimeout(() => (tokenCopied = false), 1500);
                  }}
                >
                  {#if tokenCopied}<Check size={14} />{:else}<Copy size={14} />{/if}
                </button>
              </div>
            </div>
          {/if}
        </div>
      {:else if tab === 'remote'}
        <div class="mb-1 flex items-center gap-2">
          <h3 class="text-[15px] font-semibold">Remote access</h3>
          {#if link?.enabled}
            <span
              class="rounded-full px-2 py-0.5 text-[11px] {app.linkConnected
                ? 'bg-emerald-500/15 text-emerald-400'
                : 'bg-amber-500/15 text-amber-400'}"
            >
              {app.linkConnected ? 'Connected' : 'Connecting…'}
            </span>
          {:else}
            <span class="rounded-full bg-elevated px-2 py-0.5 text-[11px] text-faint">Off</span>
          {/if}
        </div>
        {#if app.api?.link}
          <div class="mb-4 flex items-center gap-2 rounded-xl border border-line bg-surface px-4 py-3 text-[12.5px]">
            <Globe size={14} class="text-accent-hi" />
            <span class="min-w-0 flex-1">This browser is signed in as <b>{app.deviceName}</b>.</span>
            <button class="btn-outline shrink-0" onclick={() => app.signOut()}>Sign out</button>
          </div>
        {/if}
        <p class="mb-5 text-[12.5px] text-faint">
          Use I.R.I.S. from your phone or any browser, from anywhere. This computer connects out to your relay, so nothing is opened on your network, and everything is end-to-end encrypted: the relay only passes sealed messages along.
        </p>

        <div class="mb-6 rounded-xl border border-line p-4">
          <label class="label" for="link-url">Relay address</label>
          <div class="flex gap-2">
            <input id="link-url" class="input font-mono text-[13px]" bind:value={linkUrl} placeholder="https://iris.your-name.workers.dev" />
            {#if link?.enabled}
              {#if linkUrl.trim() !== link.url}
                <button class="btn-primary shrink-0" disabled={linkSaving} onclick={() => saveLink(true)}>Save</button>
              {/if}
              <button class="btn-outline shrink-0" disabled={linkSaving} onclick={() => saveLink(false)}>Turn off</button>
            {:else}
              <button class="btn-primary shrink-0" disabled={linkSaving || !linkUrl.trim()} onclick={() => saveLink(true)}>
                {#if linkSaving}<LoaderCircle size={14} class="animate-spin" />{/if} Turn on
              </button>
            {/if}
          </div>
          {#if link && !link.has_relay_key}
            <p class="mt-2 text-[12px] text-amber-400">The relay's server key is missing on this computer (secrets\relay.key), so it can't connect yet.</p>
          {/if}
        </div>

        <div class="mb-6 rounded-xl border border-line p-4">
          <div class="text-[13.5px] font-medium">Pair a phone</div>
          <p class="mb-3 text-[12.5px] text-faint">Scan the code with the I.R.I.S. phone app, or open the link on the device. Each code works once, for 10 minutes.</p>
          {#if pairing}
            <div class="flex flex-wrap items-start gap-4">
              <div class="w-40 shrink-0 overflow-hidden rounded-lg bg-white p-1 [&>svg]:h-auto [&>svg]:w-full">{@html pairQr}</div>
              <div class="min-w-0 flex-1 space-y-2 text-[12.5px]">
                <div>Code <b class="selectable font-mono text-[15px] tracking-widest">{pairing.code}</b></div>
                <div class="text-faint">Expires at {clockTime(pairing.expires_at)}</div>
                <div class="flex gap-2">
                  <button class="btn-outline" onclick={() => navigator.clipboard.writeText(pairing!.url)}><Copy size={13} /> Copy link</button>
                  <button class="btn-ghost" onclick={makePairing}><RefreshCw size={13} /> New code</button>
                </div>
              </div>
            </div>
          {:else}
            {#if pairedName}
              <div class="mb-3 flex items-center gap-2 text-[12.5px] text-emerald-400"><Check size={14} /> {pairedName} is paired.</div>
            {/if}
            <button class="btn-outline" disabled={!link?.url} onclick={makePairing}>Show a pairing code</button>
            {#if !link?.url}<span class="ml-2 text-[12px] text-faint">Set the relay address first.</span>{/if}
          {/if}
        </div>

        <div class="rounded-xl border border-line p-4">
          <div class="text-[13.5px] font-medium">Sign in from a browser</div>
          <p class="mb-3 text-[12.5px] text-faint">
            For computers where you can't scan a code: open the relay address and sign in with this username and password. Five wrong tries lock it for 15 minutes.
          </p>
          {#if link?.login_user}
            <div class="mb-3 flex items-center gap-2 text-[12.5px]">
              <Check size={14} class="text-emerald-400" /> Password set for <b>{link.login_user}</b>
              <button class="btn-ghost ml-auto text-[12px]" onclick={removeLogin}>Remove</button>
            </div>
          {/if}
          <div class="flex flex-wrap gap-2">
            <input class="input min-w-36 flex-1" bind:value={loginUser} placeholder="Username" autocomplete="off" />
            <input
              class="input min-w-36 flex-1"
              type="password"
              bind:value={loginPass}
              placeholder={link?.login_user ? 'New password' : 'Password (10+ characters)'}
              autocomplete="new-password"
            />
            <button class="btn-outline shrink-0" disabled={loginSaving || !loginUser.trim() || loginPass.length < 10} onclick={saveLogin}>
              {link?.login_user ? 'Change' : 'Set'}
            </button>
          </div>
        </div>
      {:else if tab === 'voice'}
        <VoiceSettings />
      {:else if tab === 'appearance'}
        <h3 class="mb-4 text-[15px] font-semibold">Theme</h3>
        <div class="grid max-w-md grid-cols-2 gap-3">
          {#each [{ id: 'dark', label: 'Dark', icon: Moon }, { id: 'light', label: 'Light', icon: Sun }] as const as t (t.id)}
            <button
              class="flex items-center gap-3 rounded-xl border px-4 py-3.5 text-[13.5px] transition {app.theme === t.id
                ? 'border-accent/70 bg-accent-soft'
                : 'border-line-strong hover:bg-hover'}"
              onclick={() => app.setTheme(t.id)}
            >
              <t.icon size={16} />
              {t.label}
            </button>
          {/each}
        </div>
        <h3 class="mt-7 mb-1 text-[15px] font-semibold">Animations</h3>
        <p class="mb-4 text-[12.5px] text-faint">
          The orbs' moods, hops and working rings. Remote-desktop sessions often tell apps to reduce motion on their
          own, so "Always" ignores that.
        </p>
        <div class="grid max-w-md grid-cols-3 gap-3">
          {#each [{ id: 'always', label: 'Always' }, { id: 'system', label: 'Follow Windows' }, { id: 'off', label: 'Off' }] as const as m (m.id)}
            <button
              class="rounded-xl border px-3 py-3 text-[13px] transition {app.motion === m.id
                ? 'border-accent/70 bg-accent-soft'
                : 'border-line-strong hover:bg-hover'}"
              onclick={() => app.setMotion(m.id)}>{m.label}</button
            >
          {/each}
        </div>
      {:else if tab === 'about'}
        <div class="flex items-center gap-4">
          <img src="/iris.png" alt="" class="h-14 w-14 rounded-2xl" />
          <div>
            <div class="text-[18px] font-semibold tracking-tight">I.R.I.S.</div>
            <div class="text-[13px] text-muted">Intelligent Reasoning & Initiative System</div>
          </div>
        </div>
        <dl class="mt-6 grid max-w-md grid-cols-[140px_1fr] gap-y-2 text-[13px]">
          <dt class="text-faint">Engine version</dt>
          <dd>{app.engineVersion || app.update?.current || '—'}</dd>
          <dt class="text-faint">Engine address</dt>
          <dd class="font-mono text-[12px]">{app.api?.base}</dd>
          <dt class="text-faint">Status</dt>
          <dd class="capitalize">{app.conn}</dd>
          <dt class="text-faint">License</dt>
          <dd>MIT</dd>
        </dl>

        <!-- Updates: the engine looks on GitHub for a newer version; one button installs it -->
        <h3 class="mt-8 text-[13px] font-semibold">Updates</h3>
        {#if app.update}
          {@const u = app.update}
          <div class="mt-2 max-w-md space-y-3 text-[13px]">
            {#if u.available && u.latest}
              <p>
                <span class="font-medium">I.R.I.S. {u.latest.version}</span> is available (you have {u.current}).
              </p>
              {#if u.latest.notes}
                <p class="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-lg bg-surface p-3 text-[12px] text-muted">{u.latest.notes}</p>
              {/if}
              {#if u.latest.windows && u.latest.windows.sha256}
                <button
                  class="btn-primary"
                  disabled={u.install.phase === 'downloading' || u.install.phase === 'installing'}
                  onclick={() => app.installUpdate()}
                >
                  {u.install.phase === 'downloading'
                    ? `Downloading… ${u.install.percent}%`
                    : u.install.phase === 'installing'
                      ? 'Installing…'
                      : 'Update now'}
                </button>
                <p class="text-[12px] text-faint">
                  The installer is downloaded from GitHub and checked against its checksum before it runs. I.R.I.S.
                  closes and opens again by itself; your chats, agents and settings stay.
                </p>
              {:else}
                <p class="text-[12px] text-faint">
                  This release can't be installed from here (no checked installer for Windows).
                  <a class="underline" href={u.latest.page} target="_blank" rel="noreferrer">Open the release page</a>.
                </p>
              {/if}
            {:else if u.error}
              <p class="text-faint">Couldn't check for updates: {u.error}</p>
            {:else}
              <p>You have the newest version ({u.current}).</p>
            {/if}
            {#if u.install.phase === 'failed'}
              <p class="text-danger">The last update didn't work: {u.install.error}</p>
            {/if}
            {#if u.last_update && !u.last_update.ok}
              <p class="text-danger">The last update didn't finish (see the log below).</p>
              <pre class="max-h-32 overflow-auto rounded-lg bg-surface p-2 text-[11px] text-muted">{u.last_update.log}</pre>
            {/if}
            <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
              <button class="btn-outline" onclick={() => app.checkUpdate(true)}>Check now</button>
              <label class="flex items-center gap-2 text-[12.5px] text-muted">
                <input
                  type="checkbox"
                  checked={u.auto_check}
                  onchange={async (e) => {
                    try {
                      app.update = await app.api!.setUpdateAutoCheck((e.currentTarget as HTMLInputElement).checked);
                    } catch (err) {
                      app.error(err);
                    }
                  }}
                />
                Look for updates by itself (asks GitHub every few hours)
              </label>
            </div>
          </div>
        {:else}
          <p class="mt-2 text-[13px] text-faint">Update information isn't available from this engine yet.</p>
        {/if}
      {/if}
    </div>
  </div>
</Modal>
