<script lang="ts">
  // Settings → Voice & calls, the same as the phone's Settings → Voice & calls: speech to text (on the
  // server, with Groq), how many free spoken turns are left today, what you speak, the voices
  // this computer or browser has (with a test that shows it's playing), and how calls behave.
  // Kept in this app or browser, like the phone keeps its own.
  import { Volume2, Square } from '@lucide/svelte';
  import { onDestroy, onMount } from 'svelte';
  import { app } from '../app.svelte';
  import { inWebLink } from '../connection';
  import { describeReset, SpeechStream } from '../call/logic';
  import { voicePrefs, type VoiceSettings } from '../call/settings.svelte';
  import { canSpeak, Speaker, voicesFor, type VoiceChoice } from '../call/speaker.svelte';
  import type { VoiceStatus } from '../types';
  import Select from './Select.svelte';

  const p = $derived(voicePrefs.value);
  let status = $state<VoiceStatus | null>(null);
  let failed = $state(false);
  let voicesEn = $state<VoiceChoice[]>([]);
  let voicesEl = $state<VoiceChoice[]>([]);
  const speaker = new Speaker();
  let testing = $state<string | null>(null);

  const SAMPLES = { en: 'Hello, this is Iris. Can you hear me?', el: 'Γεια σου, είμαι η Ίρις. Με ακούς;' };
  const MIXED =
    'Γεια σου! Σήμερα έχει 21,5 βαθμούς στην Αθήνα. The weather in London is cloudy, around 14.5 degrees. ' +
    'Θες να σε ενημερώσω κάθε πρωί; Just say the word.\n[details]\nThis part is only shown in the chat and is not read aloud.';

  function loadVoices() {
    voicesEn = voicesFor('en');
    voicesEl = voicesFor('el');
  }

  onMount(async () => {
    if (canSpeak()) {
      loadVoices();
      // Browsers load their voice list a moment after the page.
      speechSynthesis.addEventListener('voiceschanged', loadVoices);
    }
    try {
      status = (await app.api?.voice()) ?? null;
    } catch {
      failed = true;
    }
  });
  onDestroy(() => {
    if (canSpeak()) speechSynthesis.removeEventListener('voiceschanged', loadVoices);
    speaker.stop();
  });

  // Back to "Test" when the sound has finished.
  $effect(() => {
    if (testing && !speaker.speaking) {
      const t = setTimeout(() => !speaker.speaking && (testing = null), 300);
      return () => clearTimeout(t);
    }
  });

  function test(key: 'en' | 'el' | 'mixed') {
    if (testing === key) {
      speaker.stop();
      testing = null;
      return;
    }
    speaker.stop();
    testing = key;
    if (key === 'mixed') {
      // Fed in small pieces, the way a reply streams in.
      const s = new SpeechStream();
      for (let i = 0; i < MIXED.length; i += 6) s.push(MIXED.slice(i, i + 6)).forEach((x) => speaker.say(x, p));
      s.flush().forEach((x) => speaker.say(x, p));
    } else speaker.say(SAMPLES[key], p);
  }

  function set<K extends keyof VoiceSettings>(k: K, v: VoiceSettings[K]) {
    voicePrefs.set(k, v);
  }

  const share = $derived(status?.limits ? Math.max(0, Math.min(1, status.limits.remaining / status.limits.limit)) : 1);
</script>

