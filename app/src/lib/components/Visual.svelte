<script lang="ts">
  // A visual card in a reply (Intelligent UI): the model's HTML in a sandboxed frame.
  // `allow-scripts` without `allow-same-origin` gives the frame an opaque origin, so
  // it can't reach the app's storage, token or page; the frame's own CSP blocks the
  // network. It only tells us its height and any error.
  import { Code, Maximize2, Sparkles, X } from '@lucide/svelte';
  import { getContext } from 'svelte';
  import { frameUrl, visualTheme, VISUAL_SEND } from '../visual';

  let { title, html, closed }: { title: string; html: string; closed: boolean } = $props();

  const MAX_HEIGHT = 560;
  let showCode = $state(false);
  let full = $state(false);
  let height = $state(0);
  let error = $state<string | null>(null);
  const sendToChat = getContext<((text: string) => void) | undefined>(VISUAL_SEND);
  let lastSend = 0;

  /** A card's own code can ask to send at any time; only a request while the user has just
   *  interacted with the page (the browser's user activation, which a click inside the
   *  frame also gives this page, and which no script can fake) counts. */
  function send(text: unknown) {
    const t = String(text ?? '').trim().slice(0, 1000);
    const active = navigator.userActivation ? navigator.userActivation.isActive : true;
    if (!sendToChat || !t || !active || Date.now() - lastSend < 1500) return;
    lastSend = Date.now();
    full = false;
    sendToChat(t);
  }

  /** Hooks a frame up: sends the visual when the frame is ready, takes its height. */
  function frame(node: HTMLIFrameElement, onSize: (h: number) => void) {
    function onmessage(e: MessageEvent) {
      if (e.source !== node.contentWindow) return;
      const d = e.data;
      if (!d || typeof d !== 'object') return;
      if (d.type === 'ready') {
        node.contentWindow?.postMessage({ type: 'render', html, theme: visualTheme() }, '*');
      } else if (d.type === 'size' && typeof d.height === 'number') {
        onSize(Math.max(0, Math.ceil(d.height)));
      } else if (d.type === 'send') {
        send(d.text);
      } else if (d.type === 'error') {
        error = String(d.message ?? 'error').slice(0, 200);
      }
    }
    window.addEventListener('message', onmessage);
    return { destroy: () => window.removeEventListener('message', onmessage) };
  }

  function onkeydown(e: KeyboardEvent) {
    if (full && e.key === 'Escape') full = false;
  }
</script>

<svelte:window {onkeydown} />

<div class="visual">
  <div class="visual-head">
    <Sparkles size={14} class="text-accent-hi" />
    <span class="visual-title">{title || 'Visual'}</span>
    {#if closed}
      <button type="button" class:on={showCode} onclick={() => (showCode = !showCode)} aria-label="Show code" title="Show code">
        <Code size={15} />
      </button>
      <button type="button" onclick={() => (full = true)} aria-label="Open full screen" title="Open full screen">
        <Maximize2 size={15} />
      </button>
    {/if}
  </div>
  {#if !closed}
    <div class="visual-wait"><span class="shimmer">Building visual…</span></div>
  {:else if showCode}
    <pre class="visual-code">{html}</pre>
  {:else}
    <div class="visual-body" style:height="{height > 0 ? Math.min(height, MAX_HEIGHT) : 120}px">
      <iframe
        title={title || 'Visual'}
        src={frameUrl()}
        sandbox="allow-scripts"
        referrerpolicy="no-referrer"
        scrolling={height > MAX_HEIGHT ? 'yes' : 'no'}
        use:frame={(h) => { if (h > 0) height = h; }}
      ></iframe>
      {#if height === 0 && !error}
        <div class="visual-wait absolute inset-0"><span class="shimmer">Building visual…</span></div>
      {/if}
    </div>
    {#if error}<div class="visual-error">This visual hit an error: {error}</div>{/if}
  {/if}
</div>

{#if full}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-40 flex items-center justify-center bg-black/55 backdrop-blur-[3px] md:p-6 animate-fade-in" onclick={(e) => e.target === e.currentTarget && (full = false)}>
    <div class="flex h-full w-full max-w-[960px] flex-col overflow-hidden bg-bg shadow-[var(--shadow-pop)] md:rounded-2xl md:border md:border-line-strong animate-pop-in" role="dialog" aria-modal="true" aria-label={title || 'Visual'}>
      <div class="flex items-center gap-2.5 border-b border-line px-4 py-3 pt-[max(0.75rem,env(safe-area-inset-top))]">
        <Sparkles size={16} class="text-accent-hi" />
        <h2 class="flex-1 truncate text-[16px] font-semibold tracking-tight">{title || 'Visual'}</h2>
        <button class="rounded-lg p-1.5 text-faint hover:bg-hover hover:text-text" onclick={() => (full = false)} aria-label="Close">
          <X size={18} />
        </button>
      </div>
      <iframe class="flex-1 w-full border-0" title={title || 'Visual'} src={frameUrl()} sandbox="allow-scripts" referrerpolicy="no-referrer" use:frame={() => {}}></iframe>
    </div>
  </div>
{/if}

<style>
  .visual {
    margin: 0.9em 0;
    border-radius: 16px;
    overflow: hidden;
    background: var(--c-surface);
  }
  .visual-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 14px;
    min-height: 40px;
  }
  .visual-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 500;
    color: var(--c-muted);
  }
  .visual-head button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    color: var(--c-faint);
  }
  .visual-head button:hover { background: var(--c-hover); color: var(--c-text); }
  .visual-head button.on { color: var(--c-accent-hi); }
  .visual-body { position: relative; transition: height 0.2s ease; }
  .visual-body iframe { display: block; width: 100%; height: 100%; border: 0; }
  .visual-wait {
    display: grid;
    place-items: center;
    height: 120px;
    font-size: 13px;
  }
  .visual-code {
    margin: 0;
    padding: 0 14px 14px;
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--c-muted);
  }
  .visual-error { padding: 0 14px 12px; font-size: 12px; color: var(--c-danger); }
</style>
