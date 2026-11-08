<script lang="ts">
  import { WifiOff } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { app } from './lib/app.svelte';
  import AgentCreator from './lib/components/AgentCreator.svelte';
  import AgentEditor from './lib/components/AgentEditor.svelte';
  import ApprovalPopups from './lib/components/ApprovalPopups.svelte';
  import ChatView from './lib/components/ChatView.svelte';
  import ConnectScreen from './lib/components/ConnectScreen.svelte';
  import LinkSignIn from './lib/components/LinkSignIn.svelte';
  import Home from './lib/components/Home.svelte';
  import Rail from './lib/components/Rail.svelte';
  import Settings from './lib/components/Settings.svelte';
  import SearchPanel from './lib/components/SearchPanel.svelte';
  import ProblemsPanel from './lib/components/ProblemsPanel.svelte';
  import ContextMenu from './lib/components/ContextMenu.svelte';
  import CreditBadge from './lib/components/CreditBadge.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import UpdateBanner from './lib/components/UpdateBanner.svelte';

  onMount(() => {
    void app.start();
  });

  // Keyboard shortcuts: Ctrl+, settings, Ctrl+H home, Ctrl+K search chats.
  function onkeydown(e: KeyboardEvent) {
    if (!e.ctrlKey || e.altKey || app.modal) return;
    const key = e.key.toLowerCase();
    if (key === 'k') {
      e.preventDefault();
      app.modal = { kind: 'search' };
    } else if (key === ',') {
      e.preventDefault();
      app.modal = { kind: 'settings' };
    } else if (key === 'h') {
      e.preventDefault();
      app.goHome();
    }
  }

  // Right-click: our own menu, except where the browser's is useful (typing,
  // or copying selected text). Orbs open their own menu before this.
  function oncontextmenu(e: MouseEvent) {
    if (e.defaultPrevented) return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, [contenteditable="true"]') || window.getSelection()?.toString()) return;
    e.preventDefault();
    app.openAppMenu(e.clientX, e.clientY);
  }

  const ready = $derived(app.conn === 'online' || app.conn === 'reconnecting');
</script>

<svelte:window {onkeydown} {oncontextmenu} />

{#if ready}
  <!-- Connection trouble: a red bar across the very top -->
  {#if app.conn === 'reconnecting' || !app.online}
    <div class="flex h-7 shrink-0 items-center justify-center gap-2 bg-danger text-[12px] font-medium text-white animate-fade-in">
      <WifiOff size={13} />
      {app.conn === 'reconnecting'
        ? "Can't reach the I.R.I.S. engine. Reconnecting…"
        : `No connection to ${app.defaultModel?.provider_name ?? 'the AI provider'}: check the internet. Agents can't answer until it's back.`}
    </div>
  {/if}
  <UpdateBanner />
  <div class="flex min-h-0 flex-1">
    <!-- Phones: the rail on the home screen; chats take the whole width -->
    <div class="h-full {app.view.kind === 'home' ? 'flex' : 'hidden md:flex'}">
      <Rail />
    </div>
    <main class="relative min-w-0 flex-1 bg-bg">
      {#if app.view.kind === 'home'}
        <!-- The credit stays in the top right corner (chats have it in their header) -->
        <div class="absolute top-3 right-3 z-10 md:top-4 md:right-5"><CreditBadge /></div>
        <Home />
      {:else}
        <ChatView />
      {/if}
    </main>
    <ApprovalPopups />
  </div>
{:else if app.conn === 'signin'}
  <LinkSignIn />
{:else}
  <ConnectScreen />
{/if}

{#if app.modal?.kind === 'newAgent'}
  <AgentCreator />
{:else if app.modal?.kind === 'agent'}
  <AgentEditor agentId={app.modal.agentId} />
{:else if app.modal?.kind === 'settings'}
  <Settings tab={app.modal.tab} />
{:else if app.modal?.kind === 'search'}
  <SearchPanel />
{:else if app.modal?.kind === 'problems'}
  <ProblemsPanel />
{/if}

<Toasts />
<ContextMenu />
