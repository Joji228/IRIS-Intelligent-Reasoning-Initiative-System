<script lang="ts">
  import { Archive, ArrowDown, ChevronLeft, CornerDownRight, Ellipsis, ListOrdered, LoaderCircle, Paperclip, Pencil, X } from '@lucide/svelte';
  import { tip } from '../tooltip';
  import { dayLabel, newDay } from '../time';
  import AutoPicker from './AutoPicker.svelte';
  import ContextRing from './ContextRing.svelte';
  import ModelPicker from './ModelPicker.svelte';
  import { tick } from 'svelte';
  import { app } from '../app.svelte';
  import AgentAvatar from './AgentAvatar.svelte';
  import CreditBadge from './CreditBadge.svelte';
  import Composer from './Composer.svelte';
  import Markdown from './Markdown.svelte';
  import MessageItem from './MessageItem.svelte';

  const agent = $derived(app.currentAgent);
  const threadId = $derived(app.view.kind === 'agent' ? app.view.threadId : null);
  const running = $derived(app.isRunning(threadId));
  // How full this conversation is: the latest reply that reported it.
  const contextUsed = $derived.by(() => {
    for (let i = app.messages.length - 1; i >= 0; i--) {
      const n = app.messages[i].context_tokens;
      if (n != null) return n;
    }
    return 0;
  });

  // Where older messages were compacted into a summary (shown as a divider).
  const summaryUntil = $derived(app.thread?.summary_until ?? 0);
  const firstKept = $derived(summaryUntil ? app.messages.findIndex((m) => m.created_at > summaryUntil) : -1);
  let showSummary = $state(false);

  let scroller = $state<HTMLDivElement>();
  let content = $state<HTMLDivElement>();
  let composer = $state<Composer>();
  // Follow the conversation (new text, tool steps, the queue or the keyboard
  // taking space) until the user scrolls up to read; back at the bottom, it
  // follows again.
  let pinned = $state(true);
  // The "Latest" button: only when the view really is away from the bottom.
  let away = $state(false);
  let lastTop = 0;

  const atBottom = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;

  function toBottom() {
    if (!scroller) return;
    scroller.scrollTop = scroller.scrollHeight;
    lastTop = scroller.scrollTop;
    away = false;
  }

  function onscroll() {
    if (!scroller) return;
    const bottom = atBottom();
    // Only the user moves the view up; growing content never does.
    if (bottom) pinned = true;
    else if (scroller.scrollTop < lastTop - 2) pinned = false;
    lastTop = scroller.scrollTop;
    away = !bottom;
  }

  // Any change in size, of the messages or of the visible area, keeps the
  // latest text in view while pinned.
  $effect(() => {
    if (!scroller || !content) return;
    const observer = new ResizeObserver(() => {
      if (pinned) toBottom();
      else away = !atBottom();
    });
    observer.observe(scroller);
    observer.observe(content);
    return () => observer.disconnect();
  });

  // Jump to the bottom and focus the input whenever another thread opens.
  $effect(() => {
    void threadId;
    pinned = true;
    tick().then(() => {
      toBottom();
      composer?.focus();
    });
  });

  // Sending a message always brings the view back down.
  $effect(() => {
    const last = app.messages[app.messages.length - 1];
    if (last?.role === 'user') {
      pinned = true;
      tick().then(toBottom);
    }
  });

  // Starters that fit the agent: Iris runs the team; a specialist gets asked
  // to do the job it was made for.
  const suggestions = $derived(
    agent?.role === 'new'
      ? [
          'Keep an eye on this server: disk space, updates and errors.',
          'Check every morning whether my websites are up and fast.',
          'Research topics in depth and give me sourced summaries.',
          'Help me write and polish emails.',
        ]
      : agent?.role === 'main'
      ? [
          'What can you and the team do for me?',
          'What do you remember about me?',
          'Put a small team on researching a topic for me.',
          'Which model are you running on?',
        ]
      : [
          'What can you help me with?',
          ...(agent?.description ? [`Go ahead and do your job: ${agent.description}`] : []),
          'What do you remember from earlier?',
        ],
  );
