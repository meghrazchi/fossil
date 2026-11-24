<script lang="ts">
  import {
    ExternalLink,
    Trash2,
    File,
    Folder,
    Sparkles,
    Layers,
    FileCode2,
    Clock,
    BarChart3,
  } from '@lucide/svelte';
  import { CATEGORY_BY_KEY } from '../lib/colors';
  import { formatBytes, formatCount, formatDate, formatPercent } from '../lib/format';
  import type { FileCategory, GroupEntry, NodeItem, SizeMetric } from '../lib/types';

  export type InspectorTab = 'largest' | 'category' | 'extension' | 'bloat' | 'tiers';

  interface Props {
    activeTab: InspectorTab;
    metric: SizeMetric;
    largestFiles: NodeItem[];
    largestTotal: number;
    categoryGroups: GroupEntry[];
    extensionGroups: GroupEntry[];
    sizeTierGroups: GroupEntry[];
    ageGroups: GroupEntry[];
    bloatDirs: NodeItem[];
    activeExtFilter: string | null;
    onTabChange: (tab: InspectorTab) => void;
    onSelectCategory: (cat: FileCategory | null) => void;
    onSelectExtension: (ext: string | null) => void;
    onZoom: (nodeId: number) => void;
    onReveal: (nodeId: number) => void;
    onTrash: (nodeId: number) => void;
    onContextMenu: (nodeId: number, x: number, y: number) => void;
  }

  let {
    activeTab,
    metric,
    largestFiles,
    largestTotal,
    categoryGroups,
    extensionGroups,
    sizeTierGroups,
    ageGroups,
    bloatDirs,
    activeExtFilter,
    onTabChange,
    onSelectCategory,
    onSelectExtension,
    onZoom,
    onReveal,
    onTrash,
    onContextMenu,
  }: Props = $props();

  function nodeSize(n: NodeItem): number {
    return metric === 'disk' ? n.disk_size : n.logical_size;
  }

  function groupSize(g: GroupEntry): number {
    return metric === 'disk' ? g.disk_size : g.logical_size;
  }
</script>

