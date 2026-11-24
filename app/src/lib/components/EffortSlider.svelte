<script lang="ts">
  // How hard an agent thinks: a slider over Off → Max with an "Auto" pill.
  // The thumb springs between stops, the fill warms up as the level rises, and
  // dragging only commits once, on release (one settings change per gesture).
  import { Brain } from '@lucide/svelte';
  import { REASONING_LEVELS } from '../types';

  let {
    value,
    onchange,
    autoLabel = 'Auto',
    autoLevel = '',
    autoHint = 'The model decides',
  }: {
    /** '' = Auto, else one of REASONING_LEVELS. */
    value: string;
    onchange: (level: string) => void;
    /** The pill's name: "Auto" for the default, "Default" for an agent's own level. */
    autoLabel?: string;
    /** Where the thumb rests while on Auto (e.g. the default level), if known. */
    autoLevel?: string;
    autoHint?: string;
  } = $props();

  const stops = REASONING_LEVELS;
  const last = stops.length - 1;
  const hints: Record<string, string> = {
    off: 'Answers right away',
    low: 'A quick think',
    medium: 'Balanced',
    high: 'Thinks it through',
    max: 'Deepest and slowest',
  };

  const auto = $derived(value === '');
  const index = (level: string) => stops.findIndex((s) => s.id === level);
  const chosen = $derived(auto ? (index(autoLevel) >= 0 ? index(autoLevel) : 2) : Math.max(0, index(value)));

  let track = $state<HTMLDivElement>();
  // While dragging: the thumb's position in stops (fractional), else null.
  let drag = $state<number | null>(null);
  const pos = $derived(drag ?? chosen);
  const pct = $derived((pos / last) * 100);
  const heat = $derived(pos / last);
  const nearest = $derived(Math.round(pos));
  const label = $derived(auto && drag === null ? autoLabel : stops[nearest].label);
  const hint = $derived(auto && drag === null ? autoHint : hints[stops[nearest].id]);

  function at(e: PointerEvent) {
    const r = track!.getBoundingClientRect();
    return Math.min(last, Math.max(0, ((e.clientX - r.left) / r.width) * last));
  }
  function commit(i: number) {
    const level = stops[i].id;
    if (level !== value) onchange(level);
  }
  function down(e: PointerEvent) {
    if (e.button !== 0 || !track) return;
    track.setPointerCapture(e.pointerId);
    drag = at(e);
  }
  function move(e: PointerEvent) {
    if (drag !== null) drag = at(e);
  }
  function up() {
    if (drag === null) return;
    const i = Math.round(drag);
    drag = null;
    commit(i);
  }
  function key(e: KeyboardEvent) {
    const step = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1 }[e.key];
    if (step) {
      e.preventDefault();
      commit(Math.min(last, Math.max(0, chosen + step)));
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      commit(e.key === 'Home' ? 0 : last);
    }
  }
</script>

