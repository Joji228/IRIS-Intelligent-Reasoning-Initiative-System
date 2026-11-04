<script lang="ts">
  import { PanelLeftClose, PanelLeftOpen, Plus, Search, Settings, TriangleAlert } from '@lucide/svelte';
  import { tick } from 'svelte';
  import { flip } from 'svelte/animate';
  import { app, RAIL_DEFAULT, RAIL_LIST, RAIL_LIST_DEFAULT } from '../app.svelte';
  import { listTime } from '../time';
  import { tip } from '../tooltip';
  import type { Agent } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import { MAX_AGENTS } from '../types';

  const isHome = $derived(app.view.kind === 'home');
  const activeAgentId = $derived(app.view.kind === 'agent' ? app.view.agentId : null);
  const main = $derived(app.orderedAgents.find((a) => a.role === 'main'));
  const others = $derived(app.orderedAgents.filter((a) => a.role !== 'main'));

  // Phones always get the narrow bar (the home screen sits beside it).
  let narrow = $state(typeof window !== 'undefined' && window.innerWidth < 768);
  $effect(() => {
    const q = matchMedia('(max-width: 767px)');
    const on = () => (narrow = q.matches);
    q.addEventListener('change', on);
    return () => q.removeEventListener('change', on);
  });
  // Wide enough, the bar becomes a chat list: name, last message, time.
  const list = $derived(!narrow && app.railWidth >= RAIL_LIST);
  const width = $derived(narrow ? RAIL_DEFAULT : app.railWidth);
  // The narrow bar's orbs grow and shrink with its width.
  const scale = $derived(Math.min(width, 150) / RAIL_DEFAULT);
  const px = (n: number) => Math.round(n * scale);

  const label = (agent: Agent) => {
    const waiting = app.isWaiting(agent.id);
    const working = app.isWorking(agent.id);
    return (
      (agent.role === 'main' ? `${agent.name} · Chief of Staff` : agent.role === 'temp' ? `${agent.name} · one-off` : agent.name) +
      (waiting ? ' · needs your approval' : working ? ' · working…' : '')
    );
  };

  // A list row's second line: what the agent is doing, or the last thing said.
  function subline(agent: Agent): { text: string; tone: 'busy' | 'warn' | 'plain' | 'me' } {
    if (app.isWaiting(agent.id)) return { text: 'Waiting for your approval', tone: 'warn' };
    if (app.isWorking(agent.id)) return { text: app.statusLine(agent.id), tone: 'busy' };
    const p = app.previews[agent.id];
    if (!p) return { text: agent.description || 'No messages yet', tone: 'plain' };
    const text = p.text
      .replace(/^\[Deep research\]\s*/, '')
      .replace(/[#*_`>|]+/g, '')
      .replace(/\s+/g, ' ')
      .trim();
    return { text, tone: p.role === 'user' ? 'me' : 'plain' };
  }

  // ---- drag to reorder (Iris stays first) ----
  // Pointer events rather than HTML drag and drop: the desktop window takes
  // those for dropping files.
  const slots: Record<string, HTMLElement> = {};
  let pressed: { id: string; y: number; grab: number } | null = null;
  let dragging = $state<string | null>(null);
  // How far the dragged orb sits from its slot: it follows the pointer exactly.
  let dragDy = $state(0);
  let justDragged = false;

  const centerOf = (id: string) => {
    const r = slots[id]?.getBoundingClientRect();
    return r ? r.top + r.height / 2 : 0;
  };

  function onpointerdown(e: PointerEvent, agent: Agent) {
    if (e.button !== 0 || agent.role === 'main') return;
    pressed = { id: agent.id, y: e.clientY, grab: e.clientY - centerOf(agent.id) };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onpointermove(e: PointerEvent) {
    if (!pressed) return;
    if (!dragging) {
      if (Math.abs(e.clientY - pressed.y) < 4) return;
      dragging = pressed.id;
    }
    const ids = others.map((a) => a.id);
    const rest = ids.filter((id) => id !== dragging);
    // Insert before the first orb whose middle is below the pointer.
    const at = rest.filter((id) => {
      const r = slots[id]?.getBoundingClientRect();
      return r && r.top + r.height / 2 < e.clientY;
    }).length;
    const next = [...rest];
    next.splice(at, 0, dragging);
    if (next.join() !== ids.join()) app.setOrbOrder(next);
    // Measure after the reorder: the slot may have moved.
    const id = dragging;
    void tick().then(() => {
      if (dragging === id && pressed) dragDy = e.clientY - pressed.grab - centerOf(id);
    });
    dragDy = e.clientY - pressed.grab - centerOf(dragging);
  }

  function onpointerup() {
    justDragged = !!dragging;
    pressed = null;
    dragging = null;
    dragDy = 0;
  }

  function open(agent: Agent) {
    // The click that ends a drag doesn't open the chat.
    if (justDragged) {
      justDragged = false;
      return;
    }
    app.openAgent(agent.id);
  }

  // ---- resizing the bar ----
  let resizing = $state(false);
  let resizeFrom = { x: 0, width: 0 };

  function startResize(e: PointerEvent) {
    resizing = true;
    resizeFrom = { x: e.clientX, width: app.railWidth };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function moveResize(e: PointerEvent) {
    if (resizing) app.setRailWidth(resizeFrom.width + e.clientX - resizeFrom.x, false);
  }
  function endResize() {
    if (!resizing) return;
    resizing = false;
    app.setRailWidth(app.railWidth);
  }
</script>

{#snippet orb(agent: Agent)}
  {@const active = activeAgentId === agent.id}
  {@const working = app.isWorking(agent.id)}
  {@const waiting = app.isWaiting(agent.id)}
  <span
    class="absolute top-1/2 left-0 w-1 -translate-y-1/2 rounded-r-full bg-text transition-all {active ? 'h-8' : 'h-0'}"
  ></span>
  <button
    class="touch-none rounded-[14px] {dragging === agent.id
      ? 'relative z-10 scale-110 cursor-grabbing drop-shadow-[0_8px_14px_rgb(0_0_0/0.45)]'
      : 'transition hover:scale-[1.04] active:scale-95'} {active ? '' : 'opacity-85 hover:opacity-100'}"
    style:translate={dragging === agent.id ? `0 ${dragDy}px` : undefined}
    onclick={() => open(agent)}
    onpointerdown={(e) => onpointerdown(e, agent)}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    oncontextmenu={(e) => {
      e.preventDefault();
      app.openAgentMenu(agent, e.clientX, e.clientY);
    }}
    use:tip={dragging || working || waiting ? '' : label(agent)}
    aria-label={agent.name}
  >
    <AgentAvatar
      {agent}
      size={px(agent.role === 'main' ? 48 : 42)}
      {working}
      {waiting}
      mood={app.moods[agent.id]}
      activity={app.activityOf(agent.id)}
      detail={app.detailOf(agent.id)}
      unread={app.unread[agent.id]}
      sleepy={app.sleepy}
      speech={!dragging}
    />
  </button>
{/snippet}

{#snippet row(agent: Agent)}
  {@const active = activeAgentId === agent.id}
  {@const working = app.isWorking(agent.id)}
  {@const waiting = app.isWaiting(agent.id)}
  {@const unread = app.unread[agent.id]}
  {@const line = subline(agent)}
  {@const p = app.previews[agent.id]}
  <button
    class="flex w-full touch-none items-center gap-3 rounded-xl px-2 py-2 text-left {dragging === agent.id
      ? 'relative z-10 cursor-grabbing bg-elevated shadow-[0_10px_24px_rgb(0_0_0/0.4)]'
      : 'transition'} {active ? 'bg-hover' : 'hover:bg-hover/60'}"
    style:translate={dragging === agent.id ? `0 ${dragDy}px` : undefined}
    onclick={() => open(agent)}
    onpointerdown={(e) => onpointerdown(e, agent)}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    oncontextmenu={(e) => {
      e.preventDefault();
      app.openAgentMenu(agent, e.clientX, e.clientY);
    }}
    title={working ? app.detailOf(agent.id) : undefined}
    aria-label={agent.name}
  >
    <AgentAvatar
      {agent}
      size={agent.role === 'main' ? 44 : 40}
      {working}
      {waiting}
      mood={app.moods[agent.id]}
      activity={app.activityOf(agent.id)}
      {unread}
      sleepy={app.sleepy}
    />
    <span class="min-w-0 flex-1">
      <span class="flex items-baseline gap-2">
        <span class="truncate text-[13.5px] {unread ? 'font-bold' : 'font-semibold'}">{agent.name}</span>
        <span class="ml-auto shrink-0 text-[11px] {unread ? 'font-medium text-accent-hi' : 'text-faint'}">
          {p ? listTime(p.created_at) : ''}
        </span>
      </span>
      <span class="mt-0.5 flex items-center gap-2">
        <span
          class="min-w-0 flex-1 truncate text-[12.5px] {line.tone === 'busy'
            ? 'shimmer font-medium'
            : line.tone === 'warn'
              ? 'font-medium text-warn'
              : unread
                ? 'text-text'
                : 'text-faint'}"
        >
          {#if line.tone === 'me'}<span class="text-muted">{'You: '}</span>{/if}{line.text}
        </span>
        {#if unread}<span class="h-2.5 w-2.5 shrink-0 rounded-full bg-accent shadow-[0_0_8px_var(--c-accent)]"></span>{/if}
      </span>
    </span>
  </button>
{/snippet}

<nav
  class="relative flex h-full shrink-0 flex-col border-r border-line bg-rail py-3 {list ? 'items-stretch px-2' : 'items-center'}"
  style:width="{width}px"
>
  <!-- Home -->
  {#if list}
    <div class="flex items-center gap-1 px-0.5">
      <button
        class="flex min-w-0 flex-1 items-center gap-2.5 rounded-xl px-1.5 py-1 text-left transition {isHome ? 'bg-hover' : 'hover:bg-hover/60'}"
        onclick={() => app.goHome()}
        aria-label="Home"
      >
        <img src="/iris.png" alt="" class="h-9 w-9 rounded-[11px]" />
        <span class="truncate text-[15px] font-semibold tracking-tight">I.R.I.S.</span>
      </button>
      <button
        class="rounded-lg p-2 text-faint transition hover:bg-hover hover:text-text"
        onclick={() => app.setRailWidth(RAIL_DEFAULT)}
        use:tip={'Back to orbs only'}
        aria-label="Collapse the list"
      >
        <PanelLeftClose size={17} />
      </button>
    </div>
  {:else}
    <div class="relative flex w-full justify-center">
      {#if isHome}<span class="absolute top-1/2 left-0 h-8 w-1 -translate-y-1/2 rounded-r-full bg-text"></span>{/if}
      <button
        class="rounded-[14px] p-0.5 transition hover:scale-[1.04] active:scale-95"
        onclick={() => app.goHome()}
        use:tip={'Home'}
        aria-label="Home"
      >
        <img src="/iris.png" alt="I.R.I.S." class="rounded-[14px]" style:width="{px(44)}px" style:height="{px(44)}px" />
      </button>
    </div>
  {/if}

  <div class="my-3 h-px shrink-0 bg-line-strong {list ? 'mx-2' : 'w-8 self-center'}"></div>

  <!-- Agents: Iris, then the rest in the user's order (drag to reorder) -->
  {#if list}
    <div class="flex w-full flex-1 flex-col gap-0.5 overflow-x-hidden overflow-y-auto pb-1 scroll-none">
      {#if main}
        {@render row(main)}
        {#if others.length}<div class="mx-3 my-1 h-px shrink-0 bg-line"></div>{/if}
      {/if}
      {#each others as agent (agent.id)}
        <div bind:this={slots[agent.id]} animate:flip={{ duration: agent.id === dragging ? 0 : 110 }}>
          {@render row(agent)}
        </div>
      {/each}
      {#if app.agents.length < MAX_AGENTS}
        <button
          class="group flex w-full items-center gap-3 rounded-xl px-2 py-2 text-left text-faint transition hover:bg-hover/60 hover:text-text"
          onclick={() => app.summonAgent()}
        >
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full border border-dashed border-line-strong transition group-hover:border-accent group-hover:text-accent-hi"
          >
            <Plus size={18} />
          </span>
          <span class="text-[13px]">New agent <span class="text-faint">· {app.agents.length} of {MAX_AGENTS}</span></span>
        </button>
      {/if}
    </div>
  {:else}
    <div
      class="flex w-full flex-1 flex-col items-center overflow-x-hidden overflow-y-auto pt-1 pb-1 scroll-none"
      style:gap="{px(10)}px"
    >
      {#if main}
        <div class="relative flex w-full justify-center">{@render orb(main)}</div>
        {#if others.length}<div class="h-px w-6 shrink-0 bg-line"></div>{/if}
      {/if}
      {#each others as agent (agent.id)}
        <div class="relative flex w-full justify-center" bind:this={slots[agent.id]} animate:flip={{ duration: agent.id === dragging ? 0 : 110 }}>
          {@render orb(agent)}
        </div>
      {/each}

      {#if app.agents.length < MAX_AGENTS}
        <button
          class="flex shrink-0 items-center justify-center rounded-full border border-dashed border-line-strong text-faint transition hover:border-accent hover:bg-accent-soft hover:text-accent-hi"
          style:width="{px(44)}px"
          style:height="{px(44)}px"
          onclick={() => app.summonAgent()}
          use:tip={`New agent (${app.agents.length} of ${MAX_AGENTS})`}
          aria-label="New agent"
        >
          <Plus size={px(20)} />
        </button>
      {:else}
        <span
          class="flex shrink-0 cursor-default items-center justify-center rounded-full border border-dashed border-line text-[11px] text-faint"
          style:width="{px(44)}px"
          style:height="{px(44)}px"
          use:tip={`The team is full (${MAX_AGENTS} agents)`}>{MAX_AGENTS}/{MAX_AGENTS}</span
        >
      {/if}
    </div>
  {/if}

  <!-- Problems inbox (only when something failed) -->
  {#if app.problems.length}
    <button
      class="relative mt-2 flex items-center gap-2 rounded-xl text-danger transition hover:bg-danger-soft {list
        ? 'mx-0.5 h-10 px-3 text-[13px] font-medium'
        : 'h-10 w-10 justify-center self-center'}"
      onclick={() => (app.modal = { kind: 'problems' })}
      use:tip={list ? '' : `${app.problems.length} problem${app.problems.length === 1 ? '' : 's'} need a look`}
      aria-label="Problems"
    >
      <TriangleAlert size={list ? 16 : 19} />
      {#if list}<span class="mr-auto">Problems</span>{/if}
      <span
        class="flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-danger px-1 text-[10.5px] font-bold text-white {list
          ? ''
          : 'absolute -top-0.5 -right-0.5'}">{app.problems.length}</span
      >
    </button>
  {/if}

  <!-- Bottom -->
  <div class="mt-3 flex {list ? 'items-center gap-1 px-0.5' : 'flex-col items-center gap-3'}">
    {#if !list && !narrow}
      <button
        class="flex h-10 w-10 items-center justify-center rounded-xl text-muted transition hover:bg-hover hover:text-text"
        onclick={() => app.setRailWidth(RAIL_LIST_DEFAULT)}
        use:tip={'Show as a chat list'}
        aria-label="Show as a chat list"
      >
        <PanelLeftOpen size={19} />
      </button>
    {/if}
    <button
      class="flex h-10 items-center justify-center gap-2 rounded-xl text-muted transition hover:bg-hover hover:text-text {list
        ? 'flex-1 px-3 text-[13px]'
        : 'w-10'}"
      onclick={() => (app.modal = { kind: 'search' })}
      use:tip={list ? '' : 'Search chats (Ctrl K)'}
      aria-label="Search chats"
    >
      <Search size={list ? 16 : 19} />
      {#if list}<span class="mr-auto">Search</span><kbd class="font-sans text-[10.5px] text-faint">Ctrl K</kbd>{/if}
    </button>
    <button
      class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-muted transition hover:bg-hover hover:text-text"
      onclick={() => (app.modal = { kind: 'settings' })}
      use:tip={'Settings'}
      aria-label="Settings"
    >
      <Settings size={list ? 17 : 19} />
    </button>
  </div>

  <!-- Drag the edge to resize: the orbs grow, and wide enough it becomes a chat list; double-click resets -->
  {#if !narrow}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute top-0 -right-1 z-20 h-full w-2 cursor-col-resize touch-none transition-colors {resizing
        ? 'bg-accent/40'
        : 'hover:bg-accent/25'}"
      onpointerdown={startResize}
      onpointermove={moveResize}
      onpointerup={endResize}
      onpointercancel={endResize}
      ondblclick={() => app.setRailWidth(RAIL_DEFAULT)}
      use:tip={'Drag to resize (wider shows a chat list) · double-click to reset'}
    ></div>
  {/if}
</nav>
