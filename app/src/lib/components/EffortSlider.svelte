<script lang="ts">
  // How hard an agent thinks: a slider over Off → Max with an "Auto" pill.
  // The thumb follows the pointer exactly; on release a critically damped
  // spring carries it on with the pointer's speed and settles on a level
  // without overshooting (a quick flick reaches the next one). The choice
  // shows at once, before the server answers, and only one change is sent per
  // gesture.
  import { Brain } from '@lucide/svelte';
  import { onDestroy, untrack } from 'svelte';
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

  // What was just picked, shown at once while it travels to the server (slow
  // through the relay); it gives way when the server's answer arrives.
  let pending = $state<string | null>(null);
  let pendingTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (pending !== null && value === pending) pending = null;
  });
  const current = $derived(pending ?? value);
  const auto = $derived(current === '');
  const index = (level: string) => stops.findIndex((s) => s.id === level);
  const chosen = $derived(auto ? (index(autoLevel) >= 0 ? index(autoLevel) : 2) : Math.max(0, index(current)));

  // ---- the thumb's motion: position in stops (fractional), driven by a spring ----
  /** Stiffness (1/s²): settles in about a third of a second. */
  const K = 260;
  /** Critical damping: the fastest settle with no overshoot. */
  const C = 2 * Math.sqrt(K);
  let x = $state(untrack(() => chosen));
  let v = 0;
  let target = untrack(() => chosen);
  let frame = 0;
  let lastT = 0;

  const reduced = () =>
    document.documentElement.dataset.motion === 'off' ||
    (document.documentElement.dataset.motion === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);

  function step(now: number) {
    // Small fixed sub-steps keep it stable on slow frames.
    let dt = Math.min(0.05, (now - lastT) / 1000);
    lastT = now;
    while (dt > 0) {
      const h = Math.min(dt, 1 / 240);
      const a = -K * (x - target) - C * v;
      v += a * h;
      x += v * h;
      dt -= h;
    }
    if (Math.abs(x - target) < 0.002 && Math.abs(v) < 0.02) {
      x = target;
      v = 0;
      frame = 0;
      return;
    }
    frame = requestAnimationFrame(step);
  }
  function glide(to: number, velocity = v) {
    target = to;
    v = velocity;
    if (reduced()) {
      x = to;
      v = 0;
      return;
    }
    if (!frame) {
      lastT = performance.now();
      frame = requestAnimationFrame(step);
    }
  }
  onDestroy(() => cancelAnimationFrame(frame));

  // ---- pointer ----
  let track = $state<HTMLDivElement>();
  let pressed: { id: number; startX: number } | null = null;
  let dragging = $state(false);
  // The pointer's speed in stops per second, smoothed.
  let speed = 0;
  let lastMove = { t: 0, x: 0 };

  // The level changed from elsewhere (the server, another device): glide there.
  $effect(() => {
    const to = chosen;
    if (untrack(() => !dragging && to !== target)) glide(to);
  });

  const pct = $derived((x / last) * 100);
  const heat = $derived(Math.min(1, Math.max(0, x / last)));
  const nearest = $derived(Math.min(last, Math.max(0, Math.round(x))));
  const showsAuto = $derived(auto && !dragging);
  const shown = $derived(dragging ? nearest : chosen);
  const label = $derived(showsAuto ? autoLabel : stops[shown].label);
  const hint = $derived(showsAuto ? autoHint : hints[stops[shown].id]);

  function at(clientX: number) {
    const r = track!.getBoundingClientRect();
    // Not laid out (hidden): stay where it is rather than going NaN.
    if (!(r.width > 0)) return x;
    return Math.min(last, Math.max(0, ((clientX - r.left) / r.width) * last));
  }
  function choose(level: string) {
    if (level === current) return;
    pending = level;
    clearTimeout(pendingTimer);
    // If the server never confirms (an error), fall back to what it says.
    pendingTimer = setTimeout(() => (pending = null), 10_000);
    onchange(level);
  }
  function pick(i: number, velocity = 0) {
    glide(i, velocity);
    choose(stops[i].id);
  }
  function down(e: PointerEvent) {
    if (e.button !== 0 || !track) return;
    track.setPointerCapture(e.pointerId);
    pressed = { id: e.pointerId, startX: e.clientX };
    speed = 0;
    lastMove = { t: performance.now(), x: at(e.clientX) };
  }
  function move(e: PointerEvent) {
    if (!pressed || e.pointerId !== pressed.id) return;
    if (!dragging) {
      if (Math.abs(e.clientX - pressed.startX) < 4) return;
      dragging = true;
      cancelAnimationFrame(frame);
      frame = 0;
      v = 0;
    }
    const now = performance.now();
    const nx = at(e.clientX);
    const dt = Math.max(1, now - lastMove.t) / 1000;
    speed = 0.7 * speed + 0.3 * ((nx - lastMove.x) / dt);
    lastMove = { t: now, x: nx };
    x = nx;
  }
  function up(e: PointerEvent) {
    if (!pressed) return;
    pressed = null;
    if (dragging) {
      dragging = false;
      // A pointer that stopped before letting go has no speed left.
      const fresh = performance.now() - lastMove.t < 80;
      const vel = fresh ? Math.max(-14, Math.min(14, speed)) : 0;
      // Where the release speed would carry it, rounded to a level.
      const to = Math.min(last, Math.max(0, Math.round(x + vel * 0.12)));
      // Only speed towards the level is kept, and never so much that it would
      // overshoot (critical damping overshoots once speed > √K · distance): it
      // always arrives in one smooth movement.
      const d = to - x;
      const toward = vel * d > 0 ? Math.sign(vel) * Math.min(Math.abs(vel), 0.9 * Math.sqrt(K) * Math.abs(d)) : 0;
      pick(to, toward);
    } else {
      // A tap: glide to the level nearest the tap.
      pick(Math.round(at(e.clientX)));
    }
  }
  function key(e: KeyboardEvent) {
    const dir = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1 }[e.key];
    if (dir) {
      e.preventDefault();
      pick(Math.min(last, Math.max(0, chosen + dir)));
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      pick(e.key === 'Home' ? 0 : last);
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
      onclick={() => choose('')}>{autoLabel}</button
    >
  </div>

  <!-- Inset, so the thumb and the end labels stay inside the box at Off and Max -->
  <div class="effort-body">
  <div
    bind:this={track}
    class="effort-track {showsAuto ? 'is-auto' : ''} {dragging ? 'is-dragging' : ''}"
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
    <div class="effort-fill" style="width: calc({pct}% + 5px)"></div>
    {#each stops as s, i (s.id)}
      <span class="effort-dot {i <= x + 0.01 ? 'on' : ''}" style="left: {(i / last) * 100}%"></span>
    {/each}
    <div class="effort-thumb" style="left: {pct}%"><span class="effort-thumb-core"></span></div>
  </div>

  <div class="relative mt-2 h-5">
    {#each stops as s, i (s.id)}
      <button
        class="effort-stop {!auto && i === chosen ? 'on' : ''}"
        style="left: {(i / last) * 100}%"
        tabindex="-1"
        onclick={() => pick(i)}>{s.label}</button
      >
    {/each}
  </div>
  </div>
</div>

<style>
  .effort {
    /* Smooth deceleration, no overshoot. */
    --ease: cubic-bezier(0.22, 1, 0.36, 1);
    --warm: #f472b6;
    --tint: color-mix(in srgb, var(--warm) calc(var(--heat) * 75%), var(--color-accent));
  }
  .effort-body {
    padding: 0 12px;
  }
  .effort-label {
    animation: label-in 180ms ease-out;
  }
  @keyframes label-in {
    from {
      opacity: 0.35;
    }
  }
  .effort-auto {
    transition:
      background-color 160ms ease-out,
      color 160ms ease-out,
      border-color 160ms ease-out;
  }
  .effort-track {
    position: relative;
    height: 34px;
    cursor: pointer;
    touch-action: none;
    outline: none;
    border-radius: 999px;
  }
  /* The bar reaches 5px past the end stops, so their dots sit inside it. */
  .effort-rail,
  .effort-fill {
    position: absolute;
    top: 50%;
    left: -5px;
    height: 10px;
    margin-top: -5px;
    border-radius: 999px;
  }
  .effort-rail {
    right: -5px;
    background: var(--color-line-strong);
    box-shadow: inset 0 1px 2px rgb(0 0 0 / 0.25);
  }
  /* The fill reaches under the thumb's middle; a soft highlight on top gives it depth. */
  .effort-fill {
    min-width: 10px;
    background:
      linear-gradient(180deg, rgb(255 255 255 / 0.18), rgb(255 255 255 / 0) 60%),
      linear-gradient(90deg, var(--color-accent), var(--tint));
    box-shadow: 0 0 calc(4px + var(--heat) * 10px) color-mix(in srgb, var(--tint) calc(15% + var(--heat) * 25%), transparent);
    transition:
      opacity 200ms ease-out,
      box-shadow 260ms ease-out;
  }
  .effort-dot {
    position: absolute;
    top: 50%;
    width: 4px;
    height: 4px;
    margin: -2px 0 0 -2px;
    border-radius: 999px;
    background: var(--color-faint);
    opacity: 0.6;
    transition:
      background-color 200ms ease-out,
      opacity 200ms ease-out;
  }
  .effort-dot.on {
    background: #fff;
    opacity: 0.7;
  }
  .effort-thumb {
    position: absolute;
    top: 50%;
    width: 24px;
    height: 24px;
    margin: -12px 0 0 -12px;
    border-radius: 999px;
    background: #fff;
    display: grid;
    place-items: center;
    box-shadow:
      0 1px 3px rgb(0 0 0 / 0.3),
      0 0 0 1.5px color-mix(in srgb, var(--tint) 75%, transparent),
      0 0 calc(6px + var(--heat) * 12px) color-mix(in srgb, var(--tint) calc(20% + var(--heat) * 30%), transparent);
    /* Position comes from the spring in the script; only looks are transitioned here. */
    transition:
      transform 180ms var(--ease),
      opacity 200ms ease-out,
      box-shadow 260ms ease-out;
  }
  .effort-thumb-core {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--tint);
    transition:
      background-color 260ms ease-out,
      transform 180ms var(--ease);
  }
  .effort-track:hover .effort-thumb {
    transform: scale(1.06);
  }
  .effort-track:focus-visible .effort-thumb {
    box-shadow:
      0 1px 3px rgb(0 0 0 / 0.3),
      0 0 0 4px color-mix(in srgb, var(--color-accent) 35%, transparent);
  }
  /* While dragging the thumb follows the pointer exactly and lifts a little. */
  .effort-track.is-dragging {
    cursor: grabbing;
  }
  .effort-track.is-dragging .effort-thumb {
    transform: scale(1.12);
    transition:
      transform 180ms var(--ease),
      box-shadow 260ms ease-out;
  }
  .effort-track.is-dragging .effort-thumb-core {
    transform: scale(1.25);
  }
  .effort-track.is-dragging .effort-fill {
    transition: box-shadow 260ms ease-out;
  }
  .effort-track.is-auto .effort-fill {
    opacity: 0.3;
  }
  .effort-track.is-auto .effort-thumb {
    opacity: 0.55;
  }
  .effort-stop {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    font-size: 11.5px;
    color: var(--color-faint);
    white-space: nowrap;
    transition: color 200ms ease-out;
  }
  .effort-stop:first-child {
    transform: translateX(-12px);
  }
  .effort-stop:last-child {
    transform: translateX(calc(-100% + 12px));
  }
  .effort-stop:hover {
    color: var(--color-text);
  }
  .effort-stop.on {
    color: var(--color-accent-hi);
    font-weight: 600;
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
