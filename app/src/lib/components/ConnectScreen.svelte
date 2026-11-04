<script lang="ts">
  import { LoaderCircle, RefreshCw } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { inDesktopApp, inWebLink } from '../connection';

  let url = $state('');
  let token = $state('');
  let busy = $state(false);
  let failed = $state(false);

  async function connect() {
    busy = true;
    failed = !(await app.connect({ url: url.trim(), token: token.trim() }, true));
    busy = false;
  }

  // The desktop app starts the engine itself; keep retrying while it boots.
  $effect(() => {
    if (app.conn !== 'offline' || !inDesktopApp) return;
    const timer = setInterval(() => app.retry(), 3000);
    return () => clearInterval(timer);
  });
</script>

<div class="flex h-full items-center justify-center bg-bg p-6">
  <div class="w-full max-w-sm text-center animate-fade-in">
    <img
      src="/iris.png"
      alt="I.R.I.S."
      class="mx-auto h-20 w-20 rounded-3xl {app.conn === 'starting' ? 'animate-pulse' : ''}"
    />
    <h1 class="mt-6 text-2xl font-semibold tracking-tight">I.R.I.S.</h1>
    <p class="mt-1 text-[13px] text-faint">Intelligent Reasoning & Initiative System</p>

    {#if app.conn === 'starting'}
      <p class="mt-8 flex items-center justify-center gap-2 text-[13px] text-muted">
        <LoaderCircle size={15} class="animate-spin" /> Connecting to the engine…
      </p>
    {:else if app.conn === 'offline' && inWebLink}
      <p class="mt-8 text-[13.5px] text-muted">
        {app.linkState === 'server-offline'
          ? "Your server isn't connected right now. It reconnects by itself once it's on and online."
          : app.linkState === 'connecting'
            ? 'Connecting…'
            : "Can't reach the relay. Check your internet connection."}
      </p>
      <div class="mt-4 flex justify-center gap-2">
        <button class="btn-outline" onclick={() => app.retry()}><RefreshCw size={14} /> Try again</button>
        <button class="btn-ghost" onclick={() => app.signOut()}>Sign out</button>
      </div>
    {:else if app.conn === 'offline'}
      <p class="mt-8 text-[13.5px] text-muted">
        {inDesktopApp ? 'Starting the engine…' : "Can't reach the I.R.I.S. engine."}
      </p>
      <button class="btn-outline mt-4" onclick={() => app.retry()}>
        <RefreshCw size={14} /> Try again
      </button>
    {:else}
      <div class="mt-8 space-y-3 text-left">
        <div>
          <label class="label" for="c-url">Engine address</label>
          <input id="c-url" class="input font-mono text-[13px]" bind:value={url} placeholder="http://192.168.x.x:7700" />
        </div>
        <div>
          <label class="label" for="c-token">Access token</label>
          <input id="c-token" class="input font-mono text-[13px]" type="password" bind:value={token} placeholder="iris_…" />
        </div>
        {#if failed}<p class="text-[12.5px] text-danger">Couldn't connect. Check the address and token.</p>{/if}
        <button class="btn-primary w-full py-2.5!" onclick={connect} disabled={busy || !url || !token}>
          {#if busy}<LoaderCircle size={14} class="animate-spin" />{/if} Connect
        </button>
      </div>
    {/if}
  </div>
</div>