<div class="effort" style="--heat: {heat}">
  <div class="mb-2.5 flex items-center gap-2">
    <Brain size={14} class="shrink-0 text-accent-hi" />
    {#key label}<span class="effort-label text-[13px] font-medium">{label}</span>{/key}
    <span class="min-w-0 flex-1 truncate text-[12px] text-faint">{hint}</span>
    <button
      class="effort-auto shrink-0 rounded-full border px-2.5 py-0.5 text-[11.5px] font-medium {auto
        ? 'border-accent/50 bg-accent-soft text-accent-hi'
        : 'border-line-strong text-muted hover:bg-hover hover:text-text'}"
      aria-pressed={auto}
      onclick={() => !auto && onchange('')}>{autoLabel}</button
    >
  </div>

  <!-- Inset, so the thumb and the end labels stay inside the box at Off and Max -->
  <div class="effort-body">
  <div
    bind:this={track}
    class="effort-track {auto && drag === null ? 'is-auto' : ''} {drag !== null ? 'is-dragging' : ''}"
    role="slider"
    tabindex="0"
    aria-label="Thinking effort"
    aria-valuemin={0}
    aria-valuemax={last}
    aria-valuenow={chosen}
    aria-valuetext={auto ? autoLabel : stops[chosen].label}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onkeydown={key}
  >
    <div class="effort-rail"></div>
    <div class="effort-fill" style="width: {pct}%"></div>
    {#each stops as s, i (s.id)}
      <span class="effort-dot {i <= pos + 0.01 ? 'on' : ''}" style="left: {(i / last) * 100}%"></span>
    {/each}
    <div class="effort-thumb" style="left: {pct}%"></div>
  </div>

  <div class="relative mt-1.5 h-5">
    {#each stops as s, i (s.id)}
      <button
        class="effort-stop {!auto && i === chosen ? 'on' : ''}"
        style="left: {(i / last) * 100}%"
        tabindex="-1"
        onclick={() => commit(i)}>{s.label}</button
      >
    {/each}
  </div>
  </div>
</div>

<style>
  .effort {
    --spring: cubic-bezier(0.34, 1.45, 0.64, 1);
    --warm: #f472b6;
  }
  .effort-body {
    padding: 0 10px;
  }
  .effort-label {
    animation: label-in 260ms var(--spring);
  }
  @keyframes label-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  .effort-auto {
    transition: background-color 180ms, color 180ms, border-color 180ms, transform 120ms;
  }
  .effort-auto:active {
    transform: scale(0.94);
  }
  .effort-track {
    position: relative;
    height: 28px;
    cursor: pointer;
    touch-action: none;
    outline: none;
    border-radius: 999px;
  }
  .effort-track:focus-visible .effort-thumb {
    box-shadow:
      0 0 0 4px color-mix(in srgb, var(--color-accent) 35%, transparent),
      0 0 calc(6px + var(--heat) * 14px) color-mix(in srgb, var(--color-accent) calc(25% + var(--heat) * 35%), transparent);
  }
  .effort-rail,
  .effort-fill {
    position: absolute;
    top: 50%;
    left: 0;
    height: 6px;
    margin-top: -3px;
    border-radius: 999px;
  }
  .effort-rail {
    right: 0;
    background: var(--color-line-strong);
  }
  .effort-fill {
    background: linear-gradient(
      90deg,
      var(--color-accent),
      color-mix(in srgb, var(--warm) calc(var(--heat) * 85%), var(--color-accent))
    );
    transition:
      width 420ms var(--spring),
      opacity 200ms;
  }
  .effort-dot {
    position: absolute;
    top: 50%;
    width: 6px;
    height: 6px;
    margin: -3px 0 0 -3px;
    border-radius: 999px;
    background: var(--color-faint);
    opacity: 0.55;
    transition:
      background-color 200ms,
      opacity 200ms,
      transform 200ms;
  }
  .effort-dot.on {
    background: var(--color-on-accent);
    opacity: 0.9;
    transform: scale(0.8);
  }
  .effort-thumb {
    position: absolute;
    top: 50%;
    width: 20px;
    height: 20px;
    margin: -10px 0 0 -10px;
    border-radius: 999px;
    background: #fff;
    border: 2px solid color-mix(in srgb, var(--warm) calc(var(--heat) * 70%), var(--color-accent));
    box-shadow: 0 0 calc(6px + var(--heat) * 14px) color-mix(in srgb, var(--color-accent) calc(25% + var(--heat) * 35%), transparent);
    transition:
      left 420ms var(--spring),
      transform 180ms var(--spring),
      opacity 200ms,
      box-shadow 300ms,
      border-color 300ms;
  }
  .effort-track:hover .effort-thumb {
    transform: scale(1.08);
  }
  .effort-track.is-dragging .effort-thumb {
    transform: scale(1.18);
    transition:
      transform 180ms var(--spring),
      box-shadow 300ms,
      border-color 300ms;
  }
  .effort-track.is-dragging .effort-fill {
    transition: opacity 200ms;
  }
  .effort-track.is-auto .effort-fill,
  .effort-track.is-auto .effort-thumb {
    opacity: 0.35;
  }
  .effort-stop {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    font-size: 11.5px;
    color: var(--color-faint);
    white-space: nowrap;
    transition:
      color 200ms,
      transform 260ms var(--spring);
  }
  .effort-stop:first-child {
    transform: translateX(-10px);
  }
  .effort-stop:last-child {
    transform: translateX(calc(-100% + 10px));
  }
  .effort-stop:hover {
    color: var(--color-text);
  }
  .effort-stop.on {
    color: var(--color-accent-hi);
    font-weight: 500;
  }
  .effort-stop.on:not(:first-child):not(:last-child) {
    transform: translateX(-50%) scale(1.08);
  }
  :global(:root[data-motion='off']) .effort *,
  :global(:root[data-motion='off']) .effort {
    transition: none !important;
    animation: none !important;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(:root[data-motion='system']) .effort *,
    :global(:root[data-motion='system']) .effort {
      transition: none !important;
      animation: none !important;
    }
  }
</style>
