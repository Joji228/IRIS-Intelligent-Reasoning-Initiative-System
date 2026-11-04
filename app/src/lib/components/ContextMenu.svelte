<script lang="ts">
  // The app's right-click menu. Whoever opens it (an orb, the chat header, or
  // the window itself) passes the items; dangerous ones ask once more inline.
  import { Info } from '@lucide/svelte';
  import { tick } from 'svelte';
  import { tip } from '../tooltip';
  import { app, type MenuItem } from '../app.svelte';

  let el = $state<HTMLDivElement>();
  let confirming = $state<string | null>(null);
  let pos = $state({ x: 0, y: 0 });

  // Keep the menu on screen.
  $effect(() => {
    const menu = app.menu;
    confirming = null;
    if (!menu) return;
    pos = { x: menu.x, y: menu.y };
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      pos = {
        x: Math.max(6, Math.min(menu.x, window.innerWidth - r.width - 6)),
        y: Math.max(6, Math.min(menu.y, window.innerHeight - r.height - 6)),
      };
    });
  });

  function run(item: MenuItem) {
    if (item.disabled) return;
    if (item.confirm && confirming !== item.label) {
      confirming = item.label;
      return;
    }
    app.menu = null;
    void item.action();
  }

  function onpointerdown(e: PointerEvent) {
    if (app.menu && el && !el.contains(e.target as Node)) app.menu = null;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') app.menu = null;
  }
</script>

<svelte:window {onpointerdown} {onkeydown} onblur={() => (app.menu = null)} onresize={() => (app.menu = null)} />

{#if app.menu}
  <div
    bind:this={el}
    class="fixed z-[60] w-max max-w-[calc(100vw-12px)] min-w-52 rounded-xl border border-line-strong bg-elevated p-1 shadow-[var(--shadow-pop)] animate-pop-in"
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    role="menu"
  >
    {#if app.menu.title}
      <div class="truncate px-2.5 pt-1.5 pb-1 text-[11px] font-semibold tracking-wider text-faint uppercase">
        {app.menu.title}
      </div>
    {/if}
    {#each app.menu.items as item, i (i)}
      {#if item.separator}
        <div class="my-1 h-px bg-line"></div>
      {/if}
      <button
        class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition disabled:opacity-40 {item.danger
          ? 'text-danger hover:bg-danger-soft'
          : 'text-text hover:bg-hover'}"
        onclick={() => run(item)}
        disabled={item.disabled}
        role="menuitem"
        use:tip={item.info ?? ''}
        data-tip-side="left"
      >
        {#if item.icon}<item.icon size={14} class="shrink-0 {item.danger ? '' : 'text-muted'}" />{/if}
        <!-- The label always shows in full; the hint gives way if space is short -->
        <span class="shrink-0 whitespace-nowrap">{confirming === item.label ? item.confirm : item.label}</span>
        {#if item.info}
          <!-- The explanation opens beside the menu (see use:tip on the row) -->
          <Info size={13} class="ml-auto shrink-0 text-faint" />
        {/if}
        {#if item.hint}<span class="ml-auto min-w-0 truncate pl-3 text-[11px] text-faint">{item.hint}</span>{/if}
      </button>
    {/each}
  </div>
{/if}
