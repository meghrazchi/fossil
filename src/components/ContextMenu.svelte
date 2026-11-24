<script lang="ts">
  import { ExternalLink, Copy, Trash2, AlertTriangle, ZoomIn } from '@lucide/svelte';
  import type { NodeItem } from '../lib/types';

  interface Props {
    node: NodeItem;
    x: number;
    y: number;
    onClose: () => void;
    onZoom: (nodeId: number) => void;
    onReveal: (nodeId: number) => void;
    onCopyPath: (path: string) => void;
    onTrash: (nodeId: number) => void;
    onRequestPermanentDelete: (node: NodeItem) => void;
  }

  let {
    node,
    x,
    y,
    onClose,
    onZoom,
    onReveal,
    onCopyPath,
    onTrash,
    onRequestPermanentDelete,
  }: Props = $props();
</script>

<svelte:window onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} />

<div
  role="menu"
  tabindex="-1"
  onclick={(e) => e.stopPropagation()}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
  class="fixed z-50 w-56 rounded-xl border border-zinc-700/90 bg-zinc-900/95 p-1.5 text-xs shadow-2xl backdrop-blur-md"
  style="left: {Math.min(x, window.innerWidth - 236)}px; top: {Math.min(
    y,
    window.innerHeight - 210,
  )}px;"
>
  <div
    class="truncate border-b border-zinc-800 px-2 py-1.5 text-[11px] font-semibold text-zinc-300"
  >
    {node.name}
  </div>

  {#if node.kind === 'directory'}
    <button
      onclick={() => {
        onZoom(node.id);
        onClose();
      }}
      class="mt-1 flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-zinc-200 hover:bg-zinc-800"
    >
      <ZoomIn class="h-3.5 w-3.5 text-cyan-400" />
      Zoom Into Folder
    </button>
  {/if}

  <button
    onclick={() => {
      onReveal(node.id);
      onClose();
    }}
    class="mt-0.5 flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-zinc-200 hover:bg-zinc-800"
  >
    <ExternalLink class="h-3.5 w-3.5 text-zinc-400" />
    Reveal in File Manager
  </button>

  <button
    onclick={() => {
      onCopyPath(node.path);
      onClose();
    }}
    class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-zinc-200 hover:bg-zinc-800"
  >
    <Copy class="h-3.5 w-3.5 text-zinc-400" />
    Copy Full Path
  </button>

  <div class="my-1 border-t border-zinc-800"></div>

  <button
    onclick={() => {
      onTrash(node.id);
      onClose();
    }}
    class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-amber-300 hover:bg-amber-500/15"
  >
    <Trash2 class="h-3.5 w-3.5" />
    Move to Trash
  </button>

  <button
    onclick={() => {
      onRequestPermanentDelete(node);
      onClose();
    }}
    class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-rose-400 hover:bg-rose-500/15"
  >
    <AlertTriangle class="h-3.5 w-3.5" />
    Delete Permanently...
  </button>
</div>
