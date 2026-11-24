<script lang="ts">
  // An agent's behind-the-scenes work (commands, files, searches, or for Iris
  // the team: creating agents, handing out tasks, waiting on them) folded into
  // one small box with a live status line. Click it to see every step.
  import { ChevronRight, LoaderCircle, ShieldAlert, Users, Wrench } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import type { Round } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import Markdown from './Markdown.svelte';
  import Thinking from './Thinking.svelte';
  import ToolCard from './ToolCard.svelte';

  let { rounds, live }: { rounds: Round[]; live: boolean } = $props();
  let open = $state(false);

  const steps = $derived(rounds.flatMap((r) => r.tools));
  const argOf = (args: string, key: string) => {
    try {
      const v = (JSON.parse(args || '{}') as Record<string, unknown>)[key];
      return typeof v === 'string' ? v.trim() : '';
    } catch {
      return '';
    }
  };
  // The agents put to work, in order, as they exist now (one-offs may be gone).
  const helpers = $derived.by(() => {
    const names: string[] = [];
    for (const s of steps) {
      const name = s.name === 'delegate_task' ? argOf(s.arguments, 'agent') : s.name === 'create_agent' ? argOf(s.arguments, 'name') : '';
      if (name && !names.some((n) => n.toLowerCase() === name.toLowerCase())) names.push(name);
    }
    return names.map((name) => ({
      name,
      agent: app.agents.find((a) => a.name.toLowerCase() === name.toLowerCase()),
    }));
  });
  const waitingOn = $derived(helpers.find((h) => h.agent && app.isWaiting(h.agent.id)));
  const workingOn = $derived(helpers.filter((h) => h.agent && app.isWorking(h.agent.id)));
  const lastStep = $derived(steps[steps.length - 1]);

  // What the steps were, in words: "9 commands, 1 file written, 2 searches".
  const KINDS: Record<string, [string, string]> = {
    run_command: ['command', 'commands'],
    write_file: ['file written', 'files written'],
    code_write: ['file written', 'files written'],
    code_edit: ['edit', 'edits'],
    read_file: ['file read', 'files read'],
    code_read: ['file read', 'files read'],
    list_files: ['folder listed', 'folders listed'],
    web_search: ['search', 'searches'],
    web_fetch: ['page read', 'pages read'],
    browser_open: ['page opened', 'pages opened'],
    browser_read: ['page read', 'pages read'],
    search_chats: ['chat search', 'chat searches'],
  };
  const tally = $derived.by(() => {
    const counts = new Map<string, [number, [string, string]]>();
    for (const s of steps) {
      const kind = KINDS[s.name] ?? ['other step', 'other steps'];
      const key = kind[0];
      counts.set(key, [(counts.get(key)?.[0] ?? 0) + 1, kind]);
    }
    return [...counts.values()].map(([n, [one, many]]) => `${n} ${n === 1 ? one : many}`).join(', ');
  });
  const failed = $derived(steps.filter((s) => s.status === 'error' || s.status === 'denied').length);
  // Still going: this reply is streaming, or a helper it started is busy.
  const active = $derived(live || !!waitingOn || workingOn.length > 0);
  const title = $derived(
    helpers.length
      ? `${helpers.map((h) => h.name).join(', ')} ${active ? (helpers.length > 1 ? 'are' : 'is') + ' on it' : helpers.length > 1 ? 'helped' : 'helped out'}`
      : active
        ? (lastStep?.summary ?? 'Working…')
        : steps.length === 1
          ? (lastStep?.summary ?? 'Done')
          : `Worked through ${steps.length} steps`,
  );
  const status = $derived.by(() => {
    if (waitingOn) return `${waitingOn.name} needs your approval`;
    if (workingOn.length) {
      const h = workingOn[0];
      const what = h.agent ? app.activityOf(h.agent.id) : '';
      return `${h.name} is working${what && what !== 'thinking' ? ` · ${what.replace(/_/g, ' ')}` : '…'}`;
    }
    if (live) return helpers.length ? (lastStep?.summary ?? 'Working…') : `${steps.length} ${steps.length === 1 ? 'step' : 'steps'} so far · ${tally}`;
    if (!helpers.length && steps.length === 1) return failed ? 'Failed' : 'Done';
    if (!helpers.length) return `${tally}${failed ? ` · ${failed} failed` : ''}`;
    return `${steps.length} ${steps.length === 1 ? 'step' : 'steps'}`;
  });
</script>

<div class="my-2.5 rounded-xl border bg-surface/60 transition {waitingOn ? 'border-warn/45' : 'border-line hover:border-line-strong'}">
  <button class="flex w-full items-center gap-2.5 px-3 py-2.5 text-left" onclick={() => (open = !open)} aria-expanded={open}>
    {#if helpers.length}
      <span class="flex shrink-0 -space-x-2">
        {#each helpers.slice(0, 3) as h (h.name)}
          {#if h.agent}
            <AgentAvatar
              agent={h.agent}
              size={24}
              working={live && app.isWorking(h.agent.id)}
              waiting={app.isWaiting(h.agent.id)}
            />
          {:else}
            <span class="flex h-6 w-6 items-center justify-center rounded-full bg-accent-soft text-accent-hi"><Users size={12} /></span>
          {/if}
        {/each}
      </span>
    {:else}
      <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-accent-soft text-accent-hi"><Wrench size={13} /></span>
    {/if}
    <span class="min-w-0 flex-1">
      <span class="block truncate text-[13px] font-medium text-text">{title}</span>
      <span class="block truncate text-[11.5px] {waitingOn ? 'text-warn' : 'text-faint'}">
        {#if active && !waitingOn}<span class="shimmer">{status}</span>{:else}{status}{/if}
      </span>
    </span>
    {#if waitingOn}
      <ShieldAlert size={15} class="shrink-0 text-warn" />
    {:else if active}
      <LoaderCircle size={14} class="shrink-0 animate-spin text-accent-hi" />
    {/if}
    <ChevronRight size={14} class="shrink-0 text-faint transition-transform {open ? 'rotate-90' : ''}" />
  </button>
  {#if open}
    <div class="border-t border-line px-3 py-2.5 animate-fade-in">
      {#each rounds as round, i (i)}
        {#if round.reasoning}<Thinking text={round.reasoning} live={false} />{/if}
        {#if round.content}<div class="text-[13.5px] text-muted"><Markdown text={round.content} /></div>{/if}
        {#if round.tools.length}
          <div class="my-2 space-y-1.5">
            {#each round.tools as step (step.id)}<ToolCard {step} />{/each}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>
