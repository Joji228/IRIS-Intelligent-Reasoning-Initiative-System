<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { inDesktopApp } from '../connection';
  import { renderMarkdown } from '../markdown';

  let { text }: { text: string } = $props();
  const html = $derived(renderMarkdown(text));

  // Copy buttons in code blocks, and links that open in the real browser
  // instead of navigating the app window.
  async function onclick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const copy = target.closest('[data-copy]') as HTMLButtonElement | null;
    if (copy) {
      const code = copy.closest('.codeblock')?.querySelector('pre')?.textContent ?? '';
      await navigator.clipboard.writeText(code);
      copy.textContent = 'Copied';
      setTimeout(() => (copy.textContent = 'Copy'), 1500);
      return;
    }
    const link = target.closest('a') as HTMLAnchorElement | null;
    if (link?.href) {
      e.preventDefault();
      if (!/^https?:/i.test(link.href)) return;
      if (inDesktopApp) await openUrl(link.href);
      else window.open(link.href, '_blank', 'noopener');
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md selectable" {onclick}>{@html html}</div>
