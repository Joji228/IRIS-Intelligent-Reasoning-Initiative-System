<script lang="ts">
  import { Download, LoaderCircle } from '@lucide/svelte';
  import { app } from '../app.svelte';

  // A thin bar across the top when a newer I.R.I.S. is out: one button updates it
  // (the engine downloads the installer, checks it, closes the app, installs and
  // starts it again; chats and settings stay).
  const u = $derived(app.update);
  const busy = $derived(!!u && (u.install.phase === 'downloading' || u.install.phase === 'installing'));
  const failed = $derived(u?.install.phase === 'failed');
  const show = $derived(!!u && ((u.available && !!u.latest?.windows) || busy || failed));
</script>

{#if show && u}
  <div class="flex shrink-0 flex-wrap items-center justify-center gap-x-3 gap-y-1 border-b border-line bg-accent-soft px-3 py-1.5 text-[12.5px] animate-fade-in">
    {#if busy}
      <LoaderCircle size={14} class="animate-spin" />
      <span>
        {u.install.phase === 'downloading'
          ? `Downloading I.R.I.S. ${u.latest?.version ?? ''}… ${u.install.percent}%`
          : 'Installing: I.R.I.S. closes and opens again by itself in a minute or two.'}
      </span>
    {:else if failed}
      <span class="text-danger">The update didn't work: {u.install.error}</span>
      <button class="btn-outline py-0.5! px-2.5! text-[12px]" onclick={() => app.installUpdate()}>Try again</button>
    {:else if app.updateAsk}
      <span>{app.updateAsk}</span>
      <button class="btn-primary py-0.5! px-2.5! text-[12px]" onclick={() => app.installUpdate(true)}>Update anyway</button>
      <button class="btn-ghost py-0.5! px-2.5! text-[12px]" onclick={() => (app.updateAsk = null)}>Not now</button>
    {:else}
      <Download size={14} />
      <span>I.R.I.S. {u.latest?.version} is available.</span>
      {#if u.latest?.notes}
        <button class="btn-ghost py-0.5! px-2.5! text-[12px]" onclick={() => (app.modal = { kind: 'settings', tab: 'about' })}>What's new</button>
      {/if}
      <button class="btn-primary py-0.5! px-2.5! text-[12px]" onclick={() => app.installUpdate()}>Update now</button>
    {/if}
  </div>
{/if}
