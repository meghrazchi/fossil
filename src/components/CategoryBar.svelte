<script lang="ts">
  import { X } from '@lucide/svelte';
  import { CATEGORY_BY_KEY } from '../lib/colors';
  import { formatBytes, formatPercent } from '../lib/format';
  import type { FileCategory, GroupEntry, SizeMetric } from '../lib/types';

  interface Props {
    groups: GroupEntry[];
    metric: SizeMetric;
    activeCategory: FileCategory | null;
    onSelectCategory: (cat: FileCategory | null) => void;
  }

  let { groups, metric, activeCategory, onSelectCategory }: Props = $props();

  function getSize(g: GroupEntry): number {
    return metric === 'disk' ? g.disk_size : g.logical_size;
  }

  function handleToggle(cat: FileCategory | null) {
    if (!cat) return;
    onSelectCategory(activeCategory === cat ? null : cat);
  }
</script>

<div class="border-b border-zinc-800/90 bg-zinc-950/90 px-3 py-2">
  <!-- Stacked proportional bar -->
  <div class="flex h-2.5 w-full overflow-hidden rounded-full bg-zinc-900">
    {#each groups as g (g.key)}
      {@const meta = g.category ? CATEGORY_BY_KEY[g.category] : CATEGORY_BY_KEY.other}
      {@const isDimmed = activeCategory !== null && g.category !== activeCategory}
      {#if g.percent >= 0.4}
        <button
          title="{g.label}: {formatBytes(getSize(g))} ({formatPercent(g.percent)})"
          onclick={() => handleToggle(g.category)}
          class="h-full cursor-pointer transition-opacity hover:brightness-110 {isDimmed
            ? 'opacity-25'
            : 'opacity-100'}"
          style="width: {g.percent}%; background-color: {meta.color};"
        ></button>
      {/if}
    {/each}
  </div>

  <!-- Interactive Legend Pills -->
  <div class="mt-2 flex flex-wrap items-center gap-1.5">
    {#each groups.slice(0, 8) as g (g.key)}
      {@const meta = g.category ? CATEGORY_BY_KEY[g.category] : CATEGORY_BY_KEY.other}
      {@const isSelected = activeCategory !== null && g.category === activeCategory}
      <button
        onclick={() => handleToggle(g.category)}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] transition {isSelected
          ? 'border-cyan-400 bg-zinc-800 text-white shadow-sm'
          : 'border-zinc-800/80 bg-zinc-900/60 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800/80'}"
      >
        <span class="h-2 w-2 rounded-full" style="background-color: {meta.color};"></span>
        <span class="font-medium">{g.label}</span>
        <span class="tabular-nums text-zinc-400">{formatBytes(getSize(g), 1)}</span>
        <span class="tabular-nums text-zinc-500">{formatPercent(g.percent)}</span>
      </button>
    {/each}

    {#if activeCategory !== null}
      <button
        onclick={() => onSelectCategory(null)}
        class="inline-flex cursor-pointer items-center gap-1 rounded-md border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 text-[11px] font-medium text-cyan-200 hover:bg-cyan-500/25"
      >
        <X class="h-3 w-3" />
        Clear Filter
      </button>
    {/if}
  </div>
</div>