{#snippet chips<T extends string>(options: [T, string][], value: T, onpick: (v: T) => void)}
  <div class="flex flex-wrap gap-1.5">
    {#each options as [v, label] (v)}
      <button
        class="rounded-full border px-3.5 py-1.5 text-[12.5px] transition {value === v ? 'border-accent/70 bg-accent-soft text-text' : 'border-line-strong text-muted hover:text-text'}"
        onclick={() => onpick(v)}>{label}</button
      >
    {/each}
  </div>
{/snippet}

{#snippet testButton(key: 'en' | 'el' | 'mixed', label: string)}
  <button
    class="flex items-center gap-2 rounded-xl border px-3.5 py-2 text-[13px] font-medium transition {testing === key ? 'border-transparent bg-accent text-white' : 'border-line-strong hover:bg-hover'}"
    onclick={() => test(key)}
    disabled={!canSpeak()}
  >
    {#if testing === key}
      <span class="bars" aria-hidden="true"><i></i><i></i><i></i></span> Speaking… click to stop <Square size={12} fill="currentColor" />
    {:else}
      <Volume2 size={15} /> {label}
    {/if}
  </button>
{/snippet}

<h3 class="text-[15px] font-semibold">Voice & calls</h3>
<p class="mb-5 text-[12.5px] text-faint">Call an agent from its chat (the phone button), in English or Greek. You talk, it answers out loud.</p>

<div class="max-w-xl space-y-4">
  <!-- Speech to text -->
  <div class="rounded-xl border border-accent/30 bg-accent-soft/40 p-4">
    <div class="text-[14px] font-semibold">Speech to text</div>
    <p class="mb-3 text-[12.5px] text-faint">Your voice becomes text on your server, using Groq's free Whisper.</p>
    <div class="flex items-center gap-2 text-[13px]">
      {#if !status && !failed}
        <span class="text-muted">Asking the server…</span>
      {:else if !status}
        <span class="size-1.5 rounded-full bg-danger"></span><span class="text-danger">Couldn't reach the server</span>
      {:else if status.available}
        <span class="size-1.5 rounded-full bg-success"></span><span class="text-success">{status.provider} · {status.model} · ready</span>
      {:else}
        <span class="size-1.5 rounded-full bg-warn"></span><span class="text-warn">Add a Groq key in Settings → AI providers (free at console.groq.com)</span>
      {/if}
    </div>
    {#if status?.available}
      {#if status.limits && status.limits.limit > 0}
        {@const l = status.limits}
        <div class="mt-3.5">
          <div class="mb-1.5 flex items-baseline text-[13px]">
            <span class="flex-1 font-medium">Spoken turns left today</span>
            <span class="text-muted tabular-nums">{l.remaining.toLocaleString()} of {l.limit.toLocaleString()}</span>
          </div>
          <div class="h-2 overflow-hidden rounded-full bg-hover">
            <div class="h-full rounded-full transition-all {share < 0.1 ? 'bg-danger' : share < 0.3 ? 'bg-warn' : 'bg-success'}" style="width: {share * 100}%"></div>
          </div>
          <p class="mt-1.5 text-[11.5px] text-faint">{describeReset(l, Date.now() / 1000)}</p>
        </div>
      {:else}
        <p class="mt-2 text-[12px] text-faint">How many spoken turns you have left today shows here after your first one.</p>
      {/if}
    {/if}
    <p class="mt-3 text-[12px] text-faint">Your voice clips go through your server to Groq to be turned into text. The free plan allows about 2,000 spoken turns a day.</p>
    <div class="mt-3.5">
      <div class="label">What you speak</div>
      {@render chips<'' | 'el' | 'en'>([['', 'English + Greek'], ['el', 'Ελληνικά'], ['en', 'English']], p.language, (v) => set('language', v))}
      <p class="mt-1.5 text-[11.5px] text-faint">Calls understand English and Greek only. "English + Greek" picks the right one each time; pick one language if it ever gets confused.</p>
    </div>
  </div>

  <!-- Voices -->
  <div class="rounded-xl border border-line bg-surface/60 p-4">
    <div class="text-[14px] font-semibold">Voices</div>
    <p class="mb-3 text-[12.5px] text-faint">Replies are spoken by this {inWebLink ? 'browser' : 'computer'}'s own voices. Nothing is downloaded.</p>
    {#if !canSpeak()}
      <p class="text-[13px] text-warn">This browser can't speak. Calls still work, but replies won't be read aloud.</p>
    {:else}
      {#each [['en', 'English', voicesEn], ['el', 'Ελληνικά', voicesEl]] as const as [lang, name, list] (lang)}
        <div class="mb-4">
          <div class="mb-1.5 flex items-center gap-2 text-[13.5px] font-medium">
            {name}
            {#if list.length}
              <span class="rounded-full bg-success/12 px-2 py-0.5 text-[11px] text-success">Installed</span>
            {:else}
              <span class="rounded-full bg-warn/12 px-2 py-0.5 text-[11px] text-warn">Not installed</span>
            {/if}
          </div>
          {#if list.length}
            <div class="mb-2">
              <Select
                value={lang === 'el' ? p.voiceEl : p.voiceEn}
                options={[{ value: '', label: 'Best available' }, ...list.map((v) => ({ value: v.name, label: v.label }))]}
                onchange={(v) => set(lang === 'el' ? 'voiceEl' : 'voiceEn', v)}
              />
            </div>
            {@render testButton(lang, `Test ${name}`)}
          {:else}
            <p class="text-[12px] text-faint">
              {lang === 'el'
                ? 'Windows: Settings → Time & language → Speech → Add voices → Ελληνικά. Then restart the app.'
                : 'Add an English voice in your system\'s speech settings.'}
            </p>
          {/if}
        </div>
      {/each}
      <div class="label">Speed</div>
      {@render chips<VoiceSettings['speed']>([['slow', 'Slower'], ['normal', 'Normal'], ['fast', 'Faster']], p.speed, (v) => set('speed', v))}
      <div class="mt-4 border-t border-line pt-4">
        <p class="mb-2 text-[12px] text-faint">A reply is read sentence by sentence, each in its own language; the part after [details] stays silent.</p>
        {@render testButton('mixed', 'Test a mixed paragraph')}
        {#if testing && speaker.nowSaying}<p class="mt-2 text-[13px] text-muted">“{speaker.nowSaying}”</p>{/if}
      </div>
    {/if}
  </div>

  <!-- Calls -->
  <div class="rounded-xl border border-line bg-surface/60 p-4">
    <div class="mb-3 text-[14px] font-semibold">Calls</div>
    {#each [['interrupt', 'Interrupt by talking', 'Start talking and it stops to listen. With speakers (no headphones) it needs you to speak up.'], ['sayProgress', 'Say what it\'s doing', 'A short line while it searches or works.']] as const as [key, title, hint] (key)}
      <label class="mb-3 flex cursor-pointer items-start gap-3">
        <input type="checkbox" class="mt-0.5 size-4 accent-[var(--c-accent)]" checked={p[key]} onchange={(e) => set(key, e.currentTarget.checked)} />
        <span><span class="block text-[13.5px] font-medium">{title}</span><span class="block text-[12px] text-faint">{hint}</span></span>
      </label>
    {/each}
    <div class="label">Wait before answering</div>
    {@render chips<VoiceSettings['wait']>([['short', 'Short · 0.5 s'], ['normal', 'Normal · 0.8 s'], ['long', 'Long · 1.2 s']], p.wait, (v) => set('wait', v))}
    <p class="mt-1.5 text-[11.5px] text-faint">How long you stay quiet before your turn counts as finished.</p>
  </div>
</div>

<style>
  .bars {
    display: inline-flex;
    gap: 2px;
    align-items: center;
    height: 12px;
  }
  .bars i {
    width: 2.5px;
    height: 12px;
    border-radius: 2px;
    background: currentColor;
    animation: bar 0.45s ease-in-out infinite alternate;
  }
  .bars i:nth-child(2) { animation-duration: 0.35s; }
  .bars i:nth-child(3) { animation-duration: 0.55s; }
  @keyframes bar {
    from { scale: 1 0.3; }
    to { scale: 1 1; }
  }
</style>
