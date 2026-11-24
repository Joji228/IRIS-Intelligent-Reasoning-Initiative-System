<script lang="ts">
  import { ArrowRight, Check, Plus, ShieldAlert, TriangleAlert, X } from '@lucide/svelte';
  import { flip } from 'svelte/animate';
  import { app } from '../app.svelte';
  import { greeting, relativeTime } from '../time';
  import AgentAvatar from './AgentAvatar.svelte';
  import { MAX_AGENTS, abilitiesOf } from '../types';

  const workingCount = $derived(app.agents.filter((a) => app.isWorking(a.id)).length);
  const lastActivity = (agentId: string) => app.recent.find((t) => t.agent_id === agentId);

  // ---- drag a card to reorder (Iris stays first; same order as the sidebar) ----
  // Mouse and pen only: on a touch screen a press on a card has to scroll the
  // page, so phones reorder from the sidebar.
  const cards: Record<string, HTMLElement> = {};
  let pressed: { id: string; x: number; y: number; gx: number; gy: number } | null = null;
  let dragging = $state<string | null>(null);
  let dragX = $state(0);
  let dragY = $state(0);
  let justDragged = false;
  // The grid's slots (Iris's excluded), measured once when a drag starts. The
  // cards slide between them with an animation, so measuring the cards
  // themselves mid-drag gives in-between positions and made them swap back and
  // forth; the slots don't move.
  let slots: DOMRect[] = [];

  const others = () => app.orderedAgents.filter((a) => a.role !== 'main').map((a) => a.id);
  const middle = (r: DOMRect): [number, number] => [r.left + r.width / 2, r.top + r.height / 2];

  function ondown(e: PointerEvent, id: string, main: boolean) {
    if (e.button !== 0 || main || e.pointerType === 'touch') return;
    const r = cards[id]?.getBoundingClientRect();
    if (!r) return;
    const [cx, cy] = middle(r);
    pressed = { id, x: e.clientX, y: e.clientY, gx: e.clientX - cx, gy: e.clientY - cy };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  // The dragged card sits under the pointer: its offset from its current slot.
  function follow(e: PointerEvent, index: number) {
    const slot = slots[index];
    if (!slot) return;
    const [cx, cy] = middle(slot);
    dragX = e.clientX - pressed!.gx - cx;
    dragY = e.clientY - pressed!.gy - cy;
  }

  function onmove(e: PointerEvent) {
    if (!pressed) return;
    if (!dragging) {
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 6) return;
      dragging = pressed.id;
      slots = others().map((id) => cards[id]?.getBoundingClientRect()).filter((r): r is DOMRect => !!r);
    }
    const id = dragging;
    const ids = others();
    const from = ids.indexOf(id);
    // Move only once the pointer is inside another slot (a little inset, so the
    // edges between slots don't flip it back and forth).
    const to = slots.findIndex(
      (r, i) =>
        i !== from &&
        e.clientX > r.left + 12 &&
        e.clientX < r.right - 12 &&
        e.clientY > r.top + 12 &&
        e.clientY < r.bottom - 12,
    );
    if (to >= 0 && from >= 0) {
      const next = ids.filter((x) => x !== id);
      next.splice(to, 0, id);
      app.setOrbOrder(next);
      follow(e, to);
    } else {
      follow(e, from);
    }
  }

  function onup() {
    justDragged = !!dragging;
    pressed = null;
    dragging = null;
    dragX = 0;
    dragY = 0;
    slots = [];
  }
</script>

