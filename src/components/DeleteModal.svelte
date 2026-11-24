<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { formatBytes } from '../lib/format';
  import type { NodeItem } from '../lib/types';

  interface Props {
    node: NodeItem;
    onCancel: () => void;
    onConfirm: (nodeId: number) => void;
  }

  let { node, onCancel, onConfirm }: Props = $props();
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
  role="dialog"
  aria-modal="true"
>
  <div class="w-full max-w-md rounded-2xl border border-rose-500/40 bg-zinc-900 p-5 shadow-2xl">
    <div class="flex items-start gap-3">
      <div
        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-rose-500/15 text-rose-400"
      >
        <AlertTriangle class="h-5 w-5" />
      </div>
      <div class="min-w-0">
        <h3 class="text-sm font-semibold text-zinc-100">Delete Permanently?</h3>
        <p class="mt-1 text-xs text-zinc-400">
          This bypasses the OS Trash/Recycle Bin and <strong class="text-rose-300"
            >cannot be undone</strong
          >.
        </p>
      </div>
    </div>

    <div class="mt-4 rounded-lg border border-zinc-800 bg-zinc-950 p-3 text-xs">
      <div class="font-semibold text-zinc-100">{node.name}</div>
      <div class="mt-1 break-all font-mono text-[11px] text-zinc-400">{node.path}</div>
      <div class="mt-2 flex items-center justify-between border-t border-zinc-800/80 pt-2">
        <span class="text-zinc-500">Disk space reclaimed</span>
        <span class="font-semibold tabular-nums text-rose-300">{formatBytes(node.disk_size)}</span>
      </div>
    </div>

    <div class="mt-5 flex justify-end gap-2">
      <button
        onclick={onCancel}
        class="cursor-pointer rounded-lg border border-zinc-700 bg-zinc-800 px-3.5 py-1.5 text-xs font-medium text-zinc-200 hover:bg-zinc-700"
      >
        Cancel
      </button>
      <button
        onclick={() => onConfirm(node.id)}
        class="cursor-pointer rounded-lg bg-rose-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-rose-500"
      >
        Delete Permanently
      </button>
    </div>
  </div>
</div>