<div class="flex h-full flex-col border-t border-zinc-800/90 bg-zinc-950">
  <!-- Tabs Bar -->
  <div class="flex items-center justify-between border-b border-zinc-800/80 px-2.5 py-1.5">
    <div class="flex items-center gap-1">
      <button
        onclick={() => onTabChange('largest')}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition {activeTab ===
        'largest'
          ? 'bg-zinc-800 text-white'
          : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'}"
      >
        <BarChart3 class="h-3.5 w-3.5 text-cyan-400" />
        Largest Files
        <span class="rounded bg-zinc-900/90 px-1.5 py-0.2 text-[10px] tabular-nums text-zinc-400">
          {formatCount(largestTotal)}
        </span>
      </button>

      <button
        onclick={() => onTabChange('category')}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition {activeTab ===
        'category'
          ? 'bg-zinc-800 text-white'
          : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'}"
      >
        <Layers class="h-3.5 w-3.5 text-purple-400" />
        By Category
      </button>

      <button
        onclick={() => onTabChange('extension')}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition {activeTab ===
        'extension'
          ? 'bg-zinc-800 text-white'
          : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'}"
      >
        <FileCode2 class="h-3.5 w-3.5 text-amber-400" />
        By Extension
      </button>

      <button
        onclick={() => onTabChange('bloat')}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition {activeTab ===
        'bloat'
          ? 'bg-zinc-800 text-white'
          : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'}"
      >
        <Sparkles class="h-3.5 w-3.5 text-emerald-400" />
        Heavy Bloat Dirs
        {#if bloatDirs.length > 0}
          <span class="rounded bg-cyan-500/15 px-1.5 py-0.2 text-[10px] tabular-nums text-cyan-300">
            {bloatDirs.length}
          </span>
        {/if}
      </button>

      <button
        onclick={() => onTabChange('tiers')}
        class="inline-flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium transition {activeTab ===
        'tiers'
          ? 'bg-zinc-800 text-white'
          : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'}"
      >
        <Clock class="h-3.5 w-3.5 text-rose-400" />
        Size &amp; Age Groups
      </button>
    </div>

    {#if activeExtFilter}
      <button
        onclick={() => onSelectExtension(null)}
        class="cursor-pointer rounded border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-200 hover:bg-amber-500/25"
      >
        Extension: .{activeExtFilter} &times;
      </button>
    {/if}
  </div>

  <!-- Tab Body -->
  <div class="flex-1 overflow-auto">
    {#if activeTab === 'largest'}
      <table class="w-full border-collapse text-left text-xs">
        <thead class="sticky top-0 z-10 bg-zinc-900/95 text-[11px] text-zinc-400 backdrop-blur">
          <tr>
            <th class="px-3 py-1.5 font-medium">File</th>
            <th class="px-3 py-1.5 font-medium">Category</th>
            <th class="px-3 py-1.5 text-right font-medium">Disk Usage</th>
            <th class="px-3 py-1.5 text-right font-medium">Logical</th>
            <th class="px-3 py-1.5 text-right font-medium">Modified</th>
            <th class="px-3 py-1.5 text-right font-medium">Actions</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-900">
          {#each largestFiles as f (f.id)}
            {@const cat = CATEGORY_BY_KEY[f.category] ?? CATEGORY_BY_KEY.other}
            <tr
              oncontextmenu={(e) => {
                e.preventDefault();
                onContextMenu(f.id, e.clientX, e.clientY);
              }}
              class="group hover:bg-zinc-900/80"
            >
              <td class="max-w-[340px] px-3 py-1.5">
                <div class="flex items-center gap-2">
                  <File class="h-3.5 w-3.5 shrink-0" style="color: {cat.color};" />
                  <div class="min-w-0">
                    <div class="truncate font-medium text-zinc-100">{f.name}</div>
                    <div class="truncate text-[10px] text-zinc-500" title={f.path}>{f.path}</div>
                  </div>
                </div>
              </td>
              <td class="px-3 py-1.5">
                <span class="rounded border px-1.5 py-0.5 text-[10px] font-medium {cat.badgeClass}">
                  {cat.label}
                </span>
              </td>
              <td class="px-3 py-1.5 text-right font-semibold text-zinc-100 tabular-nums">
                {formatBytes(f.disk_size)}
              </td>
              <td class="px-3 py-1.5 text-right text-zinc-400 tabular-nums">
                {formatBytes(f.logical_size)}
              </td>
              <td class="px-3 py-1.5 text-right text-zinc-400 tabular-nums">
                {formatDate(f.modified_ts)}
              </td>
              <td class="px-3 py-1.5 text-right">
                <div class="inline-flex items-center gap-1">
                  <button
                    title="Reveal in File Manager"
                    onclick={() => onReveal(f.id)}
                    class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                  >
                    <ExternalLink class="h-3.5 w-3.5" />
                  </button>
                  <button
                    title="Move to Trash"
                    onclick={() => onTrash(f.id)}
                    class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-rose-500/20 hover:text-rose-300"
                  >
                    <Trash2 class="h-3.5 w-3.5" />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if activeTab === 'category'}
      <table class="w-full border-collapse text-left text-xs">
        <thead class="sticky top-0 z-10 bg-zinc-900/95 text-[11px] text-zinc-400">
          <tr>
            <th class="px-3 py-1.5 font-medium">Category</th>
            <th class="px-3 py-1.5 font-medium">Share</th>
            <th class="px-3 py-1.5 text-right font-medium">Files</th>
            <th class="px-3 py-1.5 text-right font-medium">Disk Usage</th>
            <th class="px-3 py-1.5 text-right font-medium">Logical</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-900">
          {#each categoryGroups as g (g.key)}
            {@const meta = g.category ? CATEGORY_BY_KEY[g.category] : CATEGORY_BY_KEY.other}
            <tr
              onclick={() => {
                if (g.category) {
                  onSelectCategory(g.category);
                  onTabChange('largest');
                }
              }}
              class="cursor-pointer hover:bg-zinc-900/80"
            >
              <td class="px-3 py-2 font-medium text-zinc-100">
                <div class="flex items-center gap-2">
                  <span class="h-2.5 w-2.5 rounded-full" style="background-color: {meta.color};"
                  ></span>
                  {g.label}
                </div>
              </td>
              <td class="w-56 px-3 py-2">
                <div class="flex items-center gap-2">
                  <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-zinc-800">
                    <div
                      class="h-full rounded-full"
                      style="width: {Math.min(100, g.percent)}%; background-color: {meta.color};"
                    ></div>
                  </div>
                  <span class="w-11 text-right text-[11px] tabular-nums text-zinc-400">
                    {formatPercent(g.percent)}
                  </span>
                </div>
              </td>
              <td class="px-3 py-2 text-right tabular-nums text-zinc-300">
                {formatCount(g.file_count)}
              </td>
              <td class="px-3 py-2 text-right font-semibold tabular-nums text-zinc-100">
                {formatBytes(g.disk_size)}
              </td>
              <td class="px-3 py-2 text-right tabular-nums text-zinc-400">
                {formatBytes(g.logical_size)}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if activeTab === 'extension'}
      <table class="w-full border-collapse text-left text-xs">
        <thead class="sticky top-0 z-10 bg-zinc-900/95 text-[11px] text-zinc-400">
          <tr>
            <th class="px-3 py-1.5 font-medium">Extension</th>
            <th class="px-3 py-1.5 font-medium">Category</th>
            <th class="px-3 py-1.5 font-medium">Share</th>
            <th class="px-3 py-1.5 text-right font-medium">Files</th>
            <th class="px-3 py-1.5 text-right font-medium">Size</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-900">
          {#each extensionGroups as g (g.key)}
            {@const meta = g.category ? CATEGORY_BY_KEY[g.category] : CATEGORY_BY_KEY.other}
            <tr
              onclick={() => {
                if (g.key) {
                  onSelectExtension(g.key);
                  onTabChange('largest');
                }
              }}
              class="cursor-pointer hover:bg-zinc-900/80"
            >
              <td class="px-3 py-1.5 font-mono font-semibold text-zinc-100">{g.label}</td>
              <td class="px-3 py-1.5">
                <span class="rounded border px-1.5 py-0.5 text-[10px] {meta.badgeClass}">
                  {meta.label}
                </span>
              </td>
              <td class="w-48 px-3 py-1.5">
                <div class="flex items-center gap-2">
                  <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-zinc-800">
                    <div
                      class="h-full rounded-full"
                      style="width: {Math.min(100, g.percent)}%; background-color: {meta.color};"
                    ></div>
                  </div>
                  <span class="w-11 text-right text-[11px] tabular-nums text-zinc-400">
                    {formatPercent(g.percent)}
                  </span>
                </div>
              </td>
              <td class="px-3 py-1.5 text-right tabular-nums text-zinc-300">
                {formatCount(g.file_count)}
              </td>
              <td class="px-3 py-1.5 text-right font-semibold tabular-nums text-zinc-100">
                {formatBytes(groupSize(g))}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if activeTab === 'bloat'}
      {#if bloatDirs.length === 0}
        <div class="p-8 text-center text-xs text-zinc-500">
          No regeneratable build or cache directories detected in this folder.
        </div>
      {:else}
        <table class="w-full border-collapse text-left text-xs">
          <thead class="sticky top-0 z-10 bg-zinc-900/95 text-[11px] text-zinc-400">
            <tr>
              <th class="px-3 py-1.5 font-medium">Directory</th>
              <th class="px-3 py-1.5 font-medium">What Is It</th>
              <th class="px-3 py-1.5 text-right font-medium">Files</th>
              <th class="px-3 py-1.5 text-right font-medium">Reclaimable</th>
              <th class="px-3 py-1.5 text-right font-medium">Actions</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-900">
            {#each bloatDirs as dir (dir.id)}
              <tr class="hover:bg-zinc-900/80">
                <td class="max-w-[300px] px-3 py-1.5">
                  <button
                    onclick={() => onZoom(dir.id)}
                    class="flex cursor-pointer items-center gap-2 text-left"
                  >
                    <Folder class="h-3.5 w-3.5 shrink-0 text-cyan-400" />
                    <div class="min-w-0">
                      <div class="truncate font-medium text-zinc-100 hover:text-cyan-300">
                        {dir.name}
                      </div>
                      <div class="truncate text-[10px] text-zinc-500" title={dir.path}>
                        {dir.path}
                      </div>
                    </div>
                  </button>
                </td>
                <td class="px-3 py-1.5 text-[11px] text-cyan-300/90">
                  {dir.bloat_reason ?? 'Build/Cache artifact'}
                </td>
                <td class="px-3 py-1.5 text-right tabular-nums text-zinc-400">
                  {formatCount(dir.file_count)}
                </td>
                <td class="px-3 py-1.5 text-right font-semibold tabular-nums text-zinc-100">
                  {formatBytes(nodeSize(dir))}
                </td>
                <td class="px-3 py-1.5 text-right">
                  <div class="inline-flex items-center gap-1">
                    <button
                      title="Reveal in File Manager"
                      onclick={() => onReveal(dir.id)}
                      class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                    >
                      <ExternalLink class="h-3.5 w-3.5" />
                    </button>
                    <button
                      title="Move to Trash"
                      onclick={() => onTrash(dir.id)}
                      class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-rose-500/20 hover:text-rose-300"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {:else if activeTab === 'tiers'}
      <div class="grid grid-cols-1 gap-4 p-3 md:grid-cols-2">
        <div class="rounded-lg border border-zinc-800/80 bg-zinc-900/40 p-3">
          <h3 class="mb-2 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">
            Grouped by File Size Tier
          </h3>
          <div class="space-y-2">
            {#each sizeTierGroups as g (g.key)}
              <div class="space-y-1 text-xs">
                <div class="flex items-center justify-between">
                  <span class="font-medium text-zinc-200">{g.label}</span>
                  <span class="tabular-nums text-zinc-300">
                    {formatBytes(groupSize(g))}
                    <span class="text-zinc-500">({formatCount(g.file_count)} files)</span>
                  </span>
                </div>
                <div class="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
                  <div
                    class="h-full rounded-full bg-cyan-500"
                    style="width: {Math.min(100, g.percent)}%"
                  ></div>
                </div>
              </div>
            {/each}
          </div>
        </div>

        <div class="rounded-lg border border-zinc-800/80 bg-zinc-900/40 p-3">
          <h3 class="mb-2 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">
            Grouped by File Age (Last Modified)
          </h3>
          <div class="space-y-2">
            {#each ageGroups as g (g.key)}
              <div class="space-y-1 text-xs">
                <div class="flex items-center justify-between">
                  <span class="font-medium text-zinc-200">{g.label}</span>
                  <span class="tabular-nums text-zinc-300">
                    {formatBytes(groupSize(g))}
                    <span class="text-zinc-500">({formatCount(g.file_count)} files)</span>
                  </span>
                </div>
                <div class="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
                  <div
                    class="h-full rounded-full bg-amber-500"
                    style="width: {Math.min(100, g.percent)}%"
                  ></div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
