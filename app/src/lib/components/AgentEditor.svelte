<script lang="ts">
  import { Cpu, LoaderCircle, ShieldAlert } from '@lucide/svelte';
  import { untrack } from 'svelte';
  import { app } from '../app.svelte';
  import { ABILITY_GROUPS, BUILT_IN_ABILITIES } from '../types';
  import { colorChoices, nextColor, templates, type Template } from '../templates';
  import AgentAvatar from './AgentAvatar.svelte';
  import Modal from './Modal.svelte';
  import Select from './Select.svelte';

  let { agentId }: { agentId: string | null } = $props();

  // The editor is opened fresh for each agent, so the initial values are all we
  // need. Capture the id too: the prop can't be read once the modal closes.
  const editId = untrack(() => agentId);
  const existing = editId ? app.agent(editId) : undefined;
  const freeColor = nextColor(app.agents.map((a) => a.color));

  let name = $state(existing?.name ?? '');
  let description = $state(existing?.description ?? '');
  let color = $state(existing?.color || freeColor);
  let systemPrompt = $state(existing?.system_prompt ?? '');
  // Only the switchable abilities matter here (the built-in ones are always on).
  let tools = $state<string[]>(existing?.tools ?? []);
  let template = $state('blank');
  // Empty provider: follow the default model from Settings.
  let provider = $state(existing?.provider ?? '');
  let model = $state(existing?.model ?? '');
  let models = $state<string[]>([]);

  async function loadModels() {
    if (!app.api || !provider) return;
    try {
      models = await app.api.models(provider);
      if (!models.includes(model) && models.length) {
        const preset = app.providers.find((p) => p.id === provider)?.default_model ?? '';
        model = models.includes(preset) ? preset : models[0];
      }
    } catch {
      models = [];
      if (!model) model = app.providers.find((p) => p.id === provider)?.default_model ?? '';
    }
  }
  if (existing?.provider) void loadModels();

  // Long-term memory (MEMORY.md), loaded for existing agents.
  let memory = $state('');
  let savedMemory = '';
  let memoryLimit = $state(3000);
  if (editId && app.api) {
    void app.api.agentMemory(editId).then((m) => {
      memory = savedMemory = m.text;
      memoryLimit = m.limit;
    }).catch(() => {});
  }

  let saving = $state(false);
  let confirmDelete = $state(false);

  function toggleGroup(ids: readonly string[]) {
    const on = ids.every((t) => tools.includes(t));
    tools = on ? tools.filter((t) => !ids.includes(t)) : [...tools, ...ids.filter((t) => !tools.includes(t))];
  }

  function toggleTool(id: string) {
    tools = tools.includes(id) ? tools.filter((t) => t !== id) : [...tools, id];
  }

  function applyTemplate(t: Template) {
    template = t.key;
    name = t.name;
    description = t.description;
    systemPrompt = t.system_prompt;
    tools = [...t.tools];
  }

  async function save() {
    if (!name.trim()) return;
    saving = true;
    const saved = await app.saveAgent(editId, {
      name: name.trim(),
      description: description.trim(),
      avatar: '',
      color,
      system_prompt: systemPrompt,
      provider: provider && model.trim() ? provider : '',
      model: provider ? model.trim() : '',
      tools,
    });
    if (saved && editId && memory.trim() !== savedMemory.trim()) {
      try {
        await app.api?.putAgentMemory(editId, memory);
      } catch (e) {
        app.error(e);
      }
    }
    saving = false;
    if (saved) {
      app.modal = null;
      if (!editId) app.newChat(saved.id);
      app.toast(editId ? `${saved.name} updated` : `${saved.name} is ready`);
    }
  }

  async function remove() {
    if (!editId) return;
    await app.deleteAgent(editId);
    app.modal = null;
  }
</script>

