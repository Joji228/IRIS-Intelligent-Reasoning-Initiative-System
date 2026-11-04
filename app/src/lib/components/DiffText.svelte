<script lang="ts">
  // A unified diff (or any text) with added / removed lines colored, for
  // approving code changes.
  let { text, maxHeight = '22rem' }: { text: string; maxHeight?: string } = $props();

  const lines = $derived(
    text.split('\n').map((l) => ({
      text: l,
      kind:
        l.startsWith('+++') || l.startsWith('---') || l.startsWith('diff --git') || l.startsWith('index ')
          ? 'file'
          : l.startsWith('+')
            ? 'add'
            : l.startsWith('-')
              ? 'del'
              : l.startsWith('@@')
                ? 'hunk'
                : 'ctx',
    })),
  );
</script>

<pre
  class="selectable overflow-auto rounded-lg border border-line bg-code py-2 font-mono text-[11.5px] leading-[1.45] scroll-thin"
  style:max-height={maxHeight}>{#each lines as l, i (i)}<span
      class="block px-3 whitespace-pre {l.kind === 'add'
        ? 'bg-success/12 text-success'
        : l.kind === 'del'
          ? 'bg-danger/12 text-danger'
          : l.kind === 'hunk'
            ? 'text-accent-hi'
            : l.kind === 'file'
              ? 'font-semibold text-text'
              : 'text-muted'}">{l.text || ' '}</span
    >{/each}</pre>
