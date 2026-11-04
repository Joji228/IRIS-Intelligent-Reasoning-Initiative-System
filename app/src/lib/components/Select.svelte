<script lang="ts" generics="T extends string">
  // The app's dropdown: looks like the inputs, opens with a short pop instead
  // of the system list, works with the keyboard (arrows, Enter, Escape).
  import { Check, ChevronDown } from '@lucide/svelte';
  import { tick } from 'svelte';

  let {
    value = $bindable(),
    options,
    onchange,
    id,
    label,
    mono = false,
    class: className = '',
  }: {
    value: T;
    options: { value: T; label: string }[];
    onchange?: (value: T) => void;
    id?: string;
    /** Accessible name when there's no <label for=id>. */
    label?: string;
    mono?: boolean;
    class?: string;
  } = $props();

  let open = $state(false);
  let active = $state(0);
  let button = $state<HTMLButtonElement>();
  let list = $state<HTMLDivElement>();
  // Fixed position, so the list is never cut off by a scrolling panel.
  let pos = $state({ left: 0, top: 0, width: 0, up: false });

  const current = $derived(options.find((o) => o.value === value));

  async function show() {
    if (!button) return;
    const r = button.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    const up = below < 240 && r.top > below;
    pos = { left: r.left, top: up ? r.top - 6 : r.bottom + 6, width: r.width, up };
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
    await tick();
    list?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function choose(i: number) {
    const option = options[i];
    open = false;
    button?.focus();
    if (!option || option.value === value) return;
    value = option.value;
    onchange?.(option.value);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) {
        e.preventDefault();
        void show();
      }
      return;
    }
    if (e.key === 'Escape' || e.key === 'Tab') {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active + (e.key === 'ArrowDown' ? 1 : options.length - 1)) % options.length;
      list?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      choose(active);
    }
  }

  function onpointerdown(e: PointerEvent) {
    const t = e.target as Node;
    if (open && !list?.contains(t) && !button?.contains(t)) open = false;
  }
</script>

<svelte:window {onpointerdown} onresize={() => (open = false)} onscrollcapture={(e) => open && !list?.contains(e.target as Node) && (open = false)} />

<button
  bind:this={button}
  {id}
  type="button"
  class="input flex items-center gap-2 text-left {mono ? 'font-mono text-[13px]' : ''} {className}"
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-label={label}
  onclick={() => (open ? (open = false) : void show())}
  {onkeydown}
>
  <span class="min-w-0 flex-1 truncate">{current?.label ?? value}</span>
  <ChevronDown size={15} class="shrink-0 text-faint transition-transform duration-200 {open ? 'rotate-180' : ''}" />
</button>

{#if open}
  <div
    bind:this={list}
    class="select-list fixed z-[70] max-h-64 overflow-y-auto rounded-xl border border-line-strong bg-elevated p-1 shadow-[var(--shadow-pop)] scroll-thin"
    class:up={pos.up}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    style:min-width="{pos.width}px"
    role="listbox"
  >
    {#each options as o, i (o.value)}
      <button
        type="button"
        data-i={i}
        class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-[13px] transition-colors {mono ? 'font-mono' : ''} {i === active
          ? 'bg-hover text-text'
          : 'text-muted'} {o.value === value ? 'font-medium text-text' : ''}"
        role="option"
        aria-selected={o.value === value}
        onpointerenter={() => (active = i)}
        onclick={() => choose(i)}
      >
        <span class="min-w-0 flex-1 truncate">{o.label}</span>
        {#if o.value === value}<Check size={14} class="shrink-0 text-accent-hi" />{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .select-list {
    transform-origin: top center;
    animation: select-in 0.14s cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }
  .select-list.up {
    transform-origin: bottom center;
    translate: 0 -100%;
  }
  @keyframes select-in {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
