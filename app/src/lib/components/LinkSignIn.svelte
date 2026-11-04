<script lang="ts">
  import { KeyRound, LoaderCircle, QrCode, ShieldCheck } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { app } from '../app.svelte';
  import { guessDeviceName, relayEngineKey, saveCredentials, signInOnce, type LinkCredentials } from '../link';

  const relay = location.origin;
  let mode = $state<'password' | 'code'>('password');
  let user = $state('');
  let password = $state('');
  let code = $state('');
  let name = $state(guessDeviceName());
  let busy = $state(false);
  let error = $state(app.signinError);
  let pairingFromLink = $state(false);

  function done(creds: LinkCredentials) {
    saveCredentials(creds);
    history.replaceState(null, '', location.pathname);
    app.signinError = '';
    void app.connectLink(creds);
  }

  async function engineKey(): Promise<string | null> {
    const key = await relayEngineKey(relay);
    if (!key) error = "Your server hasn't connected to this relay yet. Turn on Remote access in the I.R.I.S. app's settings.";
    return key;
  }

  async function run(fn: () => Promise<LinkCredentials>) {
    busy = true;
    error = '';
    try {
      done(await fn());
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function signIn() {
    void run(async () => {
      const key = await engineKey();
      if (!key) throw new Error(error);
      return signInOnce(relay, key, { t: 'login', user: user.trim(), password, name: name.trim() });
    });
  }

  function pairWithCode() {
    void run(async () => {
      const key = await engineKey();
      if (!key) throw new Error(error);
      return signInOnce(relay, key, { t: 'pair', code: code.trim(), name: name.trim() });
    });
  }

  // Opened from the QR code / pairing link: pair right away.
  onMount(() => {
    const params = new URLSearchParams(location.hash.slice(1));
    const linkCode = params.get('pair');
    const key = params.get('k');
    if (linkCode && key) {
      pairingFromLink = true;
      void run(() => signInOnce(relay, key, { t: 'pair', code: linkCode, name: name.trim() })).then(() => {
        // Didn't work (the error says why): leave the code ready to retry.
        pairingFromLink = false;
        code = linkCode;
        mode = 'code';
      });
    }
  });
</script>

<div class="flex min-h-full items-center justify-center overflow-y-auto bg-bg px-4 py-10">
  <div class="w-full max-w-sm animate-fade-in">
    <div class="text-center">
      <img src="/iris.png" alt="I.R.I.S." class="mx-auto h-16 w-16 rounded-2xl {busy ? 'animate-pulse' : ''}" />
      <h1 class="mt-5 text-2xl font-semibold tracking-tight">I.R.I.S.</h1>
      <p class="mt-1 text-[13px] text-faint">Sign in to reach your agents from anywhere</p>
    </div>

    {#if pairingFromLink}
      <p class="mt-10 flex items-center justify-center gap-2 text-[14px] text-muted">
        <LoaderCircle size={16} class="animate-spin" /> Pairing this device…
      </p>
    {:else}
      <div class="mt-8 rounded-2xl border border-line bg-surface p-5">
        <div class="mb-4 grid grid-cols-2 gap-1 rounded-xl bg-elevated p-1 text-[13px]">
          <button
            class="flex items-center justify-center gap-1.5 rounded-lg py-2 transition {mode === 'password' ? 'bg-surface font-medium text-text shadow-sm' : 'text-muted'}"
            onclick={() => (mode = 'password')}
          >
            <KeyRound size={14} /> Password
          </button>
          <button
            class="flex items-center justify-center gap-1.5 rounded-lg py-2 transition {mode === 'code' ? 'bg-surface font-medium text-text shadow-sm' : 'text-muted'}"
            onclick={() => (mode = 'code')}
          >
            <QrCode size={14} /> Pairing code
          </button>
        </div>

        <form
          class="space-y-3"
          onsubmit={(e) => {
            e.preventDefault();
            if (mode === 'password') signIn();
            else pairWithCode();
          }}
        >
          {#if mode === 'password'}
            <div>
              <label class="label" for="si-user">Username</label>
              <input id="si-user" class="input" bind:value={user} autocomplete="username" autocapitalize="none" />
            </div>
            <div>
              <label class="label" for="si-pass">Password</label>
              <input id="si-pass" class="input" type="password" bind:value={password} autocomplete="current-password" />
            </div>
          {:else}
            <div>
              <label class="label" for="si-code">Pairing code</label>
              <input
                id="si-code"
                class="input font-mono tracking-widest uppercase"
                bind:value={code}
                placeholder="ABCD2345"
                autocomplete="one-time-code"
                autocapitalize="characters"
              />
              <p class="mt-1.5 text-[12px] text-faint">On the server: Settings → Remote access → Show a pairing code. Or just scan its QR code with your phone's camera.</p>
            </div>
          {/if}
          <div>
            <label class="label" for="si-name">This device</label>
            <input id="si-name" class="input" bind:value={name} />
          </div>
          {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
          <button
            type="submit"
            class="btn-primary w-full py-2.5!"
            disabled={busy || !name.trim() || (mode === 'password' ? !user.trim() || !password : code.trim().length < 6)}
          >
            {#if busy}<LoaderCircle size={14} class="animate-spin" />{/if}
            {mode === 'password' ? 'Sign in' : 'Pair'}
          </button>
        </form>
      </div>
      <p class="mt-4 flex items-start justify-center gap-1.5 text-center text-[11.5px] text-faint">
        <ShieldCheck size={13} class="mt-px shrink-0" /> End-to-end encrypted between this device and your server.
      </p>
    {/if}
  </div>
</div>
