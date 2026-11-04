<script lang="ts">
  // One labelled usage bar: "Daily limit   127.8k / 500k". Without a limit it
  // shows the usage and "no limit" with an empty track.
  import { formatTokens } from '../format';

  let { label, used, limit, hint = '' }: { label: string; used: number; limit: number | null; hint?: string } =
    $props();

  const pct = $derived(limit ? Math.min(100, (used / limit) * 100) : 0);
  const tone = $derived(pct >= 100 ? 'bg-danger' : pct >= 80 ? 'bg-warn' : 'bg-accent');
</script>

<div>
  <div class="mb-1.5 flex items-baseline justify-between gap-3 text-[12px]">
    <span class="font-medium text-text">{label}</span>
    <span class="text-muted tabular-nums">
      {formatTokens(used)}
      {#if limit}
        <span class="text-faint">/ {formatTokens(limit)}</span>
        <span class="ml-1 text-faint">({pct < 1 && pct > 0 ? '<1' : Math.round(pct)}%)</span>
      {:else}
        <span class="text-faint">· no limit</span>
      {/if}
    </span>
  </div>
  <div class="h-1.5 overflow-hidden rounded-full bg-hover">
    {#if limit}
      <div class="h-full rounded-full transition-all {tone}" style:width="{Math.max(pct, used > 0 ? 1.5 : 0)}%"></div>
    {/if}
  </div>
  {#if hint}<p class="mt-1 text-[11px] text-faint">{hint}</p>{/if}
</div>
