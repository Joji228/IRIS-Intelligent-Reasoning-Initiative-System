<script lang="ts">
  // The call screen, the same design as the phone's: a slow glow in the agent's own color behind
  // everything (calm while it listens, greener while you talk, pulsing while it speaks, amber when
  // it needs your OK), the orb, a status line with your real microphone level, what you said and
  // what it said, and Mute / Stop / Hang up. Approvals show here and need a click; they are never
  // given by voice. Always dark, whatever the app's theme, like a phone call.
  //
  // Every change of state cross-fades: each look (calm, you talking, speaking, waiting) is its own
  // layer that runs all the time and only fades in or out, so nothing ever jumps mid-animation.
  import { ChevronDown, Mic, MicOff, Phone, ShieldAlert, Square, Check, X } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { fade, fly, scale } from 'svelte/transition';
  import { cubicOut, backOut } from 'svelte/easing';
  import { app } from '../app.svelte';
  import { call } from '../call/call.svelte';
  import AgentAvatar from './AgentAvatar.svelte';

  const agent = $derived(call.agent!);
  const phase = $derived(call.phase);
  const waiting = $derived(app.isWaiting(agent.id));
  const working = $derived(app.isWorking(agent.id));
  const approvals = $derived(Object.values(app.approvals).filter((a) => a.agent_id === agent.id));
  const canStop = $derived(working || call.speaker.speaking);

  let now = $state(Date.now());
  const timer = setInterval(() => (now = Date.now()), 1000);
  onDestroy(() => clearInterval(timer));
  const elapsed = $derived(clock(Math.max(0, Math.floor((now - call.startedAt) / 1000))));

  function clock(s: number) {
    const mm = String(Math.floor(s / 60) % 60).padStart(2, '0');
    const ss = String(s % 60).padStart(2, '0');
    return s >= 3600 ? `${Math.floor(s / 3600)}:${mm}:${ss}` : `${Math.floor(s / 60)}:${ss}`;
  }

  const status = $derived(
    call.trouble ??
      (waiting
        ? 'Waiting for your OK'
        : call.muted
          ? 'Muted'
          : phase === 'you'
            ? "I'm listening"
            : phase === 'thinking'
              ? working
                ? app.detailOf(agent.id) || app.statusLine(agent.id)
                : 'Thinking…'
              : phase === 'speaking'
                ? `${agent.name} is speaking`
                : 'Listening'),
  );
  const tone = $derived(call.trouble ? 'trouble' : waiting ? 'waiting' : call.muted ? 'muted' : phase);

  /** A button that was just used gives a short "pop" (also when used from the keyboard). */
  let popped = $state<string | null>(null);
  let popTimer: ReturnType<typeof setTimeout> | undefined;
  function pop(which: string) {
    popped = null;
    requestAnimationFrame(() => (popped = which));
    clearTimeout(popTimer);
    popTimer = setTimeout(() => (popped = null), 450);
  }
  function mute() {
    pop('mute');
    call.toggleMute();
  }
  function stop() {
    if (!canStop) return;
    pop('stop');
    void call.stopReply();
  }

  // Space: stop it talking (and its work); M: mute; Esc: back to the chat, the call goes on.
  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, [contenteditable="true"]')) return;
    if (e.key === ' ') {
      e.preventDefault();
      stop();
    } else if (e.key.toLowerCase() === 'm' && !e.ctrlKey && !e.metaKey) {
      mute();
    } else if (e.key === 'Escape') {
      call.open = false;
    }
  }

  // The orb is drawn to fit the room left between the header and the captions.
  let areaW = $state(300);
  let areaH = $state(300);
  const box = $derived(Math.max(80, Math.min(areaW, areaH - 12, 300)));
</script>

<svelte:window {onkeydown} />

<div
  class="call fixed inset-0 z-50 flex flex-col overflow-hidden"
  style="--tint: {agent.color || '#c4b5fd'}"
  data-tone={tone}
  role="dialog"
  aria-label="Call with {agent.name}"
  transition:fade|global={{ duration: 280 }}
