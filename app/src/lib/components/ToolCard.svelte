<script lang="ts">
  import {
    Ban,
    Check,
    ChevronRight,
    CircleSlash,
    Clock,
    FilePen,
    FileText,
    FolderOpen,
    Globe,
    LoaderCircle,
    Search,
    ShieldAlert,
    ShieldCheck,
    ListChecks,
    Send,
    SquareTerminal,
    UserMinus,
    UserPlus,
    Users,
    Wrench,
    X,
  } from '@lucide/svelte';
  import { app } from '../app.svelte';
  import DiffText from './DiffText.svelte';
  import type { ToolStep } from '../types';

  let { step }: { step: ToolStep } = $props();
  let open = $state(false);

  const icons: Record<string, typeof Search> = {
    web_search: Search,
    web_fetch: Globe,
    get_time: Clock,
    list_files: FolderOpen,
    read_file: FileText,
    write_file: FilePen,
    run_command: SquareTerminal,
    list_agents: Users,
    create_agent: UserPlus,
    delegate_task: Send,
    check_task: ListChecks,
    dismiss_agent: UserMinus,
  };
  const Icon = $derived(icons[step.name] ?? Wrench);

  const args = $derived.by(() => {
    try {
      return JSON.parse(step.arguments || '{}') as Record<string, unknown>;
    } catch {
      return {};
    }
  });
  const command = $derived(typeof args.command === 'string' ? args.command : '');
  const duration = $derived(
    step.finished_at && step.started_at ? `${((step.finished_at - step.started_at) / 1000).toFixed(1)}s` : '',
  );
  // A code change's diff, to review before approving.
  const detail = $derived(step.approval_id ? app.pendingApprovals.find((a) => a.id === step.approval_id)?.detail : undefined);
  const prettyArgs = $derived(
    Object.keys(args).length ? JSON.stringify(args, null, 2) : '',
  );
</script>

{#if step.status === 'awaiting_approval' && step.approval_id}
  <div class="rounded-xl border border-warn/40 bg-warn/[0.06] p-3.5 animate-pop-in">
    <div class="flex items-center gap-2 text-[13px] font-semibold text-warn">
      <ShieldAlert size={16} /> Approval needed
    </div>
    {#if detail}
      <p class="mt-1 text-[13px] text-muted">{step.summary}. Review the change:</p>
      <div class="mt-2"><DiffText text={detail} /></div>
    {:else}
      <p class="mt-1 text-[13px] text-muted">The agent wants to {command ? 'run this command' : `use ${step.name}`}:</p>
      <pre
        class="selectable mt-2 max-h-48 overflow-auto rounded-lg border border-line bg-code px-3 py-2 font-mono text-[12.5px] whitespace-pre-wrap">{command ||
          prettyArgs}</pre>
    {/if}
    <div class="mt-3 flex gap-2">
      <button class="btn-primary" onclick={() => app.decide(step.approval_id!, true)}>
        <Check size={14} /> Approve
      </button>
      <button class="btn-outline" onclick={() => app.decide(step.approval_id!, false)}>
        <X size={14} /> Deny
      </button>
    </div>
  </div>
{:else}
  <div class="rounded-xl border border-line bg-surface/60 transition hover:border-line-strong">
    <button class="flex w-full items-center gap-2.5 px-3 py-2 text-left" onclick={() => (open = !open)}>
      <span
        class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md {step.status === 'error'
          ? 'bg-danger-soft text-danger'
          : 'bg-accent-soft text-accent-hi'}"
      >
        <Icon size={13} />
      </span>
      <span class="min-w-0 flex-1 truncate text-[13px] {step.status === 'running' ? 'text-text' : 'text-muted'}">
        {#if step.status === 'running'}<span class="shimmer font-medium">{step.summary}</span>{:else}{step.summary}{/if}
      </span>
      {#if step.auto_approved}
        <span
          class="flex shrink-0 items-center gap-0.5 rounded-full bg-accent-soft px-1.5 py-px text-[10.5px] font-medium text-accent-hi"
          title="Auto-approved: {step.auto_approved}"><ShieldCheck size={10} /> Auto</span
        >
      {/if}
      {#if duration}<span class="shrink-0 text-[11px] text-faint">{duration}</span>{/if}
      <span class="shrink-0">
        {#if step.status === 'running'}
          <LoaderCircle size={14} class="animate-spin text-accent-hi" />
        {:else if step.status === 'done'}
          <Check size={14} class="text-success" />
        {:else if step.status === 'error'}
          <X size={14} class="text-danger" />
        {:else if step.status === 'denied'}
          <Ban size={14} class="text-warn" />
        {:else}
          <CircleSlash size={14} class="text-faint" />
        {/if}
      </span>
      <ChevronRight size={14} class="shrink-0 text-faint transition-transform {open ? 'rotate-90' : ''}" />
    </button>
    {#if open}
      <div class="space-y-2 border-t border-line px-3 py-2.5 animate-fade-in">
        {#if step.auto_approved}
          <div class="flex items-center gap-1.5 text-[12px] text-accent-hi">
            <ShieldCheck size={13} /> Auto-approved by the safety check: {step.auto_approved}
          </div>
        {/if}
        {#if prettyArgs}
          <div>
            <div class="mb-1 text-[11px] font-semibold tracking-wider text-faint uppercase">Input</div>
            <pre class="selectable max-h-40 overflow-auto rounded-lg bg-code px-2.5 py-2 font-mono text-[12px] whitespace-pre-wrap text-muted">{command || prettyArgs}</pre>
          </div>
        {/if}
        {#if step.output}
          <div>
            <div class="mb-1 text-[11px] font-semibold tracking-wider text-faint uppercase">Result</div>
            <pre class="selectable max-h-64 overflow-auto rounded-lg bg-code px-2.5 py-2 font-mono text-[12px] whitespace-pre-wrap text-muted">{step.output}</pre>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
