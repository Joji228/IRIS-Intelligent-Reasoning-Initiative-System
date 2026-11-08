<script lang="ts">
  import { ArrowUp, FileText, LoaderCircle, Mic, Plus, Square, Telescope, X } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { inWebLink } from '../connection';
  import { tip } from '../tooltip';
  import { voice } from '../voice.svelte';

  // The message box: one rounded pill (attach, text, research, mic, send).
  // The model and auto pickers live in the chat header.
  let {
    placeholder,
    running,
    disabled = false,
  }: {
    placeholder: string;
    running: boolean;
    disabled?: boolean;
  } = $props();

  let text = $state('');
  let input: HTMLTextAreaElement;
  // Deep research for the next message: the agent reads many sources and
  // writes a cited report. Matches the engine's chat::RESEARCH_PREFIX.
  let research = $state(false);
  const RESEARCH_PREFIX = '[Deep research]';
  // Text that came from the microphone: its reply is read aloud.
  let dictated = $state(false);

  $effect(() => {
    if (app.api && voice.available === null && voice.canRecord) {
      app.api.voice().then((s) => (voice.available = s.available)).catch(() => (voice.available = false));
    }
  });

  async function mic() {
    if (voice.transcribing) return;
    if (!voice.recording) {
      try {
        await voice.start();
      } catch {
        app.toast('The microphone is not available here', 'error');
      }
      return;
    }
    if (!app.api) return voice.cancel();
    try {
      const said = await voice.finish(app.api);
      if (said) {
        text = text.trim() ? `${text.trimEnd()} ${said}` : said;
        dictated = true;
        queueMicrotask(() => {
          autosize();
          input?.focus();
        });
      }
    } catch (e) {
      app.error(e);
    }
  }
  // Files waiting to be sent with the next message.
  let files = $state<File[]>([]);
  let picker = $state<HTMLInputElement>();
  // Through the relay a file grows to about 1.8x its size and the relay drops
  // frames over 32 MB, so files that big would vanish without a word: say so up front.
  const MAX_MB = inWebLink ? 16 : 20;
  const MAX_FILE = MAX_MB * 1024 * 1024;
  const ACCEPT =
    'image/png,image/jpeg,image/gif,image/webp,.txt,.md,.csv,.tsv,.json,.xml,.yaml,.yml,.toml,.ini,.log,.html,.htm,.css,.js,.ts,.py,.rs,.ps1,.sh,.bat,.sql,.pdf';

  function addFiles(list: FileList | File[] | null) {
    for (const f of Array.from(list ?? [])) {
      if (f.size > MAX_FILE) app.toast(`${f.name} is larger than ${MAX_MB} MB`, 'error');
      else if (files.length < 10) files.push(f);
    }
    input?.focus();
  }

  // Pasting a screenshot attaches it.
  function onpaste(e: ClipboardEvent) {
    const pasted = Array.from(e.clipboardData?.files ?? []);
    if (pasted.length) {
      e.preventDefault();
      addFiles(pasted.map((f, i) => (f.name && f.name !== 'image.png' ? f : new File([f], `pasted-${Date.now()}-${i}.png`, { type: f.type }))));
    }
  }

  function ondrop(e: DragEvent) {
    if (e.dataTransfer?.files.length) {
      e.preventDefault();
      addFiles(e.dataTransfer.files);
    }
  }

  const previews = $derived(files.map((f) => (f.type.startsWith('image/') ? URL.createObjectURL(f) : null)));

  export function focus() {
    input?.focus();
  }

  export function fill(value: string) {
    text = value;
    queueMicrotask(() => {
      autosize();
      input?.focus();
    });
  }

  function autosize() {
    if (!input) return;
    input.style.height = 'auto';
    input.style.height = `${Math.min(input.scrollHeight, 240)}px`;
  }

  async function submit() {
    const value = text.trim();
    if ((!value && files.length === 0) || disabled) return;
    const sending = files;
    const deep = research && !!value;
    if (voice.recording) voice.cancel();
    voice.autoSpeak = dictated;
    dictated = false;
    text = '';
    files = [];
    research = false;
    queueMicrotask(autosize);
    const ok = await app.send(deep ? `${RESEARCH_PREFIX} ${value}` : value, sending);
    if (!ok) {
      // keep what the user typed and attached if sending failed
      text = value;
      files = sending;
      research = deep;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="group relative rounded-[26px] border border-line-strong bg-surface shadow-[0_8px_30px_rgb(0_0_0/0.22)] transition focus-within:border-accent/45"
  {ondrop}
  ondragover={(e) => e.preventDefault()}
>
  {#if files.length}
    <div class="flex flex-wrap gap-2 px-3 pt-3">
      {#each files as f, i (i)}
        <div class="group/chip relative flex items-center gap-2 rounded-xl border border-line bg-elevated py-1.5 pr-7 pl-1.5 text-[12px]">
          {#if previews[i]}
            <img src={previews[i]} alt="" class="h-9 w-9 rounded-lg object-cover" />
          {:else}
            <span class="flex h-9 w-9 items-center justify-center rounded-lg bg-accent-soft text-accent-hi"><FileText size={16} /></span>
          {/if}
          <span class="max-w-40 truncate">{f.name}</span>
          <button
            class="absolute top-1 right-1 rounded-md p-0.5 text-faint hover:bg-hover hover:text-text"
            onclick={() => files.splice(i, 1)}
            aria-label="Remove {f.name}"
          >
            <X size={12} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
  <div class="flex items-end gap-1 p-1.5">
    <input
      bind:this={picker}
      type="file"
      multiple
      accept={ACCEPT}
      class="hidden"
      onchange={(e) => {
        addFiles((e.currentTarget as HTMLInputElement).files);
        (e.currentTarget as HTMLInputElement).value = '';
      }}
    />
    <button
      class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-muted transition hover:bg-hover hover:text-text"
      onclick={() => picker?.click()}
      {disabled}
      use:tip={'Attach photos or files (or paste / drop them)'}
      data-tip-side="top"
      aria-label="Attach files"
    >
      <Plus size={19} />
    </button>
    <textarea
      bind:this={input}
      bind:value={text}
      oninput={autosize}
      {onkeydown}
      {onpaste}
      rows="1"
      {placeholder}
      disabled={disabled}
      class="block max-h-60 min-h-9 min-w-0 flex-1 resize-none bg-transparent px-1.5 py-[7px] text-[14.5px] leading-[22px] text-text placeholder:text-faint focus:outline-none scroll-thin"
    ></textarea>
    <button
      class="flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-full transition {research
        ? 'border border-accent/60 bg-accent-soft px-2.5 text-[12px] font-medium text-accent-hi'
        : 'w-9 text-muted hover:bg-hover hover:text-text'}"
      onclick={() => {
        research = !research;
        input?.focus();
      }}
      {disabled}
      use:tip={research ? 'Deep research is on for the next message' : 'Deep research: many sources, a cited report (slower, uses more tokens)'}
      data-tip-side="top"
      aria-pressed={research}
      aria-label="Deep research"
    >
      <Telescope size={16} />
      {#if research}<span>Research</span>{/if}
    </button>
    {#if voice.available && voice.canRecord}
      <button
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition {voice.recording
          ? 'animate-pulse bg-danger-soft text-danger'
          : 'text-muted hover:bg-hover hover:text-text'}"
        onclick={mic}
        {disabled}
        use:tip={voice.recording ? 'Stop and turn it into text' : voice.transcribing ? 'Turning it into text…' : 'Speak (the reply is read aloud)'}
        data-tip-side="top"
        aria-pressed={voice.recording}
        aria-label={voice.recording ? 'Stop recording' : 'Speak'}
      >
        {#if voice.transcribing}<LoaderCircle size={16} class="animate-spin" />{:else if voice.recording}<Square size={12} fill="currentColor" />{:else}<Mic size={17} />{/if}
      </button>
    {/if}
    {#if running && !text.trim() && files.length === 0}
      <button
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-text text-bg transition hover:opacity-85"
        onclick={() => app.stop()}
        use:tip={'Stop'}
        data-tip-side="top"
        aria-label="Stop"
      >
        <Square size={13} fill="currentColor" />
      </button>
    {:else}
      <button
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition {(text.trim() || files.length) && !disabled
          ? 'bg-accent text-on-accent hover:bg-accent-hi'
          : 'bg-hover text-faint'}"
        onclick={submit}
        disabled={(!text.trim() && files.length === 0) || disabled}
        use:tip={running ? 'Queue: sends when the current reply ends' : 'Send'}
        data-tip-side="top"
        aria-label="Send"
      >
        <ArrowUp size={18} strokeWidth={2.4} />
      </button>
    {/if}
  </div>
</div>
