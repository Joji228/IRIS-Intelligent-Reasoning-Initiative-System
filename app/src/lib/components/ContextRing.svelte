<script lang="ts">
  // A small ring in the composer that fills as the conversation uses up the
  // model's context window. Click it for details: context, plus the daily and
  // weekly token limits.
  import { app } from '../app.svelte';
  import { formatTokens, knownContextWindow } from '../format';
  import { tip } from '../tooltip';
  import UsageBar from './UsageBar.svelte';

  /** `model`: the agent's own model, if it has one (else the default's). */
  let { used, agentName, model, below = false }: { used: number; agentName: string; model?: string; below?: boolean } = $props();

  let open = $state(false);
  let root: HTMLDivElement;

  const modelId = $derived(model ?? app.defaultModel?.model ?? 'model');
  const total = $derived(model ? knownContextWindow(model) : (app.defaultModel?.context_window ?? null));
  const pct = $derived(total ? Math.min(100, (used / total) * 100) : 0);
  const R = 7;
  const C = 2 * Math.PI * R;
  const stroke = $derived(pct >= 90 ? 'var(--c-danger)' : pct >= 70 ? 'var(--c-warn)' : 'var(--c-accent-hi)');
  const label = $derived(
    total ? `Context ${formatTokens(used)} / ${formatTokens(total)} (${pct < 1 && used > 0 ? '<1' : Math.round(pct)}%)` : `Context ${formatTokens(used)}`,
  );

  function toggle() {
    open = !open;
    if (open) void app.refreshLimits();
  }

  // Close when clicking anywhere else or pressing Escape.
  function onwindowclick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
  }
</script>

<svelte:window onclick={onwindowclick} {onkeydown} />

<div class="relative" bind:this={root}>
  <button
    class="flex h-9 w-9 items-center justify-center rounded-full transition hover:bg-hover"
    onclick={toggle}
    use:tip={label}
    data-tip-side={below ? 'bottom' : 'top'}
    aria-label={label}
    aria-expanded={open}
  >
    <svg viewBox="0 0 20 20" class="h-5 w-5 -rotate-90">
      <circle cx="10" cy="10" r={R} fill="none" stroke="var(--c-line-strong)" stroke-width="2.4" />
      <circle
        cx="10"
        cy="10"
        r={R}
        fill="none"
        stroke={stroke}
        stroke-width="2.4"
        stroke-linecap="round"
        stroke-dasharray={C}
        stroke-dashoffset={C * (1 - Math.max(pct, used > 0 ? 2 : 0) / 100)}
        class="transition-[stroke-dashoffset] duration-500"
      />
    </svg>
  </button>

  {#if open}
    <div
      class="absolute right-0 z-30 w-80 rounded-2xl max-sm:fixed max-sm:right-3 max-sm:left-3 max-sm:w-auto {below ? 'top-full mt-2 max-sm:top-16' : 'bottom-full mb-2'} border border-line-strong bg-elevated p-4 shadow-[var(--shadow-pop)] animate-pop-in"
      role="dialog"
      aria-label="Context and usage"
    >
      <div class="space-y-4">
        <UsageBar
          label="Context"
          {used}
          limit={total}
          hint={`This conversation with ${agentName} · ${modelId}${total ? ` holds up to ${formatTokens(total)} tokens` : ''}`}
        />
        <div class="h-px bg-line"></div>
        <UsageBar label="Daily limit" used={app.limits?.used_today ?? 0} limit={app.limits?.daily_tokens || null} hint="Resets at midnight" />
        <UsageBar
          label="Weekly limit"
          used={app.limits?.used_this_week ?? 0}
          limit={app.limits?.weekly_tokens || null}
          hint="Resets on Monday"
        />
      </div>
      <button
        class="mt-4 w-full rounded-lg py-1.5 text-[12px] text-accent-hi transition hover:bg-hover"
        onclick={() => {
          open = false;
          app.modal = { kind: 'settings', tab: 'limits' };
        }}>Change limits</button
      >
    </div>
  {/if}
</div>