</script>

{#if agent}
  <section class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex h-14 shrink-0 items-center gap-3 border-b border-line pr-3 pl-1.5 md:px-6">
      <button class="-mr-1.5 rounded-lg p-1.5 text-muted hover:bg-hover hover:text-text md:hidden" onclick={() => app.goHome()} aria-label="Back">
        <ChevronLeft size={22} />
      </button>
      <AgentAvatar
        {agent}
        size={30}
        working={app.isWorking(agent.id)}
        waiting={app.isWaiting(agent.id)}
        mood={app.moods[agent.id]}
        activity={app.activityOf(agent.id)}
      />
      <div class="min-w-[5.5rem] flex-1">
        <div class="truncate text-[14px] font-semibold">{agent.name}</div>
        <div class="truncate text-[11.5px] text-faint">
          {#if app.isWaiting(agent.id)}
            <span class="font-medium text-warn">Waiting for your approval</span>
          {:else if app.isWorking(agent.id)}
            <span class="shimmer" title={app.detailOf(agent.id)}>{app.statusLine(agent.id)}</span>
          {:else}
            {agent.description}
          {/if}
        </div>
      </div>
      <!-- Model, thinking effort, auto mode and how full the chat is -->
      <div class="flex min-w-0 shrink items-center gap-0.5 rounded-full border border-line bg-surface/60 p-0.5">
        <ModelPicker {agent} disabled={app.conn !== 'online'} below />
        <AutoPicker {agent} below />
        <div class="px-1 max-sm:hidden"><ContextRing used={contextUsed} agentName={agent.name} model={agent.provider && agent.model ? agent.model : undefined} below /></div>
      </div>
      <div class="max-md:hidden"><CreditBadge /></div>
      <button
        class="rounded-lg p-2 text-faint transition hover:bg-hover hover:text-text max-md:hidden"
        onclick={() => (app.modal = { kind: 'agent', agentId: agent.id })}
        use:tip={'Edit agent'}
        aria-label="Edit agent"
      >
        <Pencil size={15} />
      </button>
      <button
        class="rounded-lg p-2 text-faint transition hover:bg-hover hover:text-text"
        onclick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          app.openAgentMenu(agent, r.right - 210, r.bottom + 4);
        }}
        use:tip={'More'}
        aria-label="More"
      >
        <Ellipsis size={16} />
      </button>
    </header>

    <!-- Messages -->
    <div class="relative min-h-0 flex-1">
    <div bind:this={scroller} {onscroll} class="h-full overflow-y-auto scroll-thin">
      <div bind:this={content} class="flex min-h-full flex-col">
      {#if app.loadingThread && app.messages.length === 0}
        <div class="flex flex-1 items-center justify-center text-faint">
          <LoaderCircle size={20} class="animate-spin" />
        </div>
      {:else if app.messages.length === 0}
        <div class="flex flex-1 flex-col items-center justify-center px-4 pb-10 text-center md:px-6 animate-fade-in">
          <AgentAvatar {agent} size={64} />
          <h2 class="mt-5 text-2xl font-semibold tracking-tight">{agent.role === 'new' ? 'What should I do?' : `How can ${agent.name} help?`}</h2>
          {#if agent.description}<p class="mt-1.5 text-[14px] text-muted">{agent.description}</p>{/if}
          <div class="mt-8 grid w-full max-w-xl gap-2.5 {suggestions.length === 4 ? 'grid-cols-1 sm:grid-cols-2' : 'grid-cols-1'}">
            {#each suggestions as s}
              <button
                class="rounded-xl border border-line px-4 py-3 text-left text-[13px] text-muted transition hover:border-line-strong hover:bg-surface hover:text-text"
                onclick={() => composer?.fill(s)}>{s}</button
              >
            {/each}
          </div>
        </div>
      {:else}
        <div class="mx-auto flex w-full max-w-4xl flex-col gap-5 px-3 pt-4 pb-6 md:px-6 md:pt-6">
          {#each app.messages as message, i (message.id)}
            {#if i === 0 || newDay(app.messages[i - 1].created_at, message.created_at)}
              <div class="flex justify-center">
                <span class="rounded-full border border-line bg-panel/85 px-3 py-0.5 text-[11.5px] font-medium text-muted backdrop-blur">
                  {dayLabel(message.created_at)}
                </span>
              </div>
            {/if}
            {#if i === firstKept && app.thread?.summary}
              <div class="flex flex-col items-center gap-2">
                <button
                  class="flex items-center gap-1.5 rounded-full bg-surface px-3 py-1 text-[11.5px] text-faint transition hover:text-text"
                  onclick={() => (showSummary = !showSummary)}
                >
                  <Archive size={12} /> Earlier messages were compacted to save context · {showSummary ? 'hide' : 'show'} summary
                </button>
                {#if showSummary}
                  <div class="w-full rounded-xl border border-line bg-surface/60 px-4 py-3 text-[13px] text-muted animate-fade-in">
                    <Markdown text={app.thread.summary} />
                  </div>
                {/if}
              </div>
            {/if}
            <MessageItem {message} {agent} isLast={i === app.messages.length - 1} />
          {/each}
        </div>
      {/if}
      </div>
    </div>
    {#if away && !pinned}
      <button
        class="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-line-strong bg-elevated px-3 py-1.5 text-[12px] text-muted shadow-[var(--shadow-pop)] transition hover:text-text animate-fade-in"
        onclick={() => {
          pinned = true;
          toBottom();
        }}
      >
        <ArrowDown size={13} /> Latest
      </button>
    {/if}
    </div>

    <!-- Composer -->
    <div class="shrink-0 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] md:px-6 md:pb-5">
      <div class="mx-auto w-full max-w-4xl md:px-6">
        {#if threadId && app.queues[threadId]?.length}
          {@const queue = app.queues[threadId]}
          <div class="mb-2 max-h-40 overflow-y-auto rounded-2xl border border-line bg-surface/80 p-1.5 scroll-thin animate-fade-in">
            <div class="flex items-center gap-1.5 px-2 pt-0.5 pb-1 text-[11.5px] text-faint">
              <ListOrdered size={12} />
              {queue.length} queued · {running ? `sends when ${agent.name} finishes` : 'sending…'}
            </div>
            {#each queue as q, i (q.id)}
              <div class="group flex items-center gap-2 rounded-xl px-2 py-1.5 hover:bg-hover">
                <span class="w-4 shrink-0 text-center text-[11px] text-faint tabular-nums">{i + 1}</span>
                <span class="min-w-0 flex-1 truncate text-[13px]">{q.content || 'Files'}</span>
                {#if q.attachments.length}
                  <span class="flex shrink-0 items-center gap-0.5 text-[11px] text-faint"><Paperclip size={11} />{q.attachments.length}</span>
                {/if}
                <button
                  class="flex shrink-0 items-center gap-1 rounded-full border border-accent/50 bg-accent-soft px-2.5 py-0.5 text-[11.5px] font-medium text-accent-hi transition hover:bg-accent hover:text-on-accent"
                  onclick={() => app.steerQueued(threadId, q.id)}
                  use:tip={`Stop what ${agent.name} is doing and answer this now`}
                  data-tip-side="top"
                >
                  <CornerDownRight size={12} /> Steer
                </button>
                <button
                  class="shrink-0 rounded-md p-1 text-faint transition hover:bg-hover hover:text-text"
                  onclick={() => app.removeQueued(threadId, q.id)}
                  use:tip={'Remove from the queue'}
                  data-tip-side="top"
                  aria-label="Remove from the queue"
                >
                  <X size={13} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
        <Composer
          bind:this={composer}
          placeholder="Message {agent.name}…"
          {running}
          disabled={app.conn !== 'online'}
        />
      </div>
    </div>
  </section>
{/if}
