<script lang="ts">
  import { CircleAlert, CircleCheck } from '@lucide/svelte';
  import { app } from '../app.svelte';
</script>

<div class="pointer-events-none fixed right-5 bottom-5 z-50 flex flex-col items-end gap-2">
  {#each app.toasts as t (t.id)}
    <div
      class="pointer-events-auto flex max-w-sm items-start gap-2.5 rounded-xl border bg-elevated px-4 py-3 text-[13px] shadow-[var(--shadow-pop)] animate-pop-in {t.kind ===
      'error'
        ? 'border-danger/30'
        : 'border-line-strong'}"
    >
      {#if t.kind === 'error'}
        <CircleAlert size={16} class="mt-px shrink-0 text-danger" />
      {:else}
        <CircleCheck size={16} class="mt-px shrink-0 text-success" />
      {/if}
      <span class="selectable">{t.text}</span>
    </div>
  {/each}
</div>