>
  <!-- The glow: every layer always runs; the phase only fades them in and out -->
  <div class="glow" aria-hidden="true">
    <span class="blob a"></span>
    <span class="blob b"></span>
    <span class="blob c"></span>
    <span class="blob speak"></span>
    <span class="blob you"></span>
    <span class="blob warn"></span>
    <span class="floor"></span>
  </div>

  <div class="relative mx-auto flex h-full w-full max-w-[560px] flex-col px-5 pt-[max(env(safe-area-inset-top),0.5rem)] pb-[max(env(safe-area-inset-bottom),1.25rem)]" in:fly|global={{ y: 14, duration: 420, easing: cubicOut }}>
    <!-- Header: back to the chat (the call goes on), name, timer -->
    <header class="flex h-16 shrink-0 items-center">
      <button class="icon-btn" onclick={() => (call.open = false)} aria-label="Back to the chat (the call goes on)" title="Back to the chat · Esc">
        <ChevronDown size={24} />
      </button>
      <div class="flex-1 text-center">
        <div class="text-[18px] font-semibold text-[#ececf1]">{agent.name}</div>
        <div class="flex items-center justify-center gap-1.5 font-mono text-[13px] tabular-nums text-[#a3a3b8]">
          <span class="live-dot"></span>{elapsed}
        </div>
      </div>
      <div class="w-11"></div>
    </header>

    <!-- The orb gets the room that is left and never runs into anything -->
    <div class="relative flex min-h-0 flex-1 items-center justify-center" bind:clientWidth={areaW} bind:clientHeight={areaH}>
      <div class="orb-box" style="width: {box}px; height: {box}px">
        <!-- Rings: calm (agent color), speaking (faster), you talking (green); cross-faded -->
        <span class="ring calm outer"></span>
        <span class="ring calm inner"></span>
        <span class="ring speak outer"></span>
        <span class="ring speak inner"></span>
        <span class="ring you outer"></span>
        <span class="ring you inner"></span>
        <!-- Your voice, live: a soft halo that swells with your microphone level -->
        <span class="halo" style="--lvl: {call.muted ? 0 : call.level}"></span>
        <div class="orb">
          <AgentAvatar {agent} size={Math.round(box * 0.57)} {working} {waiting} activity={app.activityOf(agent.id)} />
        </div>
      </div>
    </div>

    <!-- Status, with your real microphone level -->
    <div class="mb-3 flex justify-center">
      <div class="chip" aria-live="polite">
        <span class="chip-icon">
          {#if tone === 'trouble' || tone === 'waiting'}
            <span in:scale={{ duration: 240, easing: backOut }}><ShieldAlert size={15} /></span>
          {:else if call.muted}
            <span in:scale={{ duration: 240, easing: backOut }}><MicOff size={15} /></span>
          {:else}
            <span class="meter" style="--lvl: {phase === 'speaking' ? 0.55 : call.level}" class:speaking={phase === 'speaking'} aria-hidden="true">
              <i></i><i></i><i></i><i></i><i></i>
            </span>
          {/if}
        </span>
        <span class="chip-text">
          {#key status}
            <span class={tone === 'thinking' ? 'shimmer' : ''} in:fly={{ y: 6, duration: 260, easing: cubicOut }}>{status}</span>
          {/key}
        </span>
      </div>
    </div>

    <!-- Approvals: in plain view, decided by a click -->
    {#if approvals.length}
      <div class="mb-3 max-h-72 space-y-3 overflow-y-auto scroll-thin">
        {#each approvals as a (a.id)}
          <div class="approval" in:fly={{ y: 12, duration: 320, easing: cubicOut }} out:fade={{ duration: 180 }}>
            <div class="flex items-center gap-2 text-[14px] font-semibold text-[#fbbf24]"><ShieldAlert size={16} /> Needs your OK</div>
            <div class="mt-2 font-mono text-[13px] break-words text-[#ececf1]">{a.summary}</div>
            <div class="mt-3 flex gap-2">
              <button class="ok" onclick={() => app.decide(a.id, true)}><Check size={16} /> Approve</button>
              <button class="no" onclick={() => app.decide(a.id, false)}><X size={16} /> Deny</button>
            </div>
            <div class="mt-2 text-[12px] text-[#6c6c82]">Approvals are clicked, never spoken.</div>
          </div>
        {/each}
      </div>
    {/if}

    <!-- What was said, in the order it happened; old lines fade out as new ones rise in -->
    <div class="captions {approvals.length ? 'max-h-32' : 'max-h-60'} overflow-y-auto scroll-thin">
      <div class="stack">
        {#key call.youSaid}
          {#if call.youSaid}
            <div class="text-center" in:fly={{ y: 8, duration: 300, easing: cubicOut }} out:fade={{ duration: 150 }}>
              <div class="text-[11.5px] text-[#6c6c82]">You said</div>
              <div class="text-[15px] text-[#a3a3b8]">“{call.youSaid}”</div>
            </div>
          {/if}
        {/key}
      </div>
      <div class="stack mt-2.5">
        {#key call.speaker.nowSaying}
          {#if call.speaker.nowSaying}
            <div class="text-center text-[20px] leading-[1.4] font-medium text-[#ececf1]" in:fly={{ y: 10, duration: 340, easing: cubicOut }} out:fade={{ duration: 160 }}>
              {call.speaker.nowSaying}
            </div>
          {/if}
        {/key}
      </div>
    </div>

    <!-- Controls -->
    <div class="mt-5 flex shrink-0 justify-center gap-7">
      <button class="round" class:on={call.muted} class:pop={popped === 'mute'} onclick={mute} aria-pressed={call.muted} title={call.muted ? 'Unmute · M' : 'Mute · M'}>
        <span class="disc">
          {#key call.muted}
            <span class="grid place-items-center" in:scale={{ duration: 260, start: 0.6, easing: backOut }}>
              {#if call.muted}<MicOff size={26} />{:else}<Mic size={26} />{/if}
            </span>
          {/key}
        </span>
        <span class="lbl">{call.muted ? 'Unmute' : 'Mute'}</span>
      </button>
      <button class="round" class:pop={popped === 'stop'} disabled={!canStop} onclick={stop} title="Stop · Space">
        <span class="disc"><Square size={22} fill="currentColor" /></span>
        <span class="lbl">Stop</span>
      </button>
      <button class="round hang" onclick={() => call.hangUp()} title="Hang up">
        <span class="disc"><span class="hang-icon"><Phone size={26} /></span></span>
        <span class="lbl">Hang up</span>
      </button>
    </div>
    <div class="mt-3 flex justify-center gap-3 text-[11.5px] text-[#6c6c82] max-md:hidden">
      <span><kbd>Space</kbd> stop</span><span><kbd>M</kbd> mute</span><span><kbd>Esc</kbd> back to the chat</span><span>Headphones work best</span>
    </div>
  </div>
</div>

<style>
  .call {
    background: #07070b;
    color: #ececf1;
  }

  /* ---- the glow ---- */
  .glow {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .blob {
    position: absolute;
    border-radius: 50%;
    filter: blur(40px);
    transition: opacity 0.9s ease;
    will-change: transform, opacity;
  }
  .blob.a,
  .blob.speak {
    width: 110vmax;
    height: 110vmax;
    left: calc(50% - 55vmax);
    top: calc(34% - 55vmax);
    background: radial-gradient(circle, color-mix(in srgb, var(--tint) 34%, transparent), transparent 62%);
  }
  .blob.a {
    animation: drift-a 22s ease-in-out infinite;
  }
  .blob.speak {
    opacity: 0;
    animation: beat 0.82s ease-in-out infinite alternate;
  }
  .blob.b {
    width: 90vmax;
    height: 90vmax;
    left: calc(15% - 45vmax);
    top: calc(80% - 45vmax);
    background: radial-gradient(circle, rgb(139 92 246 / 0.24), transparent 62%);
    animation: drift-b 26s ease-in-out infinite;
  }
  .blob.c {
    width: 80vmax;
    height: 80vmax;
    left: calc(90% - 40vmax);
    top: calc(66% - 40vmax);
    background: radial-gradient(circle, rgb(96 165 250 / 0.16), transparent 62%);
    animation: drift-a 30s ease-in-out infinite reverse;
  }
  .blob.you,
  .blob.warn {
    width: 90vmax;
    height: 90vmax;
    left: calc(50% - 45vmax);
    top: calc(36% - 45vmax);
    opacity: 0;
  }
  .blob.you {
    background: radial-gradient(circle, rgb(52 211 153 / 0.3), transparent 60%);
    animation: beat 0.66s ease-in-out infinite alternate;
  }
  .blob.warn {
    background: radial-gradient(circle, rgb(251 191 36 / 0.2), transparent 60%);
    animation: beat 1.6s ease-in-out infinite alternate;
  }
  [data-tone='speaking'] .blob.speak,
  [data-tone='you'] .blob.you,
  [data-tone='waiting'] .blob.warn {
    opacity: 1;
  }
  [data-tone='muted'] .blob.a,
  [data-tone='muted'] .blob.b,
  [data-tone='muted'] .blob.c {
    opacity: 0.45;
  }
  .floor {
    position: absolute;
    inset: 62% 0 0 0;
    background: linear-gradient(to bottom, transparent, rgb(7 7 11 / 0.85));
  }
  @keyframes drift-a {
    0%, 100% { transform: translate(0, 0); }
    33% { transform: translate(8vw, 3vh); }
    66% { transform: translate(-6vw, -2vh); }
  }
  @keyframes drift-b {
    0%, 100% { transform: translate(0, 0); }
    50% { transform: translate(10vw, -3vh); }
  }
  @keyframes beat {
    from { scale: 1; }
    to { scale: 1.12; }
  }

  /* ---- the orb and its rings ---- */
  .orb-box {
    position: relative;
    display: grid;
    place-items: center;
    transition: width 0.4s cubic-bezier(0.22, 1, 0.36, 1), height 0.4s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .ring {
    position: absolute;
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.6s ease;
    will-change: transform, opacity;
  }
  .ring.outer { width: 80%; height: 80%; --amount: 1.2; --o: 0.12; }
  .ring.inner { width: 68%; height: 68%; --amount: 1.08; --o: 0.18; }
  .ring.calm { background: var(--tint); animation: pulse 2.6s ease-in-out infinite alternate; }
  .ring.speak { background: var(--tint); animation: pulse 0.85s ease-in-out infinite alternate; }
  .ring.you { background: #34d399; animation: pulse 0.65s ease-in-out infinite alternate; }
  .ring.calm { opacity: var(--o); }
  [data-tone='speaking'] .ring.calm,
  [data-tone='you'] .ring.calm { opacity: 0; }
  [data-tone='speaking'] .ring.speak { opacity: var(--o); }
  [data-tone='you'] .ring.you { opacity: var(--o); }
  [data-tone='muted'] .ring.calm { opacity: calc(var(--o) * 0.45); }
  @keyframes pulse {
    from { scale: 1; }
    to { scale: var(--amount); }
  }
  .halo {
    position: absolute;
    width: 62%;
    height: 62%;
    border-radius: 50%;
    background: radial-gradient(circle, rgb(52 211 153 / 0.35), transparent 70%);
    opacity: calc(var(--lvl) * 1.2);
    scale: calc(1 + var(--lvl) * 0.35);
    transition: opacity 0.12s linear, scale 0.12s linear;
    pointer-events: none;
  }
  .orb {
    position: relative;
    display: grid;
    place-items: center;
  }

  /* ---- status chip and level meter ---- */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    border-radius: 999px;
    border: 1px solid rgb(255 255 255 / 0.1);
    background: rgb(255 255 255 / 0.07);
    padding: 9px 16px;
    font-size: 14.5px;
    font-weight: 500;
    color: #a3a3b8;
    max-width: 100%;
    backdrop-filter: blur(8px);
    transition: color 0.4s ease, border-color 0.4s ease, background-color 0.4s ease;
  }
  [data-tone='you'] .chip { color: #34d399; border-color: rgb(52 211 153 / 0.3); }
  [data-tone='speaking'] .chip { color: #a78bfa; border-color: rgb(167 139 250 / 0.3); }
  [data-tone='waiting'] .chip { color: #fbbf24; border-color: rgb(251 191 36 / 0.35); }
  [data-tone='trouble'] .chip { color: #f87171; border-color: rgb(248 113 113 / 0.35); }
  [data-tone='muted'] .chip { color: #6c6c82; }
  .chip-icon,
  .chip-text {
    display: grid;
  }
  .chip-text > :global(*) {
    grid-area: 1 / 1;
  }
  .meter {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 16px;
  }
  .meter i {
    width: 3px;
    border-radius: 3px;
    background: currentColor;
    height: calc(3px + 13px * var(--lvl) * var(--k));
    transition: height 90ms linear;
  }
  .meter i:nth-child(1) { --k: 0.55; }
  .meter i:nth-child(2) { --k: 0.85; }
  .meter i:nth-child(3) { --k: 1; }
  .meter i:nth-child(4) { --k: 0.8; }
  .meter i:nth-child(5) { --k: 0.5; }
  .meter.speaking i {
    animation: bar 0.5s ease-in-out infinite alternate;
  }
  .meter.speaking i:nth-child(2n) { animation-duration: 0.38s; }
  .meter.speaking i:nth-child(3n) { animation-duration: 0.62s; }
  @keyframes bar {
    from { scale: 1 0.35; }
    to { scale: 1 1; }
  }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #34d399;
    animation: live 1.6s ease-in-out infinite;
  }
  @keyframes live {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }

  /* ---- captions: the old and new line share one spot while they cross-fade ---- */
  .stack {
    display: grid;
  }
  .stack > :global(*) {
    grid-area: 1 / 1;
  }

  /* ---- approvals ---- */
  .approval {
    border-radius: 22px;
    border: 1px solid rgb(251 191 36 / 0.45);
    background: rgb(21 21 29 / 0.85);
    padding: 16px 18px;
  }
  .approval button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 14px;
    padding: 9px 16px;
    font-size: 14px;
    font-weight: 500;
    transition: transform 0.18s cubic-bezier(0.34, 1.56, 0.64, 1), filter 0.2s ease, background-color 0.2s ease;
  }
  .approval button:hover { transform: translateY(-1px); filter: brightness(1.1); }
  .approval button:active { transform: scale(0.95); }
  .approval .ok { background: #8b5cf6; color: white; }
  .approval .no { border: 1px solid rgb(255 255 255 / 0.14); color: #ececf1; }
  .approval .no:hover { background: rgb(255 255 255 / 0.06); }

  /* ---- controls ---- */
  .icon-btn {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 999px;
    color: #a3a3b8;
    transition: background-color 0.2s ease, color 0.2s ease, transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .icon-btn:hover { background: rgb(255 255 255 / 0.07); color: #ececf1; transform: translateY(1px); }
  .icon-btn:active { transform: scale(0.92); }
  .round {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    border-radius: 999px;
    outline: none;
    -webkit-tap-highlight-color: transparent;
  }
  .round .disc {
    position: relative;
    display: grid;
    place-items: center;
    width: 68px;
    height: 68px;
    border-radius: 999px;
    border: 1px solid rgb(255 255 255 / 0.12);
    background: linear-gradient(rgb(255 255 255 / 0.12), rgb(255 255 255 / 0.06));
    color: #ececf1;
    box-shadow: 0 0 0 0 transparent;
    transition:
      transform 0.22s cubic-bezier(0.34, 1.56, 0.64, 1),
      background 0.3s ease,
      border-color 0.3s ease,
      box-shadow 0.3s ease,
      color 0.3s ease;
  }
  .round:hover:not(:disabled) .disc {
    transform: translateY(-2px) scale(1.06);
    background: linear-gradient(rgb(255 255 255 / 0.18), rgb(255 255 255 / 0.09));
    box-shadow: 0 10px 28px -10px rgb(0 0 0 / 0.6);
  }
  .round:active:not(:disabled) .disc {
    transform: scale(0.92);
    transition-duration: 0.08s;
  }
  .round:focus-visible .disc {
    box-shadow: 0 0 0 3px rgb(167 139 250 / 0.55);
  }
  .round:disabled { opacity: 0.38; cursor: default; }
  .round.on .disc {
    background: linear-gradient(#a78bfa, #8b5cf6);
    border-color: transparent;
    color: white;
    box-shadow: 0 8px 26px -8px rgb(139 92 246 / 0.7);
  }
  .round.hang .disc {
    background: linear-gradient(#fb7185, #ef4444);
    border-color: transparent;
    color: white;
    box-shadow: 0 8px 26px -10px rgb(239 68 68 / 0.75);
  }
  .round.hang:hover .disc {
    background: linear-gradient(#fda4af, #ef4444);
    box-shadow: 0 12px 34px -10px rgb(239 68 68 / 0.9);
  }
  .hang-icon {
    display: grid;
    rotate: 135deg;
    transition: rotate 0.35s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .round.hang:hover .hang-icon { rotate: 150deg; }
  /* The "pop": a ring that bursts out from the button when it's used. */
  .round .disc::after {
    content: '';
    position: absolute;
    inset: -1px;
    border-radius: 999px;
    border: 2px solid currentColor;
    opacity: 0;
    pointer-events: none;
  }
  .round.pop .disc::after {
    animation: burst 0.45s ease-out;
  }
  @keyframes burst {
    from { opacity: 0.55; scale: 1; }
    to { opacity: 0; scale: 1.45; }
  }
  .round .lbl {
    font-size: 12.5px;
    font-weight: 500;
    color: #a3a3b8;
    transition: color 0.2s ease;
  }
  .round:hover:not(:disabled) .lbl { color: #ececf1; }
  .round.hang .lbl { color: #f87171; }
  kbd {
    border: 1px solid rgb(255 255 255 / 0.14);
    border-bottom-width: 2px;
    border-radius: 5px;
    padding: 0 5px;
    font-family: inherit;
    font-size: 10.5px;
    color: #a3a3b8;
  }

  @media (prefers-reduced-motion: reduce) {
    .blob, .ring, .meter.speaking i, .live-dot { animation: none !important; }
  }
</style>
