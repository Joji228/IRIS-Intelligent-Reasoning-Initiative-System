<script lang="ts">
  // An agent's face: a flat, single-color orb with eyes and an orbiting ring
  // (2D take on the I.R.I.S. orb). It floats, blinks and twinkles, and its
  // face changes with what it's up to: each state (thinking, searching,
  // reading, working, waiting, done, failed, asleep...) has a few faces it
  // morphs between, plus a body motion and now and then a little effect
  // (thought dots, sparkles when it finishes, a sweat drop when it fails, z's
  // when it dozes). Rail orbs also get a speech bubble saying what they do.
  import { onDestroy } from 'svelte';
  import { FileText, Globe, Hourglass, Search, Sparkles, SquareTerminal, Users } from '@lucide/svelte';
  import type { Agent, Mood } from '../types';
  import { FACES, HOLD, POOLS, orbState, phrases, type Face, type OrbState } from '../orbs';

  let {
    agent,
    size = 40,
    working = false,
    waiting = false,
    mood,
    activity,
    detail = '',
    unread = false,
    sleepy = false,
    speech = false,
  }: {
    agent?: Pick<Agent, 'name' | 'color'> & { id?: string };
    size?: number;
    working?: boolean;
    waiting?: boolean;
    mood?: Mood;
    /** What the agent is doing ("thinking", a tool name, "waiting"). */
    activity?: string;
    /** That step in words, shown under the bubble's phrase. */
    detail?: string;
    /** It has a reply the user hasn't read: it nudges. */
    unread?: boolean;
    /** The user is away: an idle orb dozes off. */
    sleepy?: boolean;
    /** Shows a speech bubble beside the orb (rail orbs). */
    speech?: boolean;
  } = $props();

  const color = $derived(agent?.color || '#c4b5fd');
  // Shades of the one color, mixed toward the app background: light sphere,
  // mid band, dark face (so the white eyes always read).
  const light = $derived(color);
  const mid = $derived(`color-mix(in srgb, ${color} 62%, #0b0b14)`);
  const dark = $derived(`color-mix(in srgb, ${color} 30%, #0b0b14)`);

  // Different agents float and blink out of step with each other.
  const seed = $derived(
    [...(agent?.id ?? agent?.name ?? 'iris')].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7),
  );
  const delay = $derived(`-${(seed % 4000) / 1000}s`);
  const blinkDelay = $derived(`-${(seed % 6100) / 1000}s`);
  const lookDelay = $derived(`-${(seed % 11000) / 1000}s`);
  const turnDelay = $derived(`-${(seed % 17000) / 1000}s`);
  const showRing = $derived(size >= 26);
  const effects = $derived(size >= 26);

  const orbMode: OrbState = $derived(orbState({ activity, mood, working, waiting, unread, sleepy }));

  const motionOff = () => {
    const m = document.documentElement.dataset.motion;
    return m === 'off' || (m === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  };

  // Waking up: a stretch and wide eyes when a dozing orb gets going again.
  let waking = $state(false);
  let prevState: OrbState | null = null;
  let wakeTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const s = orbMode;
    if (prevState === 'sleeping' && s !== 'sleeping') {
      waking = true;
      clearTimeout(wakeTimer);
      wakeTimer = setTimeout(() => (waking = false), 1300);
    }
    prevState = s;
  });

  // The face: cycles through the state's pool, morphing from one to the next.
  let faceIdx = $state(0);
  $effect(() => {
    const s = orbMode;
    const pool = POOLS[s];
    faceIdx = 0;
    if (pool.length < 2 || motionOff()) return;
    let t: ReturnType<typeof setTimeout>;
    const next = () => {
      const hold = (HOLD[s] ?? 2200) * (0.75 + Math.random() * 0.6);
      t = setTimeout(() => {
        // Reading sweeps left-right in order; everything else picks freely.
        if (s === 'reading' || s === 'celebrate') faceIdx = (faceIdx + 1) % pool.length;
        else {
          let i = Math.floor(Math.random() * pool.length);
          if (i === faceIdx) i = (i + 1) % pool.length;
          faceIdx = i;
        }
        next();
      }, hold);
    };
    next();
    return () => clearTimeout(t);
  });
  const face: Face = $derived(waking ? FACES.wide : FACES[POOLS[orbMode][faceIdx % POOLS[orbMode].length]]);

  const eyeT = (e: Face['l']) => `translate(${e[3]}px, ${e[4]}px) rotate(${e[2]}deg) scale(${e[0]}, ${e[1]})`;
  const browT = (rot: number, lift: number) => `translate(0px, ${lift}px) rotate(${rot}deg)`;

  // ---- speech bubble -------------------------------------------------
  // Pops up when the agent starts a new step, then folds into a small chip
  // after a few seconds; hovering the orb opens it again. Waiting for the
  // user stays open.
  const lines = $derived(phrases(orbMode, activity));
  const stepKey = $derived(`${orbMode}|${activity ?? ''}|${detail}`);
  let phraseIdx = $state(0);
  let open = $state(false);
  let hovered = $state(false);
  let openTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (!speech) return;
    void stepKey;
    phraseIdx = 0;
    clearTimeout(openTimer);
    if (!lines.length) {
      open = false;
      return;
    }
    open = true;
    if (orbMode !== 'waiting') {
      const ms = orbMode === 'celebrate' || orbMode === 'unread' || orbMode === 'spawn' ? 2600 : 5200;
      openTimer = setTimeout(() => (open = false), ms);
    }
  });
  // Long steps: the phrase changes now and then while it's open.
  $effect(() => {
    if (!speech || !(open || hovered) || lines.length < 2) return;
    const t = setInterval(() => (phraseIdx = (phraseIdx + 1) % lines.length), 3200);
    return () => clearInterval(t);
  });
  const showBubble = $derived(speech && lines.length > 0 && (open || hovered) && orbMode !== 'sleeping');
  const busy = $derived(!['idle', 'sleeping', 'unread', 'celebrate', 'failed', 'spawn'].includes(orbMode));

  // Small chip on the orb while the bubble is folded: an icon for the step.
  const chip = $derived.by(() => {
    if (!speech || showBubble || !busy) return null;
    if (orbMode === 'thinking') return { kind: 'dots' as const };
    if (orbMode === 'waiting') return { kind: 'icon' as const, icon: Hourglass, warn: true };
    const icon =
      orbMode === 'team'
        ? Users
        : orbMode === 'designing'
          ? Sparkles
          : activity === 'run_command'
            ? SquareTerminal
            : activity?.includes('file') || orbMode === 'reading'
              ? FileText
              : activity === 'web_fetch' || activity?.startsWith('browser_')
                ? Globe
                : Search;
    return { kind: 'icon' as const, icon, warn: false };
  });

  // The bubble lives on <body> (the rail scrolls and transforms its orbs),
  // following the orb while it's shown.
  let root: HTMLDivElement | undefined = $state();
  let bubbleEl: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0, left: false, hidden: false });
  $effect(() => {
    if (!showBubble || !root) return;
    let raf = 0;
    const place = () => {
      const r = root!.getBoundingClientRect();
      const w = bubbleEl?.offsetWidth ?? 180;
      const left = r.right + 10 + w > window.innerWidth - 6;
      // The orb isn't on screen (its bar is hidden, as on a phone in a chat):
      // no bubble pinned to the corner.
      const hidden = r.width === 0 && r.height === 0;
      pos = { x: left ? r.left - 10 : r.right + 8, y: r.top + r.height / 2, left, hidden };
      raf = requestAnimationFrame(place);
    };
    place();
    return () => cancelAnimationFrame(raf);
  });
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
  onDestroy(() => {
    clearTimeout(openTimer);
    clearTimeout(wakeTimer);
  });
