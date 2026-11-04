<script lang="ts">
  import { Search } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { relativeTime } from '../time';
  import type { SearchHit } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import Modal from './Modal.svelte';

  let query = $state('');
  let agentId = $state('');
  let hits = $state<SearchHit[]>([]);
  let searched = $state(false);
  let busy = $state(false);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  // Search as you type, once the typing pauses.
  $effect(() => {
    const q = query.trim();
    const agent = agentId;
    clearTimeout(timer);
    if (q.length < 2) {
      hits = [];
      searched = false;
      return;
    }
    timer = setTimeout(() => run(q, agent), 220);
  });

  async function run(q: string, agent: string) {
    if (!app.api) return;
    const mine = ++seq;
    busy = true;
    try {
      const found = await app.api.searchChats(q, agent);
      if (mine !== seq) return;
      hits = found;
      active = 0;
      searched = true;
    } catch (e) {
      app.error(e);
    } finally {
      if (mine === seq) busy = false;
    }
  }

  function open(hit: SearchHit) {
    app.modal = null;
    void app.openMessage(hit.agent_id, hit.message_id);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && hits.length) {
      e.preventDefault();
      active = (active + 1) % hits.length;
    } else if (e.key === 'ArrowUp' && hits.length) {
      e.preventDefault();
      active = (active - 1 + hits.length) % hits.length;
    } else if (e.key === 'Enter' && hits[active]) {
      e.preventDefault();
      open(hits[active]);
    }
  }

  /** The snippet with its «marked» words split out for highlighting. */
  function parts(snippet: string): { text: string; hit: boolean }[] {
    return snippet.split(/(«[^»]*»)/).filter(Boolean).map((p) =>
      p.startsWith('«') ? { text: p.slice(1, -1), hit: true } : { text: p, hit: false },
    );
  }
</script>

<Modal title="Search chats" onclose={() => (app.modal = null)} width={640}>
  <div class="sticky top-0 z-10 flex items-center gap-2 border-b border-line bg-panel px-4 py-3 md:px-6">
    <Search size={16} class="shrink-0 text-faint" />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="min-w-0 flex-1 bg-transparent text-[14.5px] text-text placeholder:text-faint focus:outline-none"
      placeholder="What was said about…"
      bind:value={query}
      {onkeydown}
      autofocus
      aria-label="Search words"
    />
    <select
      class="max-w-36 rounded-lg border border-line-strong bg-surface px-2 py-1 text-[12.5px] text-muted"
      bind:value={agentId}
      aria-label="Which agent"
    >
      <option value="">All agents</option>
      {#each app.agents as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
    </select>
  </div>
  <div class="px-2 py-2 md:px-3">
    {#if busy && !hits.length}
      <p class="px-3 py-6 text-center text-[13px] text-faint">Searching…</p>
    {:else if searched && !hits.length}
      <p class="px-3 py-6 text-center text-[13px] text-faint">Nothing found. Try other or fewer words.</p>
    {:else if !searched}
      <p class="px-3 py-6 text-center text-[13px] text-faint">Searches every agent's chat, including older parts that were compacted.</p>
    {/if}
    {#each hits as hit, i (hit.message_id)}
      <button
        class="flex w-full items-start gap-3 rounded-xl px-3 py-2.5 text-left transition {i === active ? 'bg-hover' : 'hover:bg-hover'}"
        onclick={() => open(hit)}
        onmouseenter={() => (active = i)}
      >
        <div class="pt-0.5"><AgentAvatar agent={app.agent(hit.agent_id)} size={26} /></div>
        <div class="min-w-0 flex-1">
          <div class="flex items-baseline gap-2 text-[12px]">
            <span class="font-medium text-text">{hit.role === 'user' ? 'You' : hit.agent_name}</span>
            <span class="text-faint">{hit.role === 'user' ? `to ${hit.agent_name} · ` : ''}{relativeTime(hit.created_at)}</span>
          </div>
          <div class="mt-0.5 line-clamp-3 text-[13px] leading-relaxed text-muted">
            {#each parts(hit.snippet) as p, j (j)}{#if p.hit}<mark class="rounded bg-accent-soft px-0.5 text-accent-hi">{p.text}</mark>{:else}{p.text}{/if}{/each}
          </div>
        </div>
      </button>
    {/each}
  </div>
</Modal>
