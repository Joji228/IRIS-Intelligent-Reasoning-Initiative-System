<script lang="ts">
  import { CalendarClock, Check, CircleAlert, CircleCheck, Copy, FileText, RotateCcw } from '@lucide/svelte';
  import { Rocket, Sparkles, Square, Telescope, Volume2 } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import { voice } from '../voice.svelte';
  import { clockTime } from '../time';
  import { tip } from '../tooltip';
  import type { Agent, Message } from '../types';
  import AgentAvatar from './AgentAvatar.svelte';
  import Markdown from './Markdown.svelte';
  import QuestionCard from './QuestionCard.svelte';
  import Thinking from './Thinking.svelte';
  import ToolCard from './ToolCard.svelte';
  import WorkCard from './WorkCard.svelte';

  // Matches the engine's team::UPDATE_PREFIX.
  const UPDATE_PREFIX = '[I.R.I.S. update]';
  // Matches the engine's design::SETUP_PREFIX: how a summoned orb was set up.
  const SETUP_PREFIX = '[I.R.I.S. setup]';
  // Matches the engine's develop::RELEASE_PREFIX: how an update went.
  const RELEASE_PREFIX = '[I.R.I.S. release]';
  // Matches the engine's chat::RESEARCH_PREFIX.
  const RESEARCH_PREFIX = '[Deep research]';


  let { message, agent, isLast }: { message: Message; agent?: Agent; isLast: boolean } = $props();

  // Matches the engine's routines::ROUTINE_PREFIX: "[Routine: name] task".
  const ROUTINE_PREFIX = '[Routine:';
  const routineName = $derived(
    message.role === 'user' && message.content.startsWith(ROUTINE_PREFIX)
      ? message.content.slice(ROUTINE_PREFIX.length, message.content.indexOf(']')).trim()
      : null,
  );
  const streaming = $derived(message.status === 'streaming');
  // Replies from before tools existed have no rounds: show them as one.
  const rounds = $derived(
    message.rounds?.length ? message.rounds : [{ reasoning: message.reasoning, content: message.content, tools: [] }],
  );
  const lastRound = $derived(rounds[rounds.length - 1]);
  const idle = $derived(streaming && !lastRound.content && !lastRound.reasoning && lastRound.tools.length === 0);
  // The main agent's tool steps fold into one box; only its answer (the last
  // round, once it has no tools) and steps waiting for the user stay out.
  const compact = $derived(agent?.role === 'main' && rounds.some((r) => r.tools.length > 0));
  // A reply that ends on a question (ask_user): the card is its answer.
  const asks = (r: { tools: { name: string }[] }) => r.tools.length > 0 && r.tools.every((t) => t.name === 'ask_user');
  const finalIdx = $derived(lastRound.tools.length === 0 || asks(lastRound) ? rounds.length - 1 : rounds.length);
  // What the user answered: their next message after this reply.
  const answer = $derived.by(() => {
    const i = app.messages.findIndex((m) => m.id === message.id);
    if (i < 0) return null;
    const next = app.messages.slice(i + 1).find((m) => m.role === 'user' && !m.content.startsWith('[I.R.I.S.') && !m.content.startsWith('[Routine:'));
    return next ? next.content : null;
  });
  const workRounds = $derived(rounds.slice(0, finalIdx));
  const awaiting = $derived(
    workRounds.flatMap((r) => r.tools).filter((s) => s.status === 'awaiting_approval' && s.approval_id),
  );
  let copied = $state(false);

  async function copy() {
    await navigator.clipboard.writeText(message.content);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const usage = $derived(
    message.prompt_tokens != null
      ? `${message.model ?? ''} · ${message.prompt_tokens} in / ${message.completion_tokens} out`
      : (message.model ?? ''),
  );
</script>

{#if routineName !== null}
  <!-- A routine woke the agent -->
  <div class="flex justify-center animate-fade-in">
    <span class="flex items-center gap-1.5 rounded-full bg-surface px-3 py-1 text-[11.5px] text-faint" title={message.content}>
      <CalendarClock size={12} class="text-accent-hi" /> Routine “{routineName}”
    </span>
  </div>
{:else if message.role === 'user' && message.content.startsWith(SETUP_PREFIX)}
  <!-- A summoned orb took on its job (and what Iris let it use) -->
  <div class="flex justify-center animate-fade-in">
    <span class="flex max-w-[min(36rem,90%)] items-start gap-1.5 rounded-2xl bg-surface px-3.5 py-1.5 text-[11.5px] leading-relaxed text-faint">
      <Sparkles size={12} class="mt-0.5 shrink-0 text-accent-hi" />
      <span>{message.content.slice(SETUP_PREFIX.length).replace(' Now do the user\'s request above.', '').trim()}</span>
    </span>
  </div>
{:else if message.role === 'user' && message.content.startsWith(RELEASE_PREFIX)}
  <!-- The engine telling the Developer how an update went -->
  <div class="flex justify-center animate-fade-in">
    <span class="flex max-w-[min(36rem,90%)] items-start gap-1.5 rounded-2xl bg-surface px-3.5 py-1.5 text-[11.5px] leading-relaxed text-faint" title={message.content}>
      <Rocket size={12} class="mt-0.5 shrink-0 {/FAILED|never reported/.test(message.content) ? 'text-danger' : 'text-success'}" />
      <span>{message.content.slice(RELEASE_PREFIX.length).split('\n')[0].split('. ')[0].trim()}.</span>
    </span>
  </div>
{:else if message.role === 'user' && message.content.startsWith(UPDATE_PREFIX)}
  <!-- The engine bringing the coordinator back once a delegated task is done -->
  <div class="flex justify-center animate-fade-in">
    <span class="flex items-center gap-1.5 rounded-full bg-surface px-3 py-1 text-[11.5px] text-faint">
      <CircleCheck size={12} class="text-success" />
      {message.content.slice(UPDATE_PREFIX.length).split(' has finished')[0].trim()} finished its task · {agent?.name ?? 'the agent'} is reporting back
    </span>
  </div>
{:else if message.role === 'user'}
  {#if message.attachments?.length}
    <div class="-mb-5 flex flex-wrap justify-end gap-2 animate-fade-in">
      {#each message.attachments as a (a.id)}
        {#if a.kind === 'image' && app.api}
          {#await app.api.attachmentSrc(message.thread_id, a.id) then src}
            <a href={src} target="_blank" rel="noreferrer">
              <img src={src} alt={a.name} class="max-h-48 max-w-[min(16rem,70vw)] rounded-xl border border-line object-cover" />
            </a>
          {:catch}
            <span class="rounded-xl border border-line bg-elevated px-3 py-2 text-[12.5px] text-faint">{a.name}</span>
          {/await}
        {:else}
          <span class="flex items-center gap-2 rounded-xl border border-line bg-elevated px-3 py-2 text-[12.5px]">
            <FileText size={15} class="text-accent-hi" />
            <span class="max-w-52 truncate">{a.name}</span>
            <span class="text-[11px] text-faint">{a.size < 1024 * 1024 ? `${Math.max(1, Math.round(a.size / 1024))} KB` : `${(a.size / 1048576).toFixed(1)} MB`}</span>
          </span>
        {/if}
      {/each}
    </div>
  {/if}
  {#if message.content}
  {@const deep = message.content.startsWith(RESEARCH_PREFIX)}
  <!-- The user's own bubble (violet, right), with its time tucked in the corner -->
  <div class="group flex items-end justify-end gap-1 animate-fade-in" id="msg-{message.id}">
    <button
      class="mb-0.5 rounded-md p-1.5 text-faint opacity-0 transition group-hover:opacity-100 hover:bg-hover hover:text-text"
      onclick={copy}
      use:tip={copied ? 'Copied' : 'Copy'}
      data-tip-side="left"
      aria-label="Copy"
    >
      {#if copied}<Check size={13} />{:else}<Copy size={13} />{/if}
    </button>
    <div
      class="selectable max-w-[78%] rounded-[20px] rounded-br-md bg-bubble-me px-4 pt-2 pb-1.5 text-[14.5px] leading-relaxed text-on-bubble-me whitespace-pre-wrap shadow-[0_6px_18px_-6px_color-mix(in_srgb,var(--c-bubble-me)_60%,transparent)]"
    >
      {#if deep}<span class="mb-1 flex w-fit items-center gap-1 rounded-full bg-white/15 px-2 py-0.5 text-[11px] font-medium"><Telescope size={11} /> Deep research</span>{/if}{deep ? message.content.slice(RESEARCH_PREFIX.length).trim() : message.content}<span class="msg-time float-right mt-2 ml-3 text-[10.5px] leading-none text-white/65 select-none">{clockTime(message.created_at)}</span>
    </div>
  </div>
  {/if}
{:else if message.role === 'assistant'}
  <div class="group flex gap-3.5 animate-fade-in" id="msg-{message.id}">
    <div class="pt-0.5"><AgentAvatar {agent} size={30} working={streaming} /></div>
    <div class="min-w-0 flex-1">
      <div class="mb-1 flex items-baseline gap-2">
        <span class="text-[13px] font-semibold">{agent?.name ?? 'Agent'}</span>
        <span class="text-[11px] text-faint">{clockTime(message.created_at)}</span>
      </div>

      {#if compact}
        {#if workRounds.some((r) => r.tools.length)}<WorkCard rounds={workRounds} live={streaming} />{/if}
        {#each awaiting as step (step.id)}<div class="my-2"><ToolCard {step} /></div>{/each}
        {#if finalIdx < rounds.length}
          {#if lastRound.reasoning}
            <Thinking text={lastRound.reasoning} live={streaming && !lastRound.content} />
          {/if}
          {#if lastRound.content}<div class="bubble"><Markdown text={lastRound.content} /></div>{/if}
          {#each lastRound.tools.filter((t) => t.name === 'ask_user') as step (step.id)}
            <QuestionCard {step} {answer} live={!streaming} />
          {/each}
        {/if}
      {:else}
      {#each rounds as round, i}
        {@const isLastRound = i === rounds.length - 1}
        {#if round.reasoning}
          <Thinking text={round.reasoning} live={streaming && isLastRound && !round.content && round.tools.length === 0} />
        {/if}
        {#if round.content}
          <div class="bubble"><Markdown text={round.content} /></div>
        {/if}
        {#if round.tools.length}
          <div class="my-2.5 space-y-1.5">
            {#each round.tools as step (step.id)}
              {#if step.name === 'ask_user'}
                <QuestionCard {step} {answer} live={!streaming} />
              {:else}
                <ToolCard {step} />
              {/if}
            {/each}
          </div>
        {/if}
      {/each}
      {/if}

      {#if idle}
        <div class="py-1 text-[14px]"><span class="shimmer font-medium">Thinking…</span></div>
      {:else if streaming && lastRound.content && lastRound.tools.length === 0}
        <span class="ml-0.5 inline-block h-4 w-[3px] translate-y-0.5 animate-pulse rounded-full bg-accent-hi"></span>
      {/if}

      {#if message.status === 'error'}
        <div class="mt-2 flex items-start gap-2.5 rounded-xl border border-danger/30 bg-danger-soft px-3.5 py-2.5 text-[13px]">
          <CircleAlert size={16} class="mt-0.5 shrink-0 text-danger" />
          <div class="min-w-0 flex-1">
            <div class="font-medium text-danger">The reply failed</div>
            <div class="selectable mt-0.5 break-words text-muted">{message.error}</div>
          </div>
          {#if isLast}
            <button
              class="shrink-0 rounded-lg px-2.5 py-1 text-[12px] font-medium text-text hover:bg-hover"
              onclick={() => app.regenerate()}>Retry</button
            >
          {/if}
        </div>
      {:else if message.status === 'cancelled'}
        <div class="mt-1.5 text-[12px] text-faint italic">Stopped</div>
      {/if}

      {#if !streaming && message.content}
        <div class="mt-1.5 flex items-center gap-0.5 opacity-0 transition group-hover:opacity-100">
          <button
            class="rounded-md p-1.5 text-faint hover:bg-hover hover:text-text"
            onclick={copy}
            use:tip={copied ? 'Copied' : 'Copy'}
            data-tip-side="bottom"
            aria-label="Copy"
          >
            {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
          </button>
          {#if voice.canSpeak}
            <button
              class="rounded-md p-1.5 hover:bg-hover hover:text-text {voice.speakingId === message.id ? 'text-accent-hi' : 'text-faint'}"
              onclick={() => voice.speak(message.id, message.content)}
              use:tip={voice.speakingId === message.id ? 'Stop reading' : 'Read aloud'}
              data-tip-side="bottom"
              aria-label={voice.speakingId === message.id ? 'Stop reading' : 'Read aloud'}
            >
              {#if voice.speakingId === message.id}<Square size={13} fill="currentColor" />{:else}<Volume2 size={14} />{/if}
            </button>
          {/if}
          {#if isLast}
            <button
              class="rounded-md p-1.5 text-faint hover:bg-hover hover:text-text"
              onclick={() => app.regenerate()}
              use:tip={'Regenerate'}
              data-tip-side="bottom"
              aria-label="Regenerate"
            >
              <RotateCcw size={14} />
            </button>
          {/if}
          {#if usage}<span class="ml-2 text-[11px] text-faint">{usage}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* The agent's bubble: hugs short replies, takes the full width for long ones. */
  .bubble {
    width: fit-content;
    max-width: 100%;
    margin: 0.15rem 0;
    padding: 0.6rem 1rem;
    border: 1px solid var(--c-bubble-line);
    border-radius: 20px;
    border-top-left-radius: 6px;
    background: var(--c-bubble);
  }
  .bubble + .bubble {
    border-top-left-radius: 20px;
  }
</style>
