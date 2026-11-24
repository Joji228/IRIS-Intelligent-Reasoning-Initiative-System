<script lang="ts">
  // Settings → Usage: tokens and money per day, with totals for today, the last
  // 7 days and the whole range. A switch shows tokens, money or both (both: bars
  // for tokens, a line for money). Hover or tap a day for its breakdown.
  import { LoaderCircle } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { formatTokens } from '../format';
  import type { UsageDaily, UsageDay } from '../types';

  type Mode = 'tokens' | 'money' | 'both';
  const MODE_KEY = 'iris.usageMode';
  const RANGE_KEY = 'iris.usageRange';
  const read = <T,>(k: string, d: T): T => {
    try {
      const v = localStorage.getItem(k);
      return v === null ? d : (JSON.parse(v) as T);
    } catch {
      return d;
    }
  };
  const save = (k: string, v: unknown) => {
    try {
      localStorage.setItem(k, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  };

  let mode = $state<Mode>(read(MODE_KEY, 'both'));
  let range = $state<number>(read(RANGE_KEY, 30));
  let data = $state<UsageDaily | null>(null);
  let loading = $state(false);
  let failed = $state(false);
  let hover = $state<number | null>(null);

  async function load() {
    if (!app.api) return;
    loading = true;
    failed = false;
    try {
      data = await app.api.usageDaily(range);
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }
  $effect(() => {
    void range;
    void load();
  });

  const days = $derived(data?.days ?? []);
  const sum = (list: UsageDay[]) =>
    list.reduce(
      (a, d) => ({ tokens: a.tokens + d.tokens, cost: a.cost + d.cost, estimated: a.estimated || d.estimated }),
      { tokens: 0, cost: 0, estimated: false },
    );
  const today = $derived(sum(days.slice(-1)));
  const week = $derived(sum(days.slice(-7)));
  const all = $derived(sum(days));

  const money = (v: number) => (v >= 1 ? `$${v.toFixed(2)}` : v >= 0.01 ? `$${v.toFixed(3)}` : v > 0 ? `$${v.toFixed(4)}` : '$0');
  const shortDay = (d: string) => {
    const [y, m, dd] = d.split('-').map(Number);
    return new Date(y, m - 1, dd).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  };
  const longDay = (d: string) => {
    const [y, m, dd] = d.split('-').map(Number);
    return new Date(y, m - 1, dd).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
  };

  // ---- the chart ----
  // Drawn at its real width (no stretching of the labels and dots).
  let W = $state(600);
  const H = 150;
  const PAD_B = 18;
  const maxTokens = $derived(Math.max(1, ...days.map((d) => d.tokens)));
  const maxCost = $derived(Math.max(0.0001, ...days.map((d) => d.cost)));
  const slot = $derived(days.length ? W / days.length : W);
  const barW = $derived(Math.max(2, Math.min(18, slot * 0.62)));
  const barValue = (d: UsageDay) => (mode === 'money' ? d.cost / maxCost : d.tokens / maxTokens);
  const y = (v: number) => (H - PAD_B) * (1 - v);
  const line = $derived(
    days.map((d, i) => `${i ? 'L' : 'M'}${(i * slot + slot / 2).toFixed(1)},${y(d.cost / maxCost).toFixed(1)}`).join(' '),
  );
  const labelEvery = $derived(days.length > 45 ? 14 : days.length > 20 ? 7 : days.length > 10 ? 3 : 1);
  const shown = $derived(hover ?? days.length - 1);
  const pick = $derived(days[shown]);

  function onmove(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const i = Math.floor(((e.clientX - r.left) / r.width) * days.length);
    hover = i >= 0 && i < days.length ? i : null;
  }
</script>

<div class="space-y-4">
  <div class="flex flex-wrap items-center gap-2">
    <div class="segmented" role="radiogroup" aria-label="Show">
      {#each [['tokens', 'Tokens'], ['money', 'Money'], ['both', 'Both']] as [id, label] (id)}
        <button
          role="radio"
          aria-checked={mode === id}
          class={mode === id ? 'on' : ''}
          onclick={() => {
            mode = id as Mode;
            save(MODE_KEY, mode);
          }}>{label}</button
        >
      {/each}
    </div>
    <div class="segmented ml-auto" role="radiogroup" aria-label="Range">
      {#each [14, 30, 90] as r (r)}
        <button
          role="radio"
          aria-checked={range === r}
          class={range === r ? 'on' : ''}
          onclick={() => {
            range = r;
            save(RANGE_KEY, r);
          }}>{r} days</button
        >
      {/each}
    </div>
  </div>

  <!-- Totals -->
  <div class="grid grid-cols-3 gap-2.5">
    {#each [['Today', today], ['Last 7 days', week], [`Last ${range} days`, all]] as [label, t] (label)}
      {@const total = t as ReturnType<typeof sum>}
      <div class="rounded-xl border border-line bg-surface/60 px-3.5 py-3">
        <div class="text-[11.5px] text-faint">{label}</div>
        {#if mode !== 'money'}<div class="mt-0.5 text-[16px] font-semibold tabular-nums">{formatTokens(total.tokens)}</div>{/if}
        {#if mode !== 'tokens'}
          <div class="{mode === 'money' ? 'mt-0.5 text-[16px] font-semibold' : 'text-[12.5px] text-muted'} tabular-nums">
            {total.estimated ? '≈ ' : ''}{money(total.cost)}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <!-- Per day -->
  <div class="rounded-xl border border-line bg-surface/60 p-3.5">
    <div class="mb-2 flex min-h-[34px] items-baseline gap-2 text-[12.5px]">
      {#if pick}
        <span class="font-medium">{longDay(pick.day)}</span>
        <span class="text-muted tabular-nums">
          {[
            mode !== 'money' ? `${formatTokens(pick.tokens)} tokens` : '',
            mode !== 'tokens' ? `${pick.estimated ? '≈ ' : ''}${money(pick.cost)}` : '',
          ]
            .filter(Boolean)
            .join(' · ')}
        </span>
        {#if pick.tokens}
          <!-- Older replies didn't record how much input came from the cache -->
          <span class="ml-auto hidden text-[11.5px] text-faint tabular-nums sm:inline">
            {#if pick.estimated}
              {formatTokens(pick.input)} input · {formatTokens(pick.output)} output · cache split not recorded
            {:else}
              {formatTokens(pick.cached)} cached · {formatTokens(Math.max(0, pick.input - pick.cached))} new input · {formatTokens(pick.output)} output
            {/if}
          </span>
        {/if}
      {/if}
      {#if loading}<LoaderCircle size={13} class="ml-auto animate-spin text-faint" />{/if}
    </div>
    {#if failed}
      <p class="py-8 text-center text-[12.5px] text-faint">The usage stats need the server's newest version.</p>
    {:else}
      <div bind:clientWidth={W}>
      <svg
        width={W}
        height={H}
        viewBox="0 0 {W} {H}"
        class="block touch-none select-none"
        role="img"
        aria-label="Usage per day"
        onpointermove={onmove}
        onpointerdown={onmove}
        onpointerleave={() => (hover = null)}
      >
        <line x1="0" x2={W} y1={H - PAD_B} y2={H - PAD_B} class="axis" />
        {#each days as d, i (d.day)}
          {@const v = barValue(d)}
          <rect
            x={i * slot + (slot - barW) / 2}
            y={y(v)}
            width={barW}
            height={Math.max(v > 0 ? 1.5 : 0, H - PAD_B - y(v))}
            rx={Math.min(3, barW / 2)}
            class="bar {i === shown ? 'on' : ''} {mode === 'money' ? 'money' : ''}"
          />
          {#if i % labelEvery === (days.length - 1) % labelEvery}
            <!-- The end labels hug the edges so they aren't cut off -->
            <text
              x={i === days.length - 1 ? W : i === 0 ? 0 : i * slot + slot / 2}
              y={H - 4}
              class="tick"
              text-anchor={i === days.length - 1 ? 'end' : i === 0 ? 'start' : 'middle'}>{shortDay(d.day)}</text
            >
          {/if}
        {/each}
        {#if mode === 'both'}
          <path d={line} class="cost-line" />
          {#each days as d, i (d.day)}
            {#if i === shown}<circle cx={i * slot + slot / 2} cy={y(d.cost / maxCost)} r="3.5" class="cost-dot" />{/if}
          {/each}
        {/if}
      </svg>
      </div>
      {#if mode === 'both'}
        <div class="mt-1.5 flex items-center gap-4 text-[11.5px] text-faint">
          <span class="flex items-center gap-1.5"><span class="legend-bar"></span>Tokens</span>
          <span class="flex items-center gap-1.5"><span class="legend-line"></span>Money</span>
        </div>
      {/if}
    {/if}
  </div>

  {#if data}
    <p class="hint">
      {Math.round(data.cache_rate * 100)}% of input came from DeepSeek's cache, which costs a fraction of new input.
      {#if data.prices}
        Prices for {data.prices.model}: ${data.prices.cached} cached / ${data.prices.input} new input / ${data.prices.output} output
        per 1M tokens at peak hours (01–04 and 06–10 UTC on weekdays), half the rest of the time.
      {/if}
      Amounts marked ≈ include days from before the cache split was recorded, so they're estimates. Money is worked out
      from the price list; your DeepSeek balance is the final word.
    </p>
  {/if}
</div>

<style>
  .axis {
    stroke: var(--color-line-strong);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .bar {
    fill: color-mix(in srgb, var(--color-accent) 55%, transparent);
    transition: fill 150ms ease-out;
  }
  .bar.money {
    fill: color-mix(in srgb, #f472b6 55%, transparent);
  }
  .bar.on {
    fill: var(--color-accent-hi);
  }
  .bar.money.on {
    fill: #f472b6;
  }
  .tick {
    fill: var(--color-faint);
    font-size: 10px;
  }
  .cost-line {
    fill: none;
    stroke: #f472b6;
    stroke-width: 2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .cost-dot {
    fill: #f472b6;
    stroke: var(--color-surface);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .legend-bar {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--color-accent) 55%, transparent);
  }
  .legend-line {
    width: 14px;
    height: 2px;
    border-radius: 2px;
    background: #f472b6;
  }
</style>