<div class="h-full overflow-y-auto scroll-thin">
  <div class="mx-auto max-w-5xl px-4 pt-8 pb-16 md:px-10 md:pt-14">
    <!-- Greeting -->
    <div class="animate-fade-in">
      <div class="text-[13px] font-medium tracking-wide text-faint">I.R.I.S.</div>
      <h1 class="mt-1 text-[26px] font-semibold tracking-tight md:text-[32px]">
        {greeting()}<span class="rainbow-text">.</span>
      </h1>
      <p class="mt-1.5 text-[14px] text-muted">
        {app.agents.length}
        {app.agents.length === 1 ? 'agent' : 'agents'}
        {#if app.pendingApprovals.length > 0}
          · <span class="text-warn">{app.pendingApprovals.length} waiting for you</span>
        {/if}
        {#if workingCount > 0}
          · <span class="text-accent-hi">{workingCount} working right now</span>
        {:else if app.pendingApprovals.length === 0}
          · all quiet
        {/if}
      </p>
    </div>

    <!-- Approvals -->
    {#if app.pendingApprovals.length}
      <h2 class="mt-10 mb-3 flex items-center gap-2 text-[12px] font-semibold tracking-wider text-warn uppercase">
        <ShieldAlert size={14} /> Needs your attention
      </h2>
      <div class="space-y-2.5">
        {#each app.pendingApprovals as a (a.id)}
          {@const agent = app.agent(a.agent_id)}
          <div class="flex flex-wrap items-center gap-3 rounded-2xl border border-warn/35 bg-warn/[0.05] px-4 py-3 animate-fade-in">
            <AgentAvatar {agent} size={34} waiting />
            <button class="min-w-0 flex-1 basis-40 text-left" onclick={() => app.openThread(a.agent_id, a.thread_id)}>
              <div class="text-[13px] text-muted">{agent?.name ?? 'An agent'} wants to:</div>
              <div class="truncate font-mono text-[13px] text-text">{a.summary}</div>
            </button>
            <button class="btn-primary shrink-0" onclick={() => app.decide(a.id, true)}><Check size={14} /> Approve</button>
            <button class="btn-outline shrink-0" onclick={() => app.decide(a.id, false)}><X size={14} /> Deny</button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Problems: failed replies and routines -->
    {#if app.problems.length}
      <button
        class="mt-6 flex w-full items-center gap-3 rounded-2xl border border-danger/30 bg-danger-soft/50 px-4 py-3 text-left transition hover:border-danger/50 animate-fade-in"
        onclick={() => (app.modal = { kind: 'problems' })}
      >
        <TriangleAlert size={17} class="shrink-0 text-danger" />
        <span class="min-w-0 flex-1">
          <span class="block text-[13.5px] font-medium">
            {app.problems.length === 1 ? '1 problem needs a look' : `${app.problems.length} problems need a look`}
          </span>
          <span class="block truncate text-[12px] text-faint">{app.problems[0].title} · {app.problems[0].detail}</span>
        </span>
        <span class="shrink-0 text-[12.5px] font-medium text-danger">View</span>
      </button>
    {/if}

    <!-- Agents -->
    <h2 class="mt-12 mb-4 text-[12px] font-semibold tracking-wider text-faint uppercase">Your agents</h2>
    <div class="grid grid-cols-[repeat(auto-fill,minmax(min(250px,100%),1fr))] gap-3.5">
      {#each app.orderedAgents as agent (agent.id)}
        {@const working = app.isWorking(agent.id)}
        {@const waiting = app.isWaiting(agent.id)}
        {@const last = lastActivity(agent.id)}
        <button
          bind:this={cards[agent.id]}
          animate:flip={{ duration: agent.id === dragging ? 0 : 180 }}
          class="group flex flex-col rounded-2xl border border-line bg-surface p-4 text-left {dragging === agent.id
            ? 'relative z-10 scale-[1.03] cursor-grabbing border-accent/50 shadow-[0_18px_40px_rgb(0_0_0/0.4)] select-none'
            : 'transition hover:-translate-y-0.5 hover:border-line-strong hover:shadow-[0_10px_30px_rgb(0_0_0/0.25)]'}"
          style:translate={dragging === agent.id ? `${dragX}px ${dragY}px` : undefined}
          onpointerdown={(e) => ondown(e, agent.id, agent.role === 'main')}
          onpointermove={onmove}
          onpointerup={onup}
          onpointercancel={onup}
          onclick={() => {
            // The click that ends a drag doesn't open the chat.
            if (justDragged) {
              justDragged = false;
              return;
            }
            app.openAgent(agent.id);
          }}
          oncontextmenu={(e) => {
            e.preventDefault();
            app.openAgentMenu(agent, e.clientX, e.clientY);
          }}
        >
          <div class="flex items-start gap-3">
            <AgentAvatar
              {agent}
              size={42}
              {working}
              {waiting}
              mood={app.moods[agent.id]}
              activity={app.activityOf(agent.id)}
              unread={app.unread[agent.id]}
              sleepy={app.sleepy}
            />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[15px] font-semibold">{agent.name}</div>
              <div class="mt-0.5 flex items-center gap-1.5 text-[12px]">
                {#if waiting}
                  <span class="h-1.5 w-1.5 rounded-full bg-warn"></span>
                  <span class="font-medium text-warn">Waiting for you</span>
                {:else if working}
                  <span class="h-1.5 w-1.5 rounded-full bg-accent-hi"></span>
                  <span class="shimmer truncate font-medium" title={app.detailOf(agent.id)}>{app.statusLine(agent.id)}</span>
                {:else}
                  <span class="h-1.5 w-1.5 rounded-full bg-success/70"></span>
                  <span class="text-faint">Idle</span>
                {/if}
              </div>
            </div>
            <ArrowRight
              size={16}
              class="mt-1 text-faint opacity-0 transition group-hover:translate-x-0.5 group-hover:opacity-100"
            />
          </div>
          <p class="mt-3 line-clamp-2 min-h-[2.6em] text-[13px] leading-snug text-muted" title={agent.description || undefined}>
            {agent.description || 'No description yet.'}
          </p>
          <div class="mt-3 flex items-center justify-between border-t border-line pt-3 text-[11.5px] text-faint">
            <span class="truncate">{agent.role === 'main' ? 'Every ability' : abilitiesOf(agent.tools).join(' · ') || 'Built-in abilities'}</span>
            <span class="shrink-0">{last ? `Active ${relativeTime(last.updated_at)}` : 'New'}</span>
          </div>
        </button>
      {/each}

      {#if app.agents.length < MAX_AGENTS}
        <button
          class="flex min-h-[150px] flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-line-strong text-[13px] text-faint transition hover:border-accent/60 hover:bg-accent-soft hover:text-accent-hi"
          onclick={() => app.summonAgent()}
        >
          <Plus size={22} />
          New agent
          <span class="text-[11px]">{app.agents.length} of {MAX_AGENTS}</span>
        </button>
      {/if}
    </div>

  </div>
</div>
