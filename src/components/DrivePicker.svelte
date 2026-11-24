<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FolderOpen,
    HardDrive,
    Home,
    Download,
    FileText,
    Monitor,
    Database,
    ArrowRight,
  } from '@lucide/svelte';
  import type { QuickTarget, VolumeInfo } from '../lib/types';
  import { formatBytes } from '../lib/format';

  interface Props {
    volumes: VolumeInfo[];
    quickTargets: QuickTarget[];
    onScan: (path: string) => void;
  }

  let { volumes, quickTargets, onScan }: Props = $props();

  async function pickFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select a folder to analyze',
    });
    if (typeof selected === 'string' && selected.length > 0) {
      onScan(selected);
    }
  }

  function usedPercent(v: VolumeInfo): number {
    if (v.total_bytes <= 0) return 0;
    return Math.min(100, (v.used_bytes / v.total_bytes) * 100);
  }
</script>

<div class="flex h-full w-full flex-col items-center justify-center overflow-y-auto px-6 py-10">
  <div class="w-full max-w-4xl space-y-8">
    <!-- Hero Header -->
    <div class="flex items-end justify-between border-b border-zinc-800/80 pb-6">
      <div class="space-y-1.5">
        <div class="flex items-center gap-3">
          <img
            src="/logo.png"
            alt="Fossil logo"
            class="h-10 w-10 rounded-xl border border-zinc-700/70 shadow-lg shadow-cyan-500/20"
          />
          <h1 class="text-2xl font-semibold tracking-tight text-zinc-50">Fossil</h1>
          <span
            class="rounded-full border border-cyan-500/30 bg-cyan-500/10 px-2.5 py-0.5 text-[11px] font-medium text-cyan-300"
          >
            Fast &amp; Lightweight
          </span>
        </div>
        <p class="text-sm text-zinc-400">
          Select a volume or directory to map disk usage, group heavy files, and reclaim space
          safely.
        </p>
      </div>

      <button
        onclick={pickFolder}
        class="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-cyan-500 px-4 py-2 text-sm font-medium text-zinc-950 shadow-lg shadow-cyan-500/20 transition hover:bg-cyan-400 active:scale-[0.98]"
      >
        <FolderOpen class="h-4 w-4" />
        Choose Folder...
      </button>
    </div>

    <!-- Mounted Volumes -->
    <section class="space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="text-xs font-semibold tracking-wider text-zinc-400 uppercase">
          Mounted Volumes
        </h2>
        <span class="text-xs text-zinc-500">{volumes.length} detected</span>
      </div>

      <div class="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
        {#each volumes as vol (vol.mount_point)}
          {@const pct = usedPercent(vol)}
          <button
            onclick={() => onScan(vol.mount_point)}
            class="group flex cursor-pointer flex-col gap-3 rounded-xl border border-zinc-800/90 bg-zinc-900/70 p-4 text-left transition hover:border-cyan-500/50 hover:bg-zinc-900 hover:shadow-lg hover:shadow-cyan-950/30"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="flex items-center gap-3 min-w-0">
                <div
                  class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-zinc-700/70 bg-zinc-800/80 text-cyan-400 transition group-hover:border-cyan-500/40 group-hover:bg-cyan-500/10"
                >
                  <HardDrive class="h-5 w-5" />
                </div>
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="truncate text-sm font-semibold text-zinc-100">{vol.name}</span>
                    <span
                      class="rounded bg-zinc-800 px-1.5 py-0.5 text-[10px] font-medium text-zinc-400 uppercase"
                    >
                      {vol.fs_type || 'FS'}
                    </span>
                  </div>
                  <div class="truncate text-xs text-zinc-500">{vol.mount_point}</div>
                </div>
              </div>

              <ArrowRight
                class="h-4 w-4 shrink-0 text-zinc-600 transition group-hover:translate-x-0.5 group-hover:text-cyan-400"
              />
            </div>

            <!-- Capacity Bar -->
            <div class="space-y-1.5">
              <div class="h-2 w-full overflow-hidden rounded-full bg-zinc-800">
                <div
                  class="h-full rounded-full transition-all {pct > 88
                    ? 'bg-rose-500'
                    : pct > 70
                      ? 'bg-amber-500'
                      : 'bg-cyan-500'}"
                  style="width: {pct.toFixed(1)}%"
                ></div>
              </div>
              <div class="flex items-center justify-between text-xs tabular-nums">
                <span class="text-zinc-300">
                  {formatBytes(vol.used_bytes)}
                  <span class="text-zinc-500">used</span>
                </span>
                <span class="text-zinc-400">
                  {formatBytes(vol.available_bytes)} free of {formatBytes(vol.total_bytes)}
                </span>
              </div>
            </div>
          </button>
        {/each}
      </div>
    </section>

    <!-- Quick Folders -->
    {#if quickTargets.length > 0}
      <section class="space-y-3">
        <h2 class="text-xs font-semibold tracking-wider text-zinc-400 uppercase">Quick Folders</h2>
        <div class="flex flex-wrap gap-2.5">
          {#each quickTargets as qt (qt.path)}
            <button
              onclick={() => onScan(qt.path)}
              class="inline-flex cursor-pointer items-center gap-2 rounded-lg border border-zinc-800 bg-zinc-900/70 px-3.5 py-2 text-xs font-medium text-zinc-200 transition hover:border-cyan-500/40 hover:bg-zinc-800/90 hover:text-white"
            >
              {#if qt.kind === 'home'}
                <Home class="h-3.5 w-3.5 text-cyan-400" />
              {:else if qt.kind === 'downloads'}
                <Download class="h-3.5 w-3.5 text-amber-400" />
              {:else if qt.kind === 'documents'}
                <FileText class="h-3.5 w-3.5 text-blue-400" />
              {:else if qt.kind === 'desktop'}
                <Monitor class="h-3.5 w-3.5 text-emerald-400" />
              {:else}
                <Database class="h-3.5 w-3.5 text-purple-400" />
              {/if}
              <span>{qt.label}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  </div>
</div>
