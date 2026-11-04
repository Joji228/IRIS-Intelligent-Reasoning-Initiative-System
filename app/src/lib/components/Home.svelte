<script lang="ts">
  import { ArrowRight, Check, Plus, ShieldAlert, TriangleAlert, X } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { greeting, relativeTime } from '../time';
  import AgentAvatar from './AgentAvatar.svelte';
  import { MAX_AGENTS, abilitiesOf } from '../types';

  const workingCount = $derived(app.agents.filter((a) => app.isWorking(a.id)).length);
  const lastActivity = (agentId: string) => app.recent.find((t) => t.agent_id === agentId);
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
          class="group flex flex-col rounded-2xl border border-line bg-surface p-4 text-left transition hover:-translate-y-0.5 hover:border-line-strong hover:shadow-[0_10px_30px_rgb(0_0_0/0.25)]"
          onclick={() => app.openAgent(agent.id)}
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
          <p class="mt-3 line-clamp-2 min-h-[2.6em] text-[13px] leading-snug text-muted">
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