<Modal title={existing ? `Edit ${existing.name}` : 'New agent'} onclose={() => (app.modal = null)} width={600}>
  <div class="space-y-6 px-6 py-5">
    {#if !existing}
      <div>
        <span class="label">Start from</span>
        <div class="flex flex-wrap gap-2">
          {#each templates as t (t.key)}
            <button
              class="rounded-full border px-3 py-1.5 text-[12.5px] transition {template === t.key
                ? 'border-accent/70 bg-accent-soft text-text'
                : 'border-line-strong text-muted hover:text-text'}"
              onclick={() => applyTemplate(t)}>{t.label}</button
            >
          {/each}
        </div>
      </div>
    {/if}

    <!-- Identity -->
    <div class="flex gap-5">
      <div class="flex flex-col items-center gap-3 pt-5">
        <AgentAvatar agent={{ name: name || 'new', color }} size={72} />
      </div>
      <div class="flex-1 space-y-4">
        <div>
          <label class="label" for="agent-name">Name</label>
          <input id="agent-name" class="input" bind:value={name} placeholder="e.g. Chief of Staff" />
        </div>
        <div>
          <label class="label" for="agent-desc">Description</label>
          <input id="agent-desc" class="input" bind:value={description} placeholder="What is this agent for?" />
        </div>
        <div>
          <span class="label">Color</span>
          <div class="flex flex-wrap gap-2">
            {#each colorChoices as c}
              <button
                class="h-6 w-6 rounded-full transition hover:scale-110 {color === c
                  ? 'ring-2 ring-offset-2 ring-offset-[var(--c-panel)]'
                  : ''}"
                style:background={c}
                style:--tw-ring-color={c}
                onclick={() => (color = c)}
                aria-label="Color {c}"
              ></button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    <!-- Model: the default, or one of its own -->
    <div class="rounded-xl border border-line bg-surface/60 px-4 py-3">
      <div class="flex items-center gap-2">
        <Cpu size={16} class="shrink-0 text-accent-hi" />
        <span class="text-[13px] font-medium">Model</span>
      </div>
      <div class="mt-2.5 flex gap-2">
        <Select
          class="w-56! shrink-0"
          label="Provider"
          bind:value={provider}
          options={[{ value: '', label: `Default · ${app.modelLabel()}` }, ...app.providers.map((p) => ({ value: p.id, label: p.name }))]}
          onchange={() => {
            model = '';
            models = [];
            void loadModels();
          }}
        />
        {#if provider}
          {#if models.length}
            <div class="min-w-0 flex-1"><Select mono label="Model" bind:value={model} options={models.map((m) => ({ value: m, label: m }))} /></div>
          {:else}
            <input class="input min-w-0 flex-1 font-mono text-[13px]" bind:value={model} placeholder="model id" aria-label="Model" />
          {/if}
        {/if}
      </div>
      <p class="hint">
        {provider ? 'This agent always uses this model.' : 'Follows the default model from Settings → Models.'}
      </p>
    </div>

    <!-- Abilities: most are built in; two can be switched on -->
    <div>
      <span class="label">Abilities</span>
      {#if existing?.role === 'main'}
        <p class="rounded-xl border border-accent/30 bg-accent-soft/40 px-3 py-2.5 text-[12.5px] text-muted">
          Iris has every ability: {BUILT_IN_ABILITIES}, plus files and computer control. Risky steps still ask you.
        </p>
      {:else}
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {#each ABILITY_GROUPS as g (g.id)}
            {@const on = g.tools.every((t) => tools.includes(t))}
            <button
              class="flex items-start gap-3 rounded-xl border px-3 py-2.5 text-left transition {on
                ? 'border-accent/55 bg-accent-soft'
                : 'border-line-strong hover:bg-hover'}"
              onclick={() => toggleGroup(g.tools)}
              role="switch"
              aria-checked={on}
            >
              <span
                class="mt-0.5 flex h-4 w-7 shrink-0 items-center rounded-full p-0.5 transition {on ? 'bg-accent' : 'bg-line-strong'}"
              >
                <span class="h-3 w-3 rounded-full bg-white transition {on ? 'translate-x-3' : ''}"></span>
              </span>
              <span class="min-w-0">
                <span class="flex items-center gap-1.5 text-[13px] font-medium">
                  {g.name}
                  {#if g.asks}
                    <span class="flex items-center gap-0.5 rounded-full bg-warn/12 px-1.5 py-px text-[10.5px] font-medium text-warn">
                      <ShieldAlert size={10} /> Asks first
                    </span>
                  {/if}
                </span>
                <span class="mt-0.5 block text-[12px] leading-snug text-faint">{g.description}</span>
              </span>
            </button>
          {/each}
        </div>
        <p class="hint">
          Always on for every agent: {BUILT_IN_ABILITIES}. If it needs more, it asks Iris, who grants what fits.
        </p>
      {/if}
    </div>

    <!-- Personality -->
    <div>
      <label class="label" for="agent-prompt">Personality & instructions <span class="font-normal text-faint">· SOUL.md</span></label>
      <textarea
        id="agent-prompt"
        class="input min-h-32 resize-y font-[inherit] leading-relaxed"
        bind:value={systemPrompt}
        placeholder="Describe who this agent is, how it should behave and what it should focus on."
      ></textarea>
    </div>

    {#if existing}
      <div>
        <label class="label" for="agent-memory">
          Long-term memory <span class="font-normal text-faint">· MEMORY.md · {memory.trim().length} / {memoryLimit}</span>
        </label>
        <textarea
          id="agent-memory"
          class="input min-h-24 resize-y font-mono text-[12.5px] leading-relaxed"
          maxlength={memoryLimit}
          bind:value={memory}
          placeholder="Notes {existing.name} keeps across chats and compactions. It adds to them itself; you can edit them here."
        ></textarea>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    {#if existing?.role === 'main'}
      <span class="text-[12.5px] text-faint">The main agent (your Chief of Staff) stays on the team.</span>
    {:else if existing}
      {#if confirmDelete}
        <span class="text-[13px] text-danger">Delete {existing.name} and all its conversations?</span>
        <button class="btn-danger" onclick={remove}>Delete</button>
        <button class="btn-ghost" onclick={() => (confirmDelete = false)}>Keep</button>
      {:else}
        <button class="btn-danger" onclick={() => (confirmDelete = true)}>Delete agent</button>
      {/if}
    {/if}
    <div class="ml-auto flex gap-2">
      <button class="btn-ghost" onclick={() => (app.modal = null)}>Cancel</button>
      <button class="btn-primary" onclick={save} disabled={saving || !name.trim()}>
        {#if saving}<LoaderCircle size={14} class="animate-spin" />{/if}
        {existing ? 'Save changes' : 'Create agent'}
      </button>
    </div>
  {/snippet}
</Modal>
