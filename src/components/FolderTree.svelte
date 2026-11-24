<script lang="ts">
  import { Folder, File, Link2, ExternalLink, Trash2, Sparkles } from '@lucide/svelte';
  import { CATEGORY_BY_KEY } from '../lib/colors';
  import { formatBytes, formatCount, formatPercent } from '../lib/format';
  import type { NodeItem, SizeMetric } from '../lib/types';

  interface Props {
    items: NodeItem[];
    total: number;
    metric: SizeMetric;
    selectedNodeId: number | null;
    onSelect: (nodeId: number) => void;
    onZoom: (nodeId: number) => void;
    onReveal: (nodeId: number) => void;
    onTrash: (nodeId: number) => void;
    onContextMenu: (nodeId: number, x: number, y: number) => void;
  }

  let {
    items,
    total,
    metric,
    selectedNodeId,
    onSelect,
    onZoom,
    onReveal,
    onTrash,
    onContextMenu,
  }: Props = $props();

  function getSize(n: NodeItem): number {
    return metric === 'disk' ? n.disk_size : n.logical_size;
  }
</script>

<div class="flex h-full flex-col border-r border-zinc-800/90 bg-zinc-950">
  <div
    class="flex items-center justify-between border-b border-zinc-800/80 px-3 py-2 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase"
  >
    <span>Folder Contents</span>
    <span class="tabular-nums text-zinc-500">{formatCount(total)} items</span>
  </div>

  <div class="flex-1 overflow-y-auto">
    {#if items.length === 0}
      <div class="px-3 py-8 text-center text-xs text-zinc-500">Empty or filtered out</div>
    {:else}
      <ul class="divide-y divide-zinc-900/80">
        {#each items as item (item.id)}
          {@const cat = CATEGORY_BY_KEY[item.category] ?? CATEGORY_BY_KEY.other}
          {@const isSelected = selectedNodeId === item.id}
          <li
            oncontextmenu={(e) => {
              e.preventDefault();
              onSelect(item.id);
              onContextMenu(item.id, e.clientX, e.clientY);
            }}
            class="group relative flex items-center justify-between gap-2 px-2.5 py-1.5 text-xs transition {isSelected
              ? 'bg-cyan-500/15 text-white'
              : 'hover:bg-zinc-900/80'}"
          >
            <!-- Subtle proportional background bar -->
            <div
              class="pointer-events-none absolute inset-y-0 left-0 opacity-12"
              style="width: {Math.min(
                100,
                item.percent_of_parent,
              )}%; background-color: {cat.color};"
            ></div>

            <button
              onclick={() => onSelect(item.id)}
              ondblclick={() => {
                if (item.kind === 'directory') onZoom(item.id);
              }}
              class="relative flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left"
            >
              {#if item.kind === 'directory'}
                <Folder class="h-3.5 w-3.5 shrink-0" style="color: {cat.color};" />
              {:else if item.kind === 'symlink'}
                <Link2 class="h-3.5 w-3.5 shrink-0 text-zinc-400" />
              {:else}
                <File class="h-3.5 w-3.5 shrink-0" style="color: {cat.color};" />
              {/if}

              <span class="truncate font-medium text-zinc-200">{item.name}</span>

              {#if item.is_bloat}
                <span
                  title={item.bloat_reason ?? 'Regeneratable build artifact'}
                  class="inline-flex shrink-0 items-center gap-0.5 rounded bg-cyan-500/15 px-1 py-0.2 text-[9px] font-medium text-cyan-300"
                >
                  <Sparkles class="h-2.5 w-2.5" />
                  Bloat
                </span>
              {/if}
            </button>

            <div class="relative flex shrink-0 items-center gap-2">
              <span class="tabular-nums text-[11px] font-medium text-zinc-200">
                {formatBytes(getSize(item), 1)}
              </span>
              <span class="w-10 text-right tabular-nums text-[10px] text-zinc-500">
                {formatPercent(item.percent_of_parent)}
              </span>

              <div class="hidden items-center gap-0.5 group-hover:flex">
                <button
                  title="Reveal in File Manager"
                  onclick={() => onReveal(item.id)}
                  class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                >
                  <ExternalLink class="h-3 w-3" />
                </button>
                <button
                  title="Move to Trash"
                  onclick={() => onTrash(item.id)}
                  class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-rose-500/20 hover:text-rose-300"
                >
                  <Trash2 class="h-3 w-3" />
                </button>
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
