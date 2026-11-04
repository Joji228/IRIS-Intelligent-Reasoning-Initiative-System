<script lang="ts">
  // The problems inbox: failed replies and routines, newest first, each with
  // Retry, a way to its chat, and Dismiss.
  import { CalendarClock, CircleCheck, MessageSquare, RotateCcw, X } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { relativeTime } from '../time';
  import type { Problem } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import Modal from './Modal.svelte';

  const close = () => (app.modal = null);
  let busy = $state<string | null>(null);

  async function retry(p: Problem) {
    busy = p.id;
    await app.retryProblem(p);
    busy = null;
    if (p.kind === 'reply') close();
  }

  function open(p: Problem) {
    close();
    void app.openAgent(p.agent_id);
  }
</script>

<Modal title="Problems" onclose={close} width={620}>
  {#if app.problems.length === 0}
    <div class="flex flex-col items-center gap-2 py-10 text-center">
      <CircleCheck size={28} class="text-success" />
      <div class="text-[14px] font-medium">Nothing needs a look</div>
      <div class="text-[12.5px] text-faint">Failed replies and routines show up here, with a way to try again.</div>
    </div>
  {:else}
    <div class="space-y-2">
      {#each app.problems as p (p.id)}
        {@const agent = app.agent(p.agent_id)}
        <div class="rounded-xl border border-danger/25 bg-danger-soft/40 p-3.5 animate-fade-in">
          <div class="flex items-start gap-3">
            {#if agent}<AgentAvatar {agent} size={32} mood="sad" />{/if}
            <div class="min-w-0 flex-1">
              <div class="flex items-baseline gap-2">
                <span class="truncate text-[13.5px] font-semibold">{p.title}</span>
                <span class="ml-auto shrink-0 text-[11px] text-faint">{relativeTime(p.created_at)}</span>
              </div>
              <div class="mt-1 flex items-center gap-1.5 text-[11.5px] text-faint">
                {#if p.kind === 'routine'}<CalendarClock size={12} /> Routine{:else}<MessageSquare size={12} /> Reply{/if}
                · {agent?.name ?? 'removed agent'}
              </div>
              {#if p.detail}
                <div class="selectable mt-2 line-clamp-4 rounded-lg bg-code px-2.5 py-1.5 font-mono text-[11.5px] break-words whitespace-pre-wrap text-muted">
                  {p.detail}
                </div>
              {/if}
              <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
                <button class="btn-outline h-8! px-3! text-[12.5px]!" onclick={() => retry(p)} disabled={busy === p.id || !agent}>
                  <RotateCcw size={13} /> Retry
                </button>
                {#if agent}
                  <button class="btn-ghost h-8! px-3! text-[12.5px]!" onclick={() => open(p)}>
                    <MessageSquare size={13} /> Open chat
                  </button>
                {/if}
                <button class="btn-ghost ml-auto h-8! px-2.5! text-[12.5px]!" onclick={() => app.dismissProblem(p.id)}>
                  <X size={13} /> Dismiss
                </button>
              </div>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
  {#snippet footer()}
    {#if app.problems.length > 1}
      <button class="btn-ghost" onclick={() => app.dismissProblem()}>Dismiss all</button>
    {/if}
    <button class="btn-primary" onclick={close}>Done</button>
  {/snippet}
</Modal>
