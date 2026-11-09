<script lang="ts">
  // While a call goes on behind the app (the call screen closed with Esc or the arrow), a green
  // pill at the top says so: who, for how long, click to go back, or hang up right there.
  import { Phone } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { call } from '../call/call.svelte';

  let now = $state(Date.now());
  const timer = setInterval(() => (now = Date.now()), 1000);
  onDestroy(() => clearInterval(timer));
  const s = $derived(Math.max(0, Math.floor((now - call.startedAt) / 1000)));
  const elapsed = $derived(`${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`);
</script>

<div class="flex shrink-0 justify-center border-b border-line bg-[color-mix(in_srgb,var(--c-success)_6%,var(--c-bg))] px-3 py-1.5 animate-fade-in">
  <div class="flex items-center gap-3 rounded-full border border-success/40 bg-[color-mix(in_srgb,var(--c-success)_14%,var(--c-panel))] py-1.5 pr-1.5 pl-4 ">
    <button class="flex items-center gap-2.5 text-[13.5px] font-medium" onclick={() => (call.open = true)} title="Back to the call">
      <span class="size-2.5 animate-pulse rounded-full bg-success"></span>
      On a call with {call.agent?.name}
      <span class="font-mono text-[12.5px] tabular-nums text-success">{elapsed}</span>
    </button>
    <button class="grid size-8 place-items-center rounded-full bg-danger text-white transition hover:brightness-110" onclick={() => call.hangUp()} aria-label="Hang up" title="Hang up">
      <Phone size={15} style="transform: rotate(135deg)" />
    </button>
  </div>
</div>
