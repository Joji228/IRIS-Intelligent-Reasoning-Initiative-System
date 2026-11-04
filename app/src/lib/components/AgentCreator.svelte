<script lang="ts">
  // "+" opens this: describe the agent in a sentence and I.R.I.S. designs it
  // (name, tools, instructions). The full form is still there for hand setup.
  import { LoaderCircle, Sparkles } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import Modal from './Modal.svelte';

  let request = $state('');
  let input = $state<HTMLTextAreaElement>();
  // Ready to type as soon as it opens.
  $effect(() => {
    input?.focus();
  });
  let busy = $state(false);

  const examples = [
    'Check every morning whether my websites are up and fast',
    'Research topics in depth and give me sourced summaries',
    'Keep an eye on this server: disk space, updates, errors',
    'Help me write and polish emails in English and Spanish',
  ];

  async function create() {
    if (!request.trim() || busy) return;
    busy = true;
    const agent = await app.designAgent(request.trim());
    busy = false;
    if (agent) {
      app.modal = null;
      void app.openAgent(agent.id);
      app.toast(`${agent.name} is ready`);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void create();
    }
  }
</script>

<Modal title="New agent" onclose={() => !busy && (app.modal = null)} width={560}>
  <div class="space-y-4 px-6 py-5">
    <div>
      <label class="label" for="agent-request">What should this agent do?</label>
      <textarea
        bind:this={input}
        id="agent-request"
        class="input min-h-24 resize-y font-[inherit] leading-relaxed"
        bind:value={request}
        {onkeydown}
        disabled={busy}
        placeholder="One or two sentences. I.R.I.S. picks its name, tools and instructions."
      ></textarea>
    </div>
    <div class="flex flex-wrap gap-1.5">
      {#each examples as ex}
        <button
          class="rounded-full border border-line-strong px-3 py-1 text-[12px] text-muted transition hover:text-text"
          onclick={() => (request = ex)}
          disabled={busy}>{ex}</button
        >
      {/each}
    </div>
    <p class="hint">You can change anything afterwards (right-click the orb → Edit agent).</p>
  </div>

  {#snippet footer()}
    <button
      class="text-[12.5px] text-faint hover:text-text"
      onclick={() => (app.modal = { kind: 'agent', agentId: null })}
      disabled={busy}>Set it up by hand instead</button
    >
    <div class="ml-auto">
      <button class="btn-primary" onclick={create} disabled={busy || !request.trim()}>
        {#if busy}<LoaderCircle size={14} class="animate-spin" /> Designing…{:else}<Sparkles size={14} /> Create agent{/if}
      </button>
    </div>
  {/snippet}
</Modal>
