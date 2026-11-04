<script lang="ts">
  // Approvals from agents you aren't looking at (e.g. one Iris put on a task)
  // pop up here, so you never have to go hunting through other agents' chats.
  import { Check, ExternalLink, ShieldAlert, ShieldCheck, X } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { tip } from '../tooltip';
  import AgentAvatar from './AgentAvatar.svelte';
  import DiffText from './DiffText.svelte';

  const openThread = $derived(app.view.kind === 'agent' ? app.view.threadId : null);
  // The open chat shows its own approval card; everything else pops up.
  const popups = $derived(app.pendingApprovals.filter((a) => a.thread_id !== openThread).slice(0, 3));
  const more = $derived(app.pendingApprovals.filter((a) => a.thread_id !== openThread).length - popups.length);

  function commandOf(args: string): string {
    try {
      const v = JSON.parse(args || '{}') as Record<string, unknown>;
      return typeof v.command === 'string' ? v.command : JSON.stringify(v, null, 2);
    } catch {
      return args;
    }
  }
</script>

{#if popups.length}
  <div class="fixed right-3 bottom-24 z-40 flex w-[min(23rem,calc(100vw-1.5rem))] flex-col gap-2.5 md:right-5">
    {#each popups as a (a.id)}
      {@const agent = app.agent(a.agent_id)}
      <div class="rounded-2xl border border-warn/40 bg-elevated p-3.5 shadow-[var(--shadow-pop)] animate-pop-in">
        <div class="flex items-center gap-2.5">
          {#if agent}<AgentAvatar {agent} size={26} waiting />{/if}
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5 text-[13px] font-semibold text-warn">
              <ShieldAlert size={14} /> {agent?.name ?? 'An agent'} needs your approval
            </div>
            <div class="truncate text-[11.5px] text-faint">{a.summary}</div>
          </div>
          <button
            class="rounded-md p-1.5 text-faint hover:bg-hover hover:text-text"
            onclick={() => app.openThread(a.agent_id, a.thread_id)}
            use:tip={'Open the conversation'}
            aria-label="Open the conversation"
          >
            <ExternalLink size={14} />
          </button>
        </div>
        {#if a.detail}
          <div class="mt-2.5"><DiffText text={a.detail} maxHeight="14rem" /></div>
        {:else}
          <pre
            class="selectable mt-2.5 max-h-36 overflow-auto rounded-lg border border-line bg-code px-2.5 py-2 font-mono text-[11.5px] whitespace-pre-wrap text-muted">{commandOf(
              a.arguments,
            )}</pre>
        {/if}
        <div class="mt-2.5 flex items-center gap-2">
          <button class="btn-primary py-1.5! text-[12.5px]!" onclick={() => app.decide(a.id, true)}>
            <Check size={13} /> Approve
          </button>
          <button class="btn-outline py-1.5! text-[12.5px]!" onclick={() => app.decide(a.id, false)}>
            <X size={13} /> Deny
          </button>
          {#if agent && a.tool === 'run_command' && !app.autoFor(agent)}
            {@const target = agent.created_by ? (app.agent(agent.created_by) ?? agent) : agent}
            <button
              class="ml-auto flex items-center gap-1 rounded-lg px-2 py-1.5 text-[12px] text-accent-hi hover:bg-hover"
              onclick={async () => {
                await app.setAgentAuto(target.id, 'careful');
                await app.decide(a.id, true);
              }}
              use:tip={`Approve this one and turn on Auto for ${target.name}${target.id !== agent.id ? "'s team" : ''}: a safety check approves safe commands from now on`}
            >
              <ShieldCheck size={13} /> Approve + Auto
            </button>
          {/if}
        </div>
      </div>
    {/each}
    {#if more > 0}
      <div class="self-end rounded-full bg-elevated px-3 py-1 text-[11.5px] text-faint shadow-[var(--shadow-pop)]">
        +{more} more waiting
      </div>
    {/if}
  </div>
{/if}
