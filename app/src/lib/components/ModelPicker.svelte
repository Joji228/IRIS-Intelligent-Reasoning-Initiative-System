<script lang="ts">
  // One button on the chat box for the model and how hard it thinks. The
  // choices change the app-wide default (the same settings as Settings →
  // Providers), so they stick everywhere. An agent with a model or level of
  // its own (set in its editor) changes just that agent instead.
  import { Brain, Check, ChevronDown, Cpu } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { modelVersion } from '../format';
  import { tip } from '../tooltip';
  import { REASONING_LEVELS, reasoningLabel, type Agent } from '../types';

  // `below`: the menu opens downward (the picker sits in the chat header).
  let { agent, disabled = false, below = false }: { agent: Agent; disabled?: boolean; below?: boolean } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();

  const ownModel = $derived(!!(agent.provider && agent.model));
  const ownReasoning = $derived(!!agent.reasoning_effort);
  const current = $derived(ownModel ? { provider: agent.provider, model: agent.model } : app.defaultModel);
  const effort = $derived(ownReasoning ? agent.reasoning_effort : app.reasoningDefault);
  // Every provider's models, plus the one in use if its list lacks it.
  const groups = $derived(
    app.providers
      .map((p) => {
        const models = [...(app.modelOptions[p.id] ?? [])];
        if (current?.provider === p.id && current.model && !models.includes(current.model)) models.unshift(current.model);
        return { id: p.id, name: p.name, models };
      })
      .filter((g) => g.models.length),
  );
  const label = $derived(current ? (modelVersion(current.model) ?? current.model) : 'No model');
  const efforts = [{ id: '', label: 'Auto' }, ...REASONING_LEVELS];

  $effect(() => {
    if (app.providers.length && Object.keys(app.modelOptions).length === 0) void app.loadModelOptions();
  });

  function pickModel(provider: string, model: string) {
    if (ownModel) void app.setAgentModel(agent.id, provider, model);
    else if (provider) void app.setDefaultModel(provider, model);
  }

  function pickEffort(level: string) {
    if (ownReasoning) void app.setAgentReasoning(agent.id, level);
    else void app.setReasoningDefault(level);
  }

  function onpointerdown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }
</script>

<svelte:window {onpointerdown} {onkeydown} />

<div class="relative min-w-0" bind:this={root}>
  <button
    class="flex max-w-full items-center gap-1.5 rounded-full px-2.5 py-1 text-[12px] text-muted transition hover:bg-hover hover:text-text {open
      ? 'bg-hover text-text'
      : ''}"
    onclick={() => (open = !open)}
    {disabled}
    use:tip={'Model and thinking effort'}
    data-tip-side={below ? 'bottom' : 'top'}
    aria-expanded={open}
  >
    <Cpu size={13} class="shrink-0" />
    <span class="truncate whitespace-nowrap max-sm:hidden">{label}</span>
    <span class="text-faint max-sm:hidden">·</span>
    <Brain size={13} class="shrink-0" />
    <span class="whitespace-nowrap">{reasoningLabel(effort)}</span>
    <ChevronDown size={13} class="shrink-0 text-faint transition-transform {open ? 'rotate-180' : ''}" />
  </button>

  {#if open}
    <div
      class="absolute right-0 z-30 w-80 rounded-2xl max-sm:fixed max-sm:right-3 max-sm:left-3 max-sm:w-auto {below ? 'top-full mt-2 max-sm:top-16' : 'bottom-full mb-2 max-sm:bottom-28'} border border-line-strong bg-elevated p-1.5 shadow-[var(--shadow-pop)] animate-pop-in"
      role="dialog"
      aria-label="Model and thinking effort"
    >
      <div class="px-2.5 pt-1.5 pb-1 text-[11px] font-semibold tracking-wider text-faint uppercase">
        Model {ownModel ? `· ${agent.name} only` : '· all agents'}
      </div>
      <div class="max-h-64 overflow-y-auto scroll-thin">
        {#if ownModel}
          <button
            class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] text-muted hover:bg-hover"
            onclick={() => pickModel('', '')}
          >
            <span class="w-3.5"></span> Use the default · {app.defaultModel?.model ?? 'none'}
          </button>
        {/if}
        {#each groups as group (group.id)}
          {#if groups.length > 1}<div class="px-2.5 pt-2 pb-0.5 text-[11px] text-faint">{group.name}</div>{/if}
          {#each group.models as m (m)}
            {@const on = current?.provider === group.id && current?.model === m}
            <button
              class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left transition hover:bg-hover {on ? 'text-text' : 'text-muted'}"
              onclick={() => pickModel(group.id, m)}
            >
              <Check size={14} class="shrink-0 {on ? 'text-accent-hi' : 'invisible'}" />
              <span class="min-w-0 flex-1">
                <span class="block text-[13px] font-medium">{modelVersion(m) ?? m}</span>
                {#if modelVersion(m)}<span class="block font-mono text-[11px] text-faint">{m}</span>{/if}
              </span>
            </button>
          {/each}
        {:else}
          <div class="px-2.5 py-2 text-[12.5px] text-faint">Loading models…</div>
        {/each}
      </div>

      <div class="mx-1.5 my-1.5 h-px bg-line"></div>
      <div class="px-2.5 pb-1.5 text-[11px] font-semibold tracking-wider text-faint uppercase">
        Thinking effort {ownReasoning ? `· ${agent.name} only` : '· all agents'}
      </div>
      <div class="grid grid-cols-3 gap-1 px-1.5 pb-1.5">
        {#each efforts as level (level.id)}
          <button
            class="rounded-lg border px-2 py-1.5 text-[12px] transition {effort === level.id
              ? 'border-accent/60 bg-accent-soft text-text'
              : 'border-line text-muted hover:bg-hover hover:text-text'}"
            onclick={() => pickEffort(level.id)}>{level.label}</button
          >
        {/each}
      </div>
      {#if ownReasoning}
        <button
          class="mx-1.5 mb-1.5 w-[calc(100%-0.75rem)] rounded-lg py-1.5 text-[12px] text-accent-hi hover:bg-hover"
          onclick={() => pickEffort('')}>Use the default effort ({reasoningLabel(app.reasoningDefault)})</button
        >
      {/if}
    </div>
  {/if}
</div>