</script>

<div
  bind:this={root}
  class="orb relative shrink-0 rounded-full st-{orbMode}"
  class:is-waking={waking}
  style:width="{size}px"
  style:height="{size}px"
  style:--delay={delay}
  style:--blink-delay={blinkDelay}
  style:--look-delay={lookDelay}
  style:--turn-delay={turnDelay}
  onpointerenter={() => (hovered = true)}
  onpointerleave={() => (hovered = false)}
  role="presentation"
>
  <svg viewBox="0 0 64 64" class="orb-body h-full w-full overflow-visible" aria-hidden="true">
    <g class="orb-mood">
      <g class="orb-float">
        {#if showRing}
          <!-- back half of the ring, behind the body -->
          <g transform="rotate(-14 32 37)">
            <path class="orb-ring" d="M 4 37 A 28 8.5 0 0 1 60 37" fill="none" stroke={light} stroke-width="2.4" opacity="0.5" />
          </g>
        {/if}
        <circle cx="32" cy="31" r="21.5" style:fill={light} opacity="0.55" />
        <ellipse cx="34" cy="34" rx="20" ry="15" transform="rotate(-28 34 34)" style:fill={mid} />
        <!-- The face: slides across the sphere to look or turn around -->
        <g class="orb-head">
          <circle cx="30.5" cy="33" r="14.5" style:fill={dark} />
          <g class="orb-gaze">
            <g class="orb-eyes">
              {#each [{ e: face.l, cx: 26.2 }, { e: face.r, cx: 36 }] as { e, cx }, i (i)}
                <g class="orb-eye" style:transform={eyeT(e)} style:transform-origin="{cx}px 31.3px">
                  <g class="orb-blink" style:transform-origin="{cx}px 31.3px">
                    <rect
                      x={cx - 2.8}
                      y="26.6"
                      width="5.6"
                      height="9.4"
                      rx="2.8"
                      fill="white"
                      class="eye-part"
                      style:opacity={face.eyes === 'open' ? 1 : 0}
                    />
                  </g>
                  <path
                    d="M{cx - 3} 32.6 Q{cx} 27.4 {cx + 3} 32.6"
                    fill="none"
                    stroke="white"
                    stroke-width="2.6"
                    stroke-linecap="round"
                    class="eye-part"
                    style:opacity={face.eyes === 'happy' ? 1 : 0}
                  />
                  <path
                    d="M{cx - 2.9} 32 Q{cx} 34.6 {cx + 2.9} 32"
                    fill="none"
                    stroke="white"
                    stroke-width="2.2"
                    stroke-linecap="round"
                    class="eye-part"
                    style:opacity={face.eyes === 'closed' ? 1 : 0}
                  />
                </g>
              {/each}
            </g>
          </g>
          <g class="orb-brows" fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" style:opacity={face.brows ? 1 : 0}>
            <path
              d="M23.2 23.8 L28.6 23.8"
              class="orb-brow"
              style:transform={browT(face.brows?.[0] ?? 0, face.brows?.[2] ?? 1.5)}
              style:transform-origin="25.9px 23.8px"
            />
            <path
              d="M33.6 23.8 L39 23.8"
              class="orb-brow"
              style:transform={browT(face.brows?.[1] ?? 0, face.brows?.[2] ?? 1.5)}
              style:transform-origin="36.3px 23.8px"
            />
          </g>
        </g>
        {#if showRing}
          <!-- front half of the ring and its sparkle -->
          <g transform="rotate(-14 32 37)">
            <path class="orb-ring" d="M 4 37 A 28 8.5 0 0 0 60 37" fill="none" stroke={light} stroke-width="2.4" stroke-linecap="round" />
            <path
              class="orb-sparkle"
              d="M57 26.5 L58.4 30.6 L62.5 32 L58.4 33.4 L57 37.5 L55.6 33.4 L51.5 32 L55.6 30.6 Z"
              fill="white"
            />
          </g>
        {/if}
      </g>
    </g>

    {#if effects}
      {#if orbMode === 'thinking'}
        <!-- thought dots drifting up -->
        <g class="fx fx-think" fill="white">
          <circle cx="47" cy="15" r="1.5" />
          <circle cx="51.5" cy="10" r="2" />
          <circle cx="57" cy="4.5" r="2.6" />
        </g>
      {:else if orbMode === 'sleeping'}
        <g class="fx fx-zzz" fill="white" font-family="ui-sans-serif, system-ui" font-weight="700">
          <text x="46" y="16" font-size="8">z</text>
          <text x="46" y="16" font-size="8">z</text>
          <text x="46" y="16" font-size="8">z</text>
        </g>
      {:else if orbMode === 'failed'}
        <path class="fx fx-sweat" d="M47 14 Q44 19.5 47 21 Q50 19.5 47 14 Z" fill="#9fd8ff" />
      {:else if orbMode === 'celebrate' || orbMode === 'spawn'}
        <g class="fx fx-burst" fill="white">
          {#each [[-1, -1], [1, -1.2], [1.3, 0.4], [-1.3, 0.3], [0, -1.5], [0.6, 1.2]] as [dx, dy], i (i)}
            <path
              d="M32 28.5 L33 31 L35.5 32 L33 33 L32 35.5 L31 33 L28.5 32 L31 31 Z"
              style:--dx="{dx * 26}px"
              style:--dy="{dy * 24}px"
              style:animation-delay="{i * 40}ms"
              style:fill={i % 2 ? light : 'white'}
            />
          {/each}
        </g>
      {:else if orbMode === 'unread'}
        <g class="fx fx-ping">
          <circle cx="53" cy="11" r="4.2" style:fill={light} />
          <circle cx="53" cy="11" r="4.2" class="ping-ring" fill="none" style:stroke={light} stroke-width="1.6" />
        </g>
      {/if}
    {/if}
  </svg>
  {#if orbMode === 'waiting'}
    <span class="waiting-ring"></span>
  {:else if busy}
    <span class="working-ring"></span>
  {/if}
  {#if chip}
    <span class="orb-chip" class:warn={chip.kind === 'icon' && chip.warn}>
      {#if chip.kind === 'dots'}
        <span class="dots"><i></i><i></i><i></i></span>
      {:else}
        <chip.icon size={9} strokeWidth={2.6} />
      {/if}
    </span>
  {/if}
</div>

{#if showBubble && !pos.hidden}
  <div
    use:portal
    bind:this={bubbleEl}
    class="orb-speech"
    class:left={pos.left}
    class:warn={orbMode === 'waiting'}
    class:good={orbMode === 'celebrate'}
    class:bad={orbMode === 'failed'}
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    style:--orb={light}
  >
    {#key phraseIdx}
      <span class="speech-line">{lines[phraseIdx % lines.length]}</span>
    {/key}
    {#if detail && busy}
      <span class="speech-detail">{detail}</span>
    {/if}
  </div>
{/if}

<style>
  .orb-mood,
  .orb-float,
  .orb-head,
  .orb-gaze,
  .orb-eyes,
  .orb-brows,
  .orb-sparkle,
  .fx,
  .fx * {
    transform-box: fill-box;
    transform-origin: center;
  }
  .orb-eye,
  .orb-blink,
  .orb-brow {
    transform-box: view-box;
  }
  /* Faces morph: eyes and brows glide to their new shape. */
  .orb-eye,
  .orb-brow {
    transition: transform 0.32s cubic-bezier(0.34, 1.4, 0.5, 1);
  }
  .eye-part,
  .orb-brows {
    transition: opacity 0.18s ease;
  }
  .orb-float {
    animation: orb-float 3.8s ease-in-out infinite;
    animation-delay: var(--delay);
  }
  .orb-eyes {
    filter: drop-shadow(0 0 1.6px rgb(255 255 255 / 0.9));
  }
  .orb-blink {
    animation: orb-blink 6.1s infinite;
    animation-delay: var(--blink-delay);
  }
  .orb-sparkle {
    animation: orb-twinkle 2.8s ease-in-out infinite;
    animation-delay: var(--delay);
  }

  /* Idle: now and then it looks around, and turns its head to one side. */
  .orb-gaze {
    animation: orb-look-around 11s ease-in-out infinite;
    animation-delay: var(--look-delay);
  }
  .orb-head {
    animation: orb-head-turn 17s ease-in-out infinite;
    animation-delay: var(--turn-delay);
  }
  :is(.st-thinking, .st-searching, .st-reading, .st-working, .st-team, .st-reviewing, .st-waiting, .st-designing, .st-sleeping, .st-celebrate, .st-failed)
    .orb-gaze {
    animation: none;
  }

  /* Thinking: drifts as it ponders. */
  .st-thinking .orb-head {
    animation: orb-ponder 3.6s ease-in-out infinite;
  }
  /* Searching: the head scans side to side, the sparkle spins. */
  .st-searching .orb-head {
    animation: orb-scan 2.6s ease-in-out infinite;
  }
  /* Reading: a small nod down the page. */
  .st-reading .orb-head {
    animation: orb-read 1.8s ease-in-out infinite;
  }
  /* Working: busy little shifts, the sparkle spins. */
  .st-working .orb-head {
    animation: orb-head-busy 1.9s ease-in-out infinite;
  }
  .st-working .orb-mood {
    animation: orb-effort 0.9s ease-in-out infinite;
  }
  :is(.st-working, .st-searching, .st-designing) .orb-sparkle {
    animation: orb-spin 1.6s linear infinite;
  }
  /* Handing out work: turns all the way round, now and then. */
  .st-team .orb-head,
  .st-spin .orb-head {
    animation: orb-turnaround 3.4s cubic-bezier(0.45, 0, 0.25, 1) infinite;
  }
  .st-spin .orb-mood {
    animation: orb-bob 1.6s ease-in-out;
  }
  /* Checking a command before it runs: leans in, squinting. */
  .st-reviewing .orb-head {
    animation: orb-lean 2.4s ease-in-out infinite;
  }
  /* Waiting for the user: a patient bob. */
  .st-waiting .orb-mood {
    animation: orb-bob 2.4s ease-in-out infinite;
  }
  /* Taking on its job: bounces with excitement. */
  .st-designing .orb-mood {
    animation: orb-hop 1.1s cubic-bezier(0.3, 1.6, 0.5, 1) infinite;
  }
  /* Done: hops, the ring spins, sparkles burst out. */
  .st-celebrate .orb-mood {
    animation: orb-hop 0.85s cubic-bezier(0.3, 1.6, 0.5, 1) 2;
  }
  .st-celebrate .orb-sparkle {
    animation: orb-spin 0.6s linear 3;
  }
  /* Failed: sinks and shivers. */
  .st-failed .orb-mood {
    animation: orb-sink 0.6s ease-out forwards;
  }
  .st-failed .orb-head {
    animation: orb-shiver 0.5s ease-in-out 2;
  }
  .st-failed .orb-float {
    animation-duration: 6s;
  }
  /* A reply to read: a hop and a wiggle every few seconds. */
  .st-unread .orb-mood {
    animation: orb-nudge 3.2s ease-in-out infinite;
  }
  /* Asleep: slow breathing, no blinking, the ring dims. */
  .st-sleeping .orb-float {
    animation: orb-breathe 4.6s ease-in-out infinite;
  }
  .st-sleeping .orb-blink {
    animation: none;
  }
  .st-sleeping .orb-head {
    animation: none;
    transform: translateY(1px);
  }
  .st-sleeping .orb-ring {
    opacity: 0.25;
  }
  .st-sleeping .orb-sparkle {
    animation: none;
    opacity: 0.35;
  }
  .st-spawn .orb-mood {
    animation: orb-spawn 1.1s cubic-bezier(0.25, 1.5, 0.5, 1);
  }
  .st-spawn .orb-sparkle {
    animation: orb-spin 0.7s linear 2;
  }
  .is-waking .orb-mood {
    animation: orb-stretch 1.2s cubic-bezier(0.3, 1.5, 0.5, 1);
  }

  /* Effects ------------------------------------------------------------ */
  .fx-think circle {
    animation: think-dot 1.8s ease-in-out infinite;
    opacity: 0;
  }
  .fx-think circle:nth-child(2) {
    animation-delay: 0.25s;
  }
  .fx-think circle:nth-child(3) {
    animation-delay: 0.5s;
  }
  .fx-zzz text {
    animation: zzz 3.6s ease-out infinite;
    opacity: 0;
  }
  .fx-zzz text:nth-child(2) {
    animation-delay: 1.2s;
  }
  .fx-zzz text:nth-child(3) {
    animation-delay: 2.4s;
  }
  .fx-sweat {
    animation: sweat 1.9s ease-in infinite;
  }
  .fx-burst path {
    animation: burst 0.9s cubic-bezier(0.2, 0.7, 0.3, 1) forwards;
    opacity: 0;
  }
  .ping-ring {
    animation: ping 1.6s ease-out infinite;
  }

  /* Chip on the orb (bottom right) while the bubble is folded ----------- */
  .orb-chip {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: flex;
    height: 17px;
    min-width: 17px;
    align-items: center;
    justify-content: center;
    padding: 0 3px;
    border-radius: 999px;
    background: #2d2d3d;
    color: #d4d4e0;
    box-shadow: 0 0 0 2px var(--c-rail);
    animation: pop-in 0.25s cubic-bezier(0.3, 1.5, 0.5, 1);
  }
  .orb-chip.warn {
    background: #4a3a12;
    color: var(--c-warn);
  }
  .dots {
    display: flex;
    gap: 1.5px;
  }
  .dots i {
    width: 2.6px;
    height: 2.6px;
    border-radius: 50%;
    background: currentColor;
    animation: dot 1.1s ease-in-out infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.15s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.3s;
  }

  /* Speech bubble (on <body>, beside the orb) --------------------------- */
  .orb-speech {
    position: fixed;
    z-index: 60;
    display: flex;
    max-width: 230px;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px 6px;
    border: 1px solid color-mix(in srgb, var(--orb) 35%, transparent);
    border-radius: 12px;
    border-top-left-radius: 4px;
    background: color-mix(in srgb, var(--orb) 10%, var(--c-surface));
    color: var(--c-text);
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.38);
    pointer-events: none;
    transform: translateY(-50%);
    transform-origin: left center;
    animation: speech-in 0.32s cubic-bezier(0.3, 1.5, 0.5, 1);
  }
  .orb-speech.left {
    transform: translate(-100%, -50%);
    transform-origin: right center;
    border-top-left-radius: 12px;
    border-top-right-radius: 4px;
  }
  .orb-speech.warn {
    border-color: color-mix(in srgb, var(--c-warn) 50%, transparent);
  }
  .orb-speech.good {
    border-color: color-mix(in srgb, var(--c-success) 55%, transparent);
  }
  .orb-speech.bad {
    border-color: color-mix(in srgb, var(--c-danger, #f87171) 55%, transparent);
  }
  .speech-line {
    font-size: 12.5px;
    font-weight: 600;
    line-height: 1.3;
    white-space: nowrap;
    animation: line-in 0.3s ease-out;
  }
  .speech-detail {
    overflow: hidden;
    font-size: 11px;
    line-height: 1.3;
    color: var(--c-faint);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @keyframes orb-float {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-1.6px); }
  }
  @keyframes orb-breathe {
    0%, 100% { transform: translateY(0.6px) scale(1, 1); }
    50% { transform: translateY(0) scale(1.025, 0.985); }
  }
  @keyframes orb-blink {
    0%, 93%, 100% { transform: scaleY(1); }
    95% { transform: scaleY(0.12); }
    97% { transform: scaleY(1); }
  }
  @keyframes orb-twinkle {
    0%, 100% { transform: scale(0.75); opacity: 0.7; }
    50% { transform: scale(1.15); opacity: 1; }
  }
  @keyframes orb-spin {
    to { transform: rotate(360deg) scale(1.1); }
  }
  @keyframes orb-hop {
    0%, 100% { transform: translateY(0) scale(1, 1); }
    20% { transform: translateY(1px) scale(1.06, 0.94); }
    50% { transform: translateY(-6px) scale(0.96, 1.04); }
    80% { transform: translateY(0) scale(1.03, 0.97); }
  }
  @keyframes orb-nudge {
    0%, 62%, 100% { transform: none; }
    68% { transform: translateY(1px) scale(1.05, 0.95); }
    76% { transform: translateY(-4px) rotate(-6deg); }
    84% { transform: translateY(0) rotate(5deg); }
    91% { transform: rotate(-2deg); }
  }
  @keyframes orb-effort {
    0%, 100% { transform: scale(1, 1); }
    50% { transform: scale(1.02, 0.98); }
  }
  @keyframes orb-stretch {
    0% { transform: scale(1, 1); }
    30% { transform: translateY(-3px) scale(0.92, 1.1); }
    60% { transform: scale(1.05, 0.95); }
    100% { transform: none; }
  }
  @keyframes orb-sink {
    to { transform: translateY(2px) scale(0.97); }
  }
  @keyframes orb-shiver {
    0%, 100% { transform: none; }
    25% { transform: translateX(-0.8px); }
    75% { transform: translateX(0.8px); }
  }
  @keyframes orb-spawn {
    0% { transform: scale(0) rotate(-90deg); opacity: 0; }
    55% { transform: scale(1.15) rotate(8deg); opacity: 1; }
    75% { transform: translateY(-4px) scale(0.97); }
    100% { transform: none; }
  }
  @keyframes orb-look-around {
    0%, 30%, 100% { translate: 0 0; }
    34%, 44% { translate: -2.2px 0.3px; }
    50%, 58% { translate: 2px -1.2px; }
    63% { translate: 0 0; }
  }
  @keyframes orb-head-turn {
    0%, 62%, 100% { transform: none; }
    68%, 78% { transform: translateX(-3px) scaleX(0.92); }
    84% { transform: translateX(1.2px) scaleX(0.97); }
    88% { transform: none; }
  }
  @keyframes orb-head-busy {
    0%, 100% { transform: none; }
    30% { transform: translateX(1.4px) scaleX(0.97); }
    60% { transform: translateX(-1.4px) scaleX(0.97); }
  }
  @keyframes orb-scan {
    0%, 100% { transform: translateX(-2.4px) scaleX(0.93); }
    50% { transform: translateX(2.4px) scaleX(0.93); }
  }
  @keyframes orb-read {
    0%, 100% { transform: translateY(0.4px); }
    50% { transform: translateY(1.4px); }
  }
  @keyframes orb-lean {
    0%, 100% { transform: translateX(1px) rotate(4deg); }
    50% { transform: translateX(2px) rotate(7deg); }
  }
  @keyframes orb-ponder {
    0%, 100% { transform: translate(0.6px, -0.6px); }
    50% { transform: translate(1.6px, -1.2px); }
  }
  /* The face slides off one side and shrinks (the back of the head shows),
     then comes back round from the other side. */
  @keyframes orb-turnaround {
    0% { transform: none; opacity: 1; }
    22% { transform: translateX(11px) scaleX(0.25); opacity: 0; }
    23%, 47% { transform: translateX(-11px) scaleX(0.25); opacity: 0; }
    70% { transform: translateX(1.5px) scaleX(1.03); opacity: 1; }
    80%, 100% { transform: none; opacity: 1; }
  }
  @keyframes orb-bob {
    0%, 100% { transform: none; }
    45% { transform: translateY(-2.5px); }
  }
  @keyframes think-dot {
    0% { opacity: 0; transform: translateY(2px) scale(0.6); }
    35%, 70% { opacity: 0.95; transform: translateY(0) scale(1); }
    100% { opacity: 0; transform: translateY(-2px) scale(0.9); }
  }
  @keyframes zzz {
    0% { opacity: 0; transform: translate(0, 0) scale(0.6); }
    25% { opacity: 0.9; }
    100% { opacity: 0; transform: translate(9px, -14px) scale(1.25); }
  }
  @keyframes sweat {
    0% { opacity: 0; transform: translateY(-2px); }
    20%, 70% { opacity: 0.95; }
    100% { opacity: 0; transform: translateY(5px); }
  }
  @keyframes burst {
    0% { opacity: 1; transform: translate(0, 0) scale(0.3) rotate(0deg); }
    70% { opacity: 1; }
    100% { opacity: 0; transform: translate(var(--dx), var(--dy)) scale(1) rotate(90deg); }
  }
  @keyframes ping {
    0% { transform: scale(1); opacity: 0.9; }
    100% { transform: scale(2.2); opacity: 0; }
  }
  @keyframes pop-in {
    from { transform: scale(0); }
    to { transform: scale(1); }
  }
  @keyframes speech-in {
    from { opacity: 0; scale: 0.6; }
    to { opacity: 1; scale: 1; }
  }
  @keyframes line-in {
    from { opacity: 0; translate: 0 3px; }
    to { opacity: 1; translate: 0 0; }
  }
  @keyframes dot {
    0%, 100% { opacity: 0.35; transform: translateY(0); }
    40% { opacity: 1; transform: translateY(-1.5px); }
  }

  /* Motion setting (Settings → Appearance): "system" follows Windows'
     reduce-motion signal, "off" always stops the orbs. */
  @media (prefers-reduced-motion: reduce) {
    :global(:root[data-motion='system']) .orb *,
    :global(:root[data-motion='system']) .orb-chip *,
    :global(:root[data-motion='system']) .orb-speech {
      animation: none !important;
      transition: none !important;
    }
  }
  :global(:root[data-motion='off']) .orb *,
  :global(:root[data-motion='off']) .orb-chip *,
  :global(:root[data-motion='off']) .orb-speech {
    animation: none !important;
    transition: none !important;
  }
</style>
