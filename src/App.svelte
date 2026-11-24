<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ArrowLeft,
    ArrowUp,
    ChevronRight,
    Home,
    RefreshCw,
    Search,
    Square,
    AlertCircle,
    Bot,
    Sparkles,
    X,
  } from '@lucide/svelte';
  import DrivePicker from './components/DrivePicker.svelte';
  import CategoryBar from './components/CategoryBar.svelte';
  import TreemapCanvas from './components/TreemapCanvas.svelte';
  import FolderTree from './components/FolderTree.svelte';
  import BottomInspector, { type InspectorTab } from './components/BottomInspector.svelte';
  import ContextMenu from './components/ContextMenu.svelte';
  import DeleteModal from './components/DeleteModal.svelte';
  import McpModal from './components/McpModal.svelte';
  import { formatBytes, formatCount, formatDuration } from './lib/format';
  import {
    cancelScan,
    deleteNodesPermanently,
    getBloatDirs,
    getBreadcrumbs,
    getChildren,
    getGroupSummary,
    getLargestFiles,
    getNodeDetails,
    getScanWarnings,
    getTreemapLayout,
    getVolumes,
    revealInFileManager,
    startScan,
    trashNodes,
  } from './lib/ipc';
  import type {
    BreadcrumbItem,
    FileCategory,
    GroupEntry,
    NodeItem,
    QuickTarget,
    ScanProgress,
    ScanSummary,
    ScanWarning,
    SizeMetric,
    TreemapRect,
    VolumeInfo,
  } from './lib/types';

  // Home / Volume state
  let volumes = $state<VolumeInfo[]>([]);
  let quickTargets = $state<QuickTarget[]>([]);

  // Active Scan state
  let isScanning = $state(false);
  let scanProgress = $state<ScanProgress | null>(null);
  let scanSummary = $state<ScanSummary | null>(null);
  let activeScanPath = $state<string | null>(null);
  let isMcpModalOpen = $state(false);

  // Navigation & View state
  let currentRootId = $state(0);
  let historyStack = $state<number[]>([]);
  let breadcrumbs = $state<BreadcrumbItem[]>([]);
  let selectedNodeId = $state<number | null>(null);
  let metric = $state<SizeMetric>('disk');
  let cushionShading = $state(true);
  let activeCategory = $state<FileCategory | null>(null);
  let activeExtFilter = $state<string | null>(null);
  let searchQuery = $state('');
  let activeTab = $state<InspectorTab>('largest');

  // Data slices
  let treemapWidth = $state(0);
  let treemapHeight = $state(0);
  let treemapRects = $state<TreemapRect[]>([]);
  let childrenItems = $state<NodeItem[]>([]);
  let childrenTotal = $state(0);
  let largestFiles = $state<NodeItem[]>([]);
  let largestTotal = $state(0);
  let categoryGroups = $state<GroupEntry[]>([]);
  let extensionGroups = $state<GroupEntry[]>([]);
  let sizeTierGroups = $state<GroupEntry[]>([]);
  let ageGroups = $state<GroupEntry[]>([]);
  let bloatDirs = $state<NodeItem[]>([]);

  // Overlays & Toasts
  let contextMenu = $state<{ node: NodeItem; x: number; y: number } | null>(null);
  let permanentDeleteTarget = $state<NodeItem | null>(null);
  let showWarnings = $state(false);
  let warnings = $state<ScanWarning[]>([]);
  let toastMessage = $state<string | null>(null);
  let searchInputEl: HTMLInputElement | undefined = $state();
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(async () => {
    try {
      const payload = await getVolumes();
      volumes = payload.volumes;
      quickTargets = payload.quick_targets;
    } catch (err) {
      showToast(`Could not load volumes: ${String(err)}`);
    }
  });

  function showToast(msg: string) {
    toastMessage = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastMessage = null;
    }, 4000);
  }

  async function runScan(path: string) {
    activeScanPath = path;
    isScanning = true;
    scanProgress = null;
    activeCategory = null;
    activeExtFilter = null;
    searchQuery = '';
    currentRootId = 0;
    historyStack = [];
    selectedNodeId = null;

    try {
      const summary = await startScan(path, (progress) => {
        scanProgress = progress;
      });
      scanSummary = summary;
      isScanning = false;
      await refreshAllViews();
    } catch (err) {
      isScanning = false;
      showToast(`Scan failed: ${String(err)}`);
    }
  }

  async function handleCancelScan() {
    await cancelScan();
  }

  let treemapSeq = 0;

  async function refreshTreemap() {
    if (!scanSummary || treemapWidth < 10 || treemapHeight < 10) return;
    const seq = ++treemapSeq;
    try {
      const rects = await getTreemapLayout(
        currentRootId,
        treemapWidth,
        treemapHeight,
        metric,
        activeCategory,
        3,
      );
      if (seq === treemapSeq) {
        treemapRects = rects;
      }
    } catch (err) {
      showToast(`Treemap layout error: ${String(err)}`);
    }
  }

  async function refreshAllViews() {
    if (!scanSummary) return;
    const [crumbs, kids, largest, cats, exts, tiers, ages, bloats] = await Promise.all([
      getBreadcrumbs(currentRootId),
      getChildren({
        parentId: currentRootId,
        metric,
        sortBy: metric === 'disk' ? 'disk_size' : 'logical_size',
        descending: true,
        categoryFilter: activeCategory,
        search: searchQuery,
        offset: 0,
        limit: 250,
      }),
      getLargestFiles({
        rootId: currentRootId,
        metric,
        categoryFilter: activeCategory,
        extFilter: activeExtFilter,
        search: searchQuery,
        minAgeDays: null,
        offset: 0,
        limit: 150,
      }),
      getGroupSummary(currentRootId, 'category', metric),
      getGroupSummary(currentRootId, 'extension', metric),
      getGroupSummary(currentRootId, 'size_tier', metric),
      getGroupSummary(currentRootId, 'age', metric),
      getBloatDirs(currentRootId, metric, 100),
    ]);

    breadcrumbs = crumbs;
    childrenItems = kids.items;
    childrenTotal = kids.total;
    largestFiles = largest.items;
    largestTotal = largest.total;
    categoryGroups = cats;
    extensionGroups = exts;
    sizeTierGroups = tiers;
    ageGroups = ages;
    bloatDirs = bloats;

    await refreshTreemap();
  }

  // Re-query when filters, metric, or zoom root change
  $effect(() => {
    if (!scanSummary || isScanning) return;
    void currentRootId;
    void metric;
    void activeCategory;
    void activeExtFilter;
    void searchQuery;
    refreshAllViews();
  });

  function zoomTo(nodeId: number) {
    if (nodeId === currentRootId) return;
    historyStack = [...historyStack, currentRootId];
    currentRootId = nodeId;
    selectedNodeId = null;
  }

  function goBack() {
    if (historyStack.length === 0) return;
    const prev = historyStack[historyStack.length - 1];
    historyStack = historyStack.slice(0, -1);
    currentRootId = prev;
  }

  function goUp() {
    if (breadcrumbs.length <= 1) return;
    const parent = breadcrumbs[breadcrumbs.length - 2];
    zoomTo(parent.id);
  }

  function goHome() {
    scanSummary = null;
    treemapRects = [];
    breadcrumbs = [];
    currentRootId = 0;
    historyStack = [];
    activeCategory = null;
    activeExtFilter = null;
    searchQuery = '';
  }

  async function handleReveal(nodeId: number) {
    try {
      await revealInFileManager(nodeId);
    } catch (err) {
      showToast(String(err));
    }
  }

  async function handleTrash(nodeId: number) {
    try {
      const res = await trashNodes([nodeId]);
      if (res.deleted_count > 0) {
        showToast(`Moved to Trash, reclaimed ${formatBytes(res.reclaimed_disk_bytes)}`);
        await refreshAllViews();
      } else if (res.errors.length > 0) {
        showToast(res.errors[0]);
      }
    } catch (err) {
      showToast(String(err));
    }
  }

  async function handleConfirmPermanentDelete(nodeId: number) {
    permanentDeleteTarget = null;
    try {
      const res = await deleteNodesPermanently([nodeId]);
      if (res.deleted_count > 0) {
        showToast(`Permanently deleted, reclaimed ${formatBytes(res.reclaimed_disk_bytes)}`);
        await refreshAllViews();
      } else if (res.errors.length > 0) {
        showToast(res.errors[0]);
      }
    } catch (err) {
      showToast(String(err));
    }
  }

  async function openContextMenu(nodeId: number, x: number, y: number) {
    try {
      const node = await getNodeDetails(nodeId, metric);
      contextMenu = { node, x, y };
    } catch {
      // Ignore stale node
    }
  }

  async function openWarningsModal() {
    warnings = await getScanWarnings(0, 300);
    showWarnings = true;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!scanSummary || isScanning) return;
    const target = e.target as HTMLElement | null;
    const inInput = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA';

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      searchInputEl?.focus();
      return;
    }
    if (inInput) return;

    if (e.key === 'Backspace' && !e.metaKey) {
      e.preventDefault();
      goUp();
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'r' && selectedNodeId !== null) {
      e.preventDefault();
      handleReveal(selectedNodeId);
    } else if (
      (e.key === 'Delete' || (e.metaKey && e.key === 'Backspace')) &&
      selectedNodeId !== null
    ) {
      e.preventDefault();
      handleTrash(selectedNodeId);
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<div class="flex h-screen w-screen flex-col overflow-hidden bg-zinc-950 text-zinc-100">
  {#if !scanSummary && !isScanning}
    <DrivePicker
      {volumes}
      {quickTargets}
      onScan={runScan}
      onOpenMcp={() => (isMcpModalOpen = true)}
    />
  {:else}
    <!-- Top Navigation & Toolbar -->
    <header
      class="flex items-center justify-between gap-3 border-b border-zinc-800/90 bg-zinc-950 px-3 py-2"
    >
      <div class="flex min-w-0 flex-1 items-center gap-1.5">
        <button
          title="Home / Select Drive"
          onclick={goHome}
          class="cursor-pointer rounded-lg p-1.5 text-zinc-400 hover:bg-zinc-800 hover:text-white"
        >
          <Home class="h-4 w-4" />
        </button>
        <button
          title="Back"
          disabled={historyStack.length === 0}
          onclick={goBack}
          class="cursor-pointer rounded-lg p-1.5 text-zinc-400 hover:bg-zinc-800 hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          <ArrowLeft class="h-4 w-4" />
        </button>
        <button
          title="Up to Parent Folder (Backspace)"
          disabled={breadcrumbs.length <= 1}
          onclick={goUp}
          class="cursor-pointer rounded-lg p-1.5 text-zinc-400 hover:bg-zinc-800 hover:text-white disabled:cursor-not-allowed disabled:opacity-30"
        >
          <ArrowUp class="h-4 w-4" />
        </button>

        <!-- Breadcrumb Bar -->
        <nav
          class="ml-1 flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto rounded-lg border border-zinc-800/80 bg-zinc-900/70 px-2.5 py-1 text-xs"
        >
          {#each breadcrumbs as crumb, idx (crumb.id)}
            {#if idx > 0}
              <ChevronRight class="h-3.5 w-3.5 shrink-0 text-zinc-600" />
            {/if}
            <button
              onclick={() => zoomTo(crumb.id)}
              class="shrink-0 cursor-pointer rounded px-1.5 py-0.5 transition {idx ===
              breadcrumbs.length - 1
                ? 'font-semibold text-cyan-300'
                : 'text-zinc-300 hover:bg-zinc-800 hover:text-white'}"
            >
              {crumb.name}
            </button>
          {/each}
        </nav>
      </div>

      <!-- Search + Metric Toggle + Actions -->
      <div class="flex shrink-0 items-center gap-2">
        <div class="relative">
          <Search
            class="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500"
          />
          <input
            bind:this={searchInputEl}
            bind:value={searchQuery}
            type="text"
            placeholder="Filter files..."
            class="w-44 rounded-lg border border-zinc-800 bg-zinc-900 py-1 pr-7 pl-8 text-xs text-zinc-100 placeholder-zinc-500 focus:border-cyan-500/60 focus:outline-none"
          />
          {#if searchQuery}
            <button
              onclick={() => (searchQuery = '')}
              class="absolute top-1/2 right-2 -translate-y-1/2 cursor-pointer text-zinc-500 hover:text-zinc-200"
            >
              <X class="h-3 w-3" />
            </button>
          {/if}
        </div>

        <!-- Disk Usage vs Logical Size Toggle -->
        <div
          class="inline-flex rounded-lg border border-zinc-800 bg-zinc-900 p-0.5 text-[11px] font-medium"
        >
          <button
            onclick={() => (metric = 'disk')}
            class="cursor-pointer rounded-md px-2 py-0.5 transition {metric === 'disk'
              ? 'bg-zinc-700 text-white'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Disk
          </button>
          <button
            onclick={() => (metric = 'logical')}
            class="cursor-pointer rounded-md px-2 py-0.5 transition {metric === 'logical'
              ? 'bg-zinc-700 text-white'
              : 'text-zinc-400 hover:text-zinc-200'}"
          >
            Logical
          </button>
        </div>

        <button
          title="Toggle 3D Cushion Shading"
          onclick={() => (cushionShading = !cushionShading)}
          class="cursor-pointer rounded-lg border px-2 py-1 text-[11px] font-medium transition {cushionShading
            ? 'border-cyan-500/40 bg-cyan-500/15 text-cyan-300'
            : 'border-zinc-800 bg-zinc-900 text-zinc-400 hover:text-zinc-200'}"
        >
          <Sparkles class="h-3.5 w-3.5" />
        </button>

        <button
          title="Connect AI Assistants (MCP)"
          onclick={() => (isMcpModalOpen = true)}
          class="cursor-pointer inline-flex items-center gap-1.5 rounded-lg border border-purple-500/30 bg-purple-500/10 px-2.5 py-1 text-[11px] font-medium text-purple-300 hover:bg-purple-500/20 transition"
        >
          <Bot class="h-3.5 w-3.5 text-purple-400" />
          <span>AI / MCP</span>
        </button>

        {#if isScanning}
          <button
            onclick={handleCancelScan}
            class="inline-flex cursor-pointer items-center gap-1.5 rounded-lg bg-rose-600 px-3 py-1 text-xs font-medium text-white hover:bg-rose-500"
          >
            <Square class="h-3 w-3 fill-current" />
            Stop
          </button>
        {:else if activeScanPath}
          <button
            title="Rescan"
            onclick={() => activeScanPath && runScan(activeScanPath)}
            class="inline-flex cursor-pointer items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-900 px-2.5 py-1 text-xs font-medium text-zinc-200 hover:bg-zinc-800"
          >
            <RefreshCw class="h-3.5 w-3.5" />
            Rescan
          </button>
        {/if}
      </div>
    </header>

    {#if isScanning}
      <!-- Live Scanning Progress Screen -->
      <div class="flex flex-1 flex-col items-center justify-center gap-5 px-6">
        <div
          class="h-10 w-10 animate-spin rounded-full border-2 border-zinc-800 border-t-cyan-400"
        ></div>
        <div class="text-center">
          <h2 class="text-base font-semibold text-zinc-100">Scanning Filesystem...</h2>
          <p class="mt-1 max-w-lg truncate font-mono text-xs text-zinc-400">
            {scanProgress?.current_path ?? activeScanPath}
          </p>
        </div>

        {#if scanProgress}
          <div class="grid grid-cols-3 gap-4 rounded-xl border border-zinc-800 bg-zinc-900/70 p-4">
            <div class="text-center">
              <div class="text-[11px] text-zinc-500">Files Scanned</div>
              <div class="mt-0.5 text-sm font-semibold tabular-nums text-zinc-100">
                {formatCount(scanProgress.files_scanned)}
              </div>
            </div>
            <div class="text-center">
              <div class="text-[11px] text-zinc-500">Disk Usage</div>
              <div class="mt-0.5 text-sm font-semibold tabular-nums text-cyan-300">
                {formatBytes(scanProgress.total_disk_bytes)}
              </div>
            </div>
            <div class="text-center">
              <div class="text-[11px] text-zinc-500">Elapsed</div>
              <div class="mt-0.5 text-sm font-semibold tabular-nums text-zinc-100">
                {formatDuration(scanProgress.elapsed_ms)}
              </div>
            </div>
          </div>
        {/if}
      </div>
    {:else if scanSummary}
      <!-- Category Breakdown Bar -->
      <CategoryBar
        groups={categoryGroups}
        {metric}
        {activeCategory}
        onSelectCategory={(cat) => (activeCategory = cat)}
      />

      <!-- Main Split Workspace: Top (Folder Tree + Treemap), Bottom (Tabbed Inspector) -->
      <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div class="flex min-h-0 flex-[3] overflow-hidden">
          <div class="h-full w-72 shrink-0 min-h-0 lg:w-80">
            <FolderTree
              items={childrenItems}
              total={childrenTotal}
              {metric}
              {selectedNodeId}
              onSelect={(id) => (selectedNodeId = id)}
              onZoom={zoomTo}
              onReveal={handleReveal}
              onTrash={handleTrash}
              onContextMenu={openContextMenu}
            />
          </div>
          <div class="h-full min-h-0 min-w-0 flex-1">
            <TreemapCanvas
              rects={treemapRects}
              {metric}
              {selectedNodeId}
              {cushionShading}
              onResize={(w, h) => {
                treemapWidth = w;
                treemapHeight = h;
                refreshTreemap();
              }}
              onSelect={(id) => (selectedNodeId = id)}
              onZoom={zoomTo}
              onContextMenu={openContextMenu}
            />
          </div>
        </div>

        <div class="min-h-0 flex-[2] overflow-hidden">
          <BottomInspector
            {activeTab}
            {metric}
            {largestFiles}
            {largestTotal}
            {categoryGroups}
            {extensionGroups}
            {sizeTierGroups}
            {ageGroups}
            {bloatDirs}
            {activeExtFilter}
            onTabChange={(t) => (activeTab = t)}
            onSelectCategory={(cat) => (activeCategory = cat)}
            onSelectExtension={(ext) => (activeExtFilter = ext)}
            onZoom={zoomTo}
            onReveal={handleReveal}
            onTrash={handleTrash}
            onContextMenu={openContextMenu}
          />
        </div>
      </div>

      <!-- Status Footer -->
      <footer
        class="flex items-center justify-between border-t border-zinc-800/90 bg-zinc-950 px-3 py-1 text-[11px] text-zinc-400"
      >
        <div class="flex items-center gap-3 tabular-nums">
          <span>{formatCount(scanSummary.total_files)} files</span>
          <span>&middot;</span>
          <span class="text-zinc-200">{formatBytes(scanSummary.total_disk_bytes)} disk</span>
          <span>({formatBytes(scanSummary.total_logical_bytes)} logical)</span>
          <span>&middot;</span>
          <span>Scanned in {formatDuration(scanSummary.scan_duration_ms)}</span>
        </div>

        <div class="flex items-center gap-3">
          {#if scanSummary.warnings_count > 0}
            <button
              onclick={openWarningsModal}
              class="inline-flex cursor-pointer items-center gap-1 text-amber-400 hover:underline"
            >
              <AlertCircle class="h-3.5 w-3.5" />
              {formatCount(scanSummary.warnings_count)} permission warnings
            </button>
          {/if}
        </div>
      </footer>
    {/if}
  {/if}

  <!-- Context Menu -->
  {#if contextMenu}
    <ContextMenu
      node={contextMenu.node}
      x={contextMenu.x}
      y={contextMenu.y}
      onClose={() => (contextMenu = null)}
      onZoom={zoomTo}
      onReveal={handleReveal}
      onCopyPath={async (path) => {
        await navigator.clipboard.writeText(path);
        showToast('Copied path to clipboard');
      }}
      onTrash={handleTrash}
      onRequestPermanentDelete={(node) => (permanentDeleteTarget = node)}
    />
  {/if}

  <!-- Permanent Delete Confirmation -->
  {#if permanentDeleteTarget}
    <DeleteModal
      node={permanentDeleteTarget}
      onCancel={() => (permanentDeleteTarget = null)}
      onConfirm={handleConfirmPermanentDelete}
    />
  {/if}

  <!-- AI / MCP Modal -->
  <McpModal open={isMcpModalOpen} onclose={() => (isMcpModalOpen = false)} />

  <!-- Permission Warnings Modal -->
  {#if showWarnings}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
    >
      <div
        class="flex max-h-[75vh] w-full max-w-2xl flex-col rounded-2xl border border-zinc-800 bg-zinc-900 shadow-2xl"
      >
        <div class="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
          <h3 class="text-sm font-semibold text-zinc-100">
            Scan Warnings ({warnings.length})
          </h3>
          <button
            onclick={() => (showWarnings = false)}
            class="cursor-pointer rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-white"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto p-3 text-xs">
          <ul class="space-y-1.5">
            {#each warnings as w, idx (idx)}
              <li class="rounded-lg border border-zinc-800/80 bg-zinc-950 p-2">
                <div class="truncate font-mono text-[11px] text-zinc-200">{w.path}</div>
                <div class="mt-0.5 text-[11px] text-amber-400/90">{w.message}</div>
              </li>
            {/each}
          </ul>
        </div>
      </div>
    </div>
  {/if}

  <!-- Toast Notification -->
  {#if toastMessage}
    <div
      class="fixed right-4 bottom-10 z-50 rounded-xl border border-zinc-700 bg-zinc-900/95 px-4 py-2.5 text-xs font-medium text-zinc-100 shadow-2xl backdrop-blur"
    >
      {toastMessage}
    </div>
  {/if}
</div>
