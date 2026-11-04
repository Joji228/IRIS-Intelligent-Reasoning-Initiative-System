<script lang="ts">
  // The chat box's "Auto" button: how much an agent may run without asking.
  import { Check, ShieldCheck, ShieldOff, Zap } from '@lucide/svelte';
  import { app, type AutoMode } from '../app.svelte';
  import { tip } from '../tooltip';
  import type { Agent } from '../types';

  let { agent, below = false }: { agent: Agent; below?: boolean } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();

  const own = $derived(app.autoModeOf(agent));
  const creator = $derived(agent.created_by ? app.agent(agent.created_by) : undefined);
  const inherited = $derived(creator ? app.autoModeOf(creator) : 'off');
  // The stronger of the agent's own mode and its creator's applies.
  const rank: Record<AutoMode, number> = { off: 0, careful: 1, full: 2 };
  const effective = $derived(rank[inherited] > rank[own] ? inherited : own);

  const modes: { id: AutoMode; label: string; hint: string; icon: typeof Zap }[] = [
    { id: 'off', label: 'Ask me', hint: 'Every command waits for your approval.', icon: ShieldOff },
    { id: 'careful', label: 'Careful auto', hint: 'A quick AI check approves safe commands; anything unclear asks you.', icon: ShieldCheck },
    {
      id: 'full',
      label: 'Full auto',
      hint: 'Runs commands without an AI check. Only a fixed list of obviously dangerous patterns (wiping disks, registry, security, installs, sending data out, secrets, shutdown) still asks; a determined or tricked agent can get around it. Fastest, least safe.',
      icon: Zap,
    },
  ];
  const label = $derived(effective === 'full' ? 'Full auto' : effective === 'careful' ? 'Auto' : 'Auto');

  function pick(mode: AutoMode) {
    open = false;
    void app.setAgentAuto(agent.id, mode);
  }

  function onpointerdown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window {onpointerdown} onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="relative shrink-0" bind:this={root}>
  <button
    class="flex items-center gap-1 rounded-full border px-2 py-1 text-[11.5px] font-medium whitespace-nowrap transition {effective !== 'off'
      ? 'border-accent/50 bg-accent-soft text-accent-hi'
      : 'border-line-strong text-faint hover:text-text'}"
    onclick={() => (open = !open)}
    use:tip={'How much this agent may run without asking you'}
    data-tip-side={below ? 'bottom' : 'top'}
    aria-expanded={open}
  >
    {#if effective === 'full'}<Zap size={12} strokeWidth={2.4} />{:else}<ShieldCheck size={12} strokeWidth={2.4} />{/if}
    <span class={below ? 'max-sm:hidden' : ''}>{label}</span>
  </button>

  {#if open}
    <div
      class="absolute right-0 z-30 w-80 rounded-2xl max-sm:fixed max-sm:right-3 max-sm:left-3 max-sm:w-auto {below ? 'top-full mt-2 max-sm:top-16' : 'bottom-full mb-2 max-sm:bottom-28'} border border-line-strong bg-elevated p-1.5 shadow-[var(--shadow-pop)] animate-pop-in"
      role="dialog"
      aria-label="Command approvals"
    >
      <div class="px-2.5 pt-1.5 pb-1 text-[11px] font-semibold tracking-wider text-faint uppercase">
        Commands · {agent.name}{agent.role === 'main' ? ' and its team' : ''}
      </div>
      {#each modes as m (m.id)}
        <button
          class="flex w-full items-start gap-2.5 rounded-lg px-2.5 py-2 text-left transition hover:bg-hover"
          onclick={() => pick(m.id)}
        >
          <m.icon size={15} class="mt-0.5 shrink-0 {own === m.id ? 'text-accent-hi' : 'text-faint'}" />
          <span class="min-w-0 flex-1">
            <span class="block text-[13px] font-medium {own === m.id ? 'text-text' : 'text-muted'}">{m.label}</span>
            <span class="block text-[11.5px] leading-snug text-faint">{m.hint}</span>
          </span>
          {#if own === m.id}<Check size={14} class="mt-0.5 shrink-0 text-accent-hi" />{/if}
        </button>
      {/each}
      {#if creator && rank[inherited] > rank[own]}
        <p class="px-2.5 pt-1 pb-1.5 text-[11.5px] text-faint">
          {creator.name}'s setting ({inherited === 'full' ? 'Full auto' : 'Careful auto'}) also applies, since it created
          this agent.
        </p>
      {/if}
    </div>
  {/if}
</div>
