<script lang="ts">
  import { X } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  let {
    title,
    onclose,
    width = 560,
    children,
    footer,
  }: { title: string; onclose: () => void; width?: number; children: Snippet; footer?: Snippet } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex items-center justify-center bg-black/55 backdrop-blur-[3px] md:p-6 animate-fade-in"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="flex max-h-full w-full flex-col overflow-hidden border-line-strong bg-panel shadow-[var(--shadow-pop)] max-md:h-full max-md:max-w-none! md:rounded-2xl md:border animate-pop-in"
    style:max-width="{width}px"
    role="dialog"
    aria-modal="true"
    aria-label={title}
  >
    <div class="flex items-center justify-between border-b border-line px-4 py-3 pt-[max(0.75rem,env(safe-area-inset-top))] md:px-6 md:py-4">
      <h2 class="text-[16px] font-semibold tracking-tight">{title}</h2>
      <button class="rounded-lg p-1.5 text-faint hover:bg-hover hover:text-text" onclick={onclose} aria-label="Close">
        <X size={18} />
      </button>
    </div>
    <div class="flex-1 overflow-y-auto scroll-thin">
      {@render children()}
    </div>
    {#if footer}
      <div class="flex items-center gap-2 border-t border-line px-4 py-3.5 pb-[max(0.875rem,env(safe-area-inset-bottom))] md:px-6">
        {@render footer()}
      </div>
    {/if}
  </div>
</div>
