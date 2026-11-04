<script lang="ts">
  // The default provider's API credit, top right (chat header and Home).
  import { RefreshCw, Wallet } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { relativeTime } from '../time';
  import { tip } from '../tooltip';

  const symbols: Record<string, string> = { USD: '$', CNY: '¥', EUR: '€' };
  const money = (amount: string, currency: string) => {
    const n = Number(amount);
    const value = Number.isFinite(n) ? n.toFixed(2) : amount;
    return symbols[currency] ? `${symbols[currency]}${value}` : `${value} ${currency}`;
  };
  const credit = $derived(app.balance ? money(app.balance.total, app.balance.currency) : '—');
  const low = $derived(!!app.balance && (!app.balance.available || Number(app.balance.total) < 0.5));
  const detail = $derived(
    app.balance
      ? `${app.defaultModel?.provider_name ?? 'API'} credit: ${credit} (topped up ${money(app.balance.topped_up, app.balance.currency)}, granted ${money(app.balance.granted, app.balance.currency)}) · updated ${relativeTime(app.balanceUpdated)}${app.balancePaused ? ' · auto-refresh paused while idle' : ''}`
      : app.balanceError || 'API credit',
  );
</script>

<div class="flex shrink-0 items-center rounded-full border border-line bg-surface/70 pr-0.5 pl-2.5 {low ? 'border-warn/50' : ''}">
  <button
    class="flex items-center gap-1.5 py-1 text-[13px] font-semibold tabular-nums transition {low ? 'text-warn' : 'text-muted hover:text-text'}"
    onclick={() => (app.modal = { kind: 'settings', tab: 'providers' })}
    use:tip={detail}
    data-tip-side="bottom"
  >
    <Wallet size={14} class="shrink-0" />
    {credit}
  </button>
  <button
    class="ml-0.5 rounded-full p-1.5 text-faint transition hover:bg-hover hover:text-text"
    onclick={() => app.refreshBalance(true)}
    use:tip={'Refresh credit'}
    data-tip-side="bottom"
    aria-label="Refresh credit"
  >
    <RefreshCw size={12} class={app.balanceLoading ? 'animate-spin' : ''} />
  </button>
</div>
