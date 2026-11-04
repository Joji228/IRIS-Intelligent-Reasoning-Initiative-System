<script lang="ts">
  import { ChevronRight, Sparkles } from '@lucide/svelte';

  let { text, live }: { text: string; live: boolean } = $props();
  // Folded unless the user opens it (also while the model is still thinking),
  // so long reasoning never floods the chat.
  let open = $state(false);
  const expanded = $derived(open);
</script>

<div class="mb-3">
  <button
    class="flex items-center gap-1.5 rounded-md py-0.5 text-[13px] text-muted hover:text-text"
    onclick={() => (open = !open)}
  >
    <Sparkles size={13} class="text-accent-hi" />
    {#if live}
      <span class="shimmer font-medium">Thinking…</span>
    {:else}
      <span>Thought process</span>
    {/if}
    <ChevronRight size={14} class="transition-transform {expanded ? 'rotate-90' : ''}" />
  </button>
  {#if expanded}
    <div
      class="selectable mt-2 max-h-72 overflow-y-auto border-l-2 border-line-strong pl-3 text-[13px] leading-relaxed whitespace-pre-wrap text-faint scroll-thin"
    >
      {text}
    </div>
  {/if}
</div>
