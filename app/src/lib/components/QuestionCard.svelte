<script lang="ts">
  // An agent's question with clickable answers (its ask_user tool). Picking
  // one, or typing your own, sends it as your next message; once answered
  // the card shows what you chose.
  import { Check, CornerDownLeft, MessageCircleQuestion } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import type { ToolStep } from '../types';

  let { step, answer, live }: { step: ToolStep; answer: string | null; live: boolean } = $props();

  const args = $derived.by(() => {
    try {
      return JSON.parse(step.arguments) as { question?: string; options?: string[]; allow_other?: boolean };
    } catch {
      return {};
    }
  });
  const options = $derived((args.options ?? []).filter((o) => typeof o === 'string' && o.trim()));
  const allowOther = $derived(args.allow_other !== false);
  const open = $derived(answer === null && live && step.status === 'done');
  let other = $state('');
  let sending = $state(false);

  async function pick(text: string) {
    const value = text.trim();
    if (!value || sending || !open) return;
    sending = true;
    await app.send(value);
    sending = false;
    other = '';
  }
</script>

{#if args.question}
  <div class="my-2 max-w-xl overflow-hidden rounded-2xl border border-line-strong bg-surface animate-fade-in">
    <div class="flex items-start gap-2.5 px-4 pt-3.5 pb-2.5">
      <MessageCircleQuestion size={17} class="mt-0.5 shrink-0 text-accent-hi" />
      <div class="text-[14px] leading-snug font-semibold">{args.question}</div>
    </div>
    <div class="flex flex-col">
      {#each options as o, i (i)}
        {@const chosen = answer !== null && answer.trim() === o.trim()}
        <button
          class="flex items-center gap-3 border-t border-line px-4 py-2.5 text-left text-[13.5px] transition {open
            ? 'hover:bg-hover'
            : chosen
              ? 'bg-accent-soft text-text'
              : 'text-faint'}"
          disabled={!open || sending}
          onclick={() => pick(o)}
        >
          <span
            class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md border text-[11.5px] font-semibold {chosen
              ? 'border-accent bg-accent text-on-accent'
              : 'border-line-strong text-muted'}"
          >
            {#if chosen}<Check size={13} strokeWidth={3} />{:else}{String.fromCharCode(65 + i)}{/if}
          </span>
          <span class="min-w-0 flex-1">{o}</span>
        </button>
      {/each}
      {#if open && allowOther}
        <form
          class="flex items-center gap-2 border-t border-line px-3 py-2"
          onsubmit={(e) => {
            e.preventDefault();
            pick(other);
          }}
        >
          <input
            class="min-w-0 flex-1 bg-transparent px-1 py-1 text-[13.5px] placeholder:text-faint focus:outline-none"
            placeholder="Type your own answer"
            bind:value={other}
            disabled={sending}
          />
          <button class="rounded-lg p-1.5 text-faint transition hover:bg-hover hover:text-text disabled:opacity-40" disabled={!other.trim() || sending} aria-label="Send answer">
            <CornerDownLeft size={15} />
          </button>
        </form>
      {:else if answer !== null && !options.some((o) => o.trim() === answer.trim())}
        <div class="flex items-center gap-2 border-t border-line bg-accent-soft px-4 py-2.5 text-[13.5px]">
          <Check size={14} class="shrink-0 text-accent-hi" /> <span class="truncate">{answer}</span>
        </div>
      {/if}
    </div>
  </div>
{/if}
