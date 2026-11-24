<script lang="ts">
  import { getCategoryByIndex } from '../lib/colors';
  import { formatBytes, formatDate } from '../lib/format';
  import { getNodeDetails, getNodeNames } from '../lib/ipc';
  import type { NodeItem, SizeMetric, TreemapRect } from '../lib/types';

  interface Props {
    rects: TreemapRect[];
    metric: SizeMetric;
    selectedNodeId: number | null;
    cushionShading: boolean;
    onResize: (width: number, height: number) => void;
    onSelect: (nodeId: number) => void;
    onZoom: (nodeId: number) => void;
    onContextMenu: (nodeId: number, x: number, y: number) => void;
  }

  let {
    rects,
    metric,
    selectedNodeId,
    cushionShading,
    onResize,
    onSelect,
    onZoom,
    onContextMenu,
  }: Props = $props();

  let containerEl: HTMLDivElement | undefined = $state();
  let canvasEl: HTMLCanvasElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);

  let hoveredRect: TreemapRect | null = $state(null);
  let hoveredDetails: NodeItem | null = $state(null);
  let pointerX = $state(0);
  let pointerY = $state(0);

  // Cache labels for visible rectangles large enough to draw text
  let labelMap = $state.raw<Record<number, string>>({});
  let hoverFetchSeq = 0;

  $effect(() => {
    if (!containerEl) return;

    const updateSize = (rawW: number, rawH: number) => {
      const w = Math.floor(rawW);
      const h = Math.floor(rawH);
      if (w > 0 && h > 0 && (w !== width || h !== height)) {
        width = w;
        height = h;
        onResize(w, h);
      }
    };

    const initial = containerEl.getBoundingClientRect();
    updateSize(initial.width, initial.height);

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        updateSize(entry.contentRect.width, entry.contentRect.height);
      }
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  // Fetch names in one batch for rects big enough to display labels
  $effect(() => {
    const idsToFetch: number[] = [];
    for (const r of rects) {
      if (
        (r.kindIndex === 1 && r.w > 48 && r.h > 20) ||
        (r.kindIndex !== 1 && r.w > 56 && r.h > 24)
      ) {
        idsToFetch.push(r.nodeId);
      }
    }
    if (idsToFetch.length === 0) {
      labelMap = {};
      return;
    }
    let cancelled = false;
    getNodeNames(idsToFetch).then((names) => {
      if (cancelled) return;
      const next: Record<number, string> = {};
      for (let i = 0; i < idsToFetch.length; i++) {
        next[idsToFetch[i]] = names[i] ?? '';
      }
      labelMap = next;
    });
    return () => {
      cancelled = true;
    };
  });

  // High-FPS Canvas 2D draw loop
  $effect(() => {
    if (!canvasEl || width <= 0 || height <= 0) return;
    const ctx = canvasEl.getContext('2d', { alpha: false });
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvasEl.width = Math.floor(width * dpr);
    canvasEl.height = Math.floor(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    // Background
    ctx.fillStyle = '#09090b';
    ctx.fillRect(0, 0, width, height);

    const hoveredId = hoveredRect?.nodeId ?? -1;

    for (let i = 0; i < rects.length; i++) {
      const r = rects[i];
      const cat = getCategoryByIndex(r.categoryIndex);
      const isDir = r.kindIndex === 1;
      const isHovered = r.nodeId === hoveredId;
      const isSelected = r.nodeId === selectedNodeId;

      if (isDir) {
        // Directory container frame
        ctx.fillStyle = r.depth === 0 ? '#18181b' : '#121215';
        ctx.fillRect(r.x, r.y, r.w, r.h);

        // Subtle tinted header bar
        if (r.h > 18 && r.w > 24) {
          ctx.fillStyle = `${cat.color}28`;
          ctx.fillRect(r.x, r.y, r.w, 17);
        }

        ctx.strokeStyle = isSelected ? '#22d3ee' : isHovered ? '#e4e4e7' : '#27272a';
        ctx.lineWidth = isSelected || isHovered ? 1.5 : 1;
        ctx.strokeRect(r.x + 0.5, r.y + 0.5, Math.max(0, r.w - 1), Math.max(0, r.h - 1));

        // Directory header label
        if (r.w > 48 && r.h > 20) {
          const name = labelMap[r.nodeId];
          if (name) {
            ctx.save();
            ctx.beginPath();
            ctx.rect(r.x + 4, r.y + 1, r.w - 8, 16);
            ctx.clip();
            ctx.fillStyle = '#e4e4e7';
            ctx.font = '600 11px Inter, -apple-system, sans-serif';
            ctx.textBaseline = 'middle';
            const label = r.w > 130 ? `${name}  ·  ${formatBytes(r.size, 1)}` : name;
            ctx.fillText(label, r.x + 5, r.y + 9);
            ctx.restore();
          }
        }
      } else {
        // Leaf file tile
        if (cushionShading && r.w > 6 && r.h > 6) {
          const grad = ctx.createLinearGradient(r.x, r.y, r.x + r.w, r.y + r.h);
          grad.addColorStop(0, cat.color);
          grad.addColorStop(1, cat.darkFill);
          ctx.fillStyle = grad;
        } else {
          ctx.fillStyle = cat.color;
        }
        ctx.fillRect(r.x + 0.5, r.y + 0.5, Math.max(0, r.w - 1), Math.max(0, r.h - 1));

        // Crisp subtle border
        ctx.strokeStyle = isSelected ? '#ffffff' : isHovered ? '#ffffff' : 'rgba(9, 9, 11, 0.55)';
        ctx.lineWidth = isSelected || isHovered ? 1.5 : 0.75;
        ctx.strokeRect(r.x + 0.5, r.y + 0.5, Math.max(0, r.w - 1), Math.max(0, r.h - 1));

        // File label inside large tiles
        if (r.w > 56 && r.h > 24) {
          const name = labelMap[r.nodeId];
          if (name) {
            ctx.save();
            ctx.beginPath();
            ctx.rect(r.x + 4, r.y + 3, r.w - 8, r.h - 6);
            ctx.clip();
            ctx.fillStyle = 'rgba(255, 255, 255, 0.95)';
            ctx.font = '500 11px Inter, -apple-system, sans-serif';
            ctx.textBaseline = 'top';
            ctx.fillText(name, r.x + 5, r.y + 5);
            if (r.h > 38) {
              ctx.fillStyle = 'rgba(255, 255, 255, 0.72)';
              ctx.font = '400 10px ui-monospace, SFMono-Regular, monospace';
              ctx.fillText(formatBytes(r.size, 1), r.x + 5, r.y + 20);
            }
            ctx.restore();
          }
        }
      }
    }
  });

  function hitTest(x: number, y: number): TreemapRect | null {
    // Reverse order so deepest (top-most) rectangle matches first
    for (let i = rects.length - 1; i >= 0; i--) {
      const r = rects[i];
      if (x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) {
        return r;
      }
    }
    return null;
  }

  function handleMouseMove(e: MouseEvent) {
    if (!canvasEl) return;
    const box = canvasEl.getBoundingClientRect();
    const x = e.clientX - box.left;
    const y = e.clientY - box.top;
    pointerX = e.clientX;
    pointerY = e.clientY;

    const hit = hitTest(x, y);
    if (hit?.nodeId !== hoveredRect?.nodeId) {
      hoveredRect = hit;
      if (hit) {
        const seq = ++hoverFetchSeq;
        getNodeDetails(hit.nodeId, metric)
          .then((details) => {
            if (seq === hoverFetchSeq) {
              hoveredDetails = details;
            }
          })
          .catch(() => {
            if (seq === hoverFetchSeq) hoveredDetails = null;
          });
      } else {
        hoveredDetails = null;
      }
    }
  }

  function handleMouseLeave() {
    hoveredRect = null;
    hoveredDetails = null;
  }

  function handleClick() {
    if (hoveredRect) {
      onSelect(hoveredRect.nodeId);
    }
  }

  function handleDblClick() {
    if (hoveredRect && hoveredRect.kindIndex === 1) {
      onZoom(hoveredRect.nodeId);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    if (hoveredRect) {
      onSelect(hoveredRect.nodeId);
      onContextMenu(hoveredRect.nodeId, e.clientX, e.clientY);
    }
  }
</script>

<div bind:this={containerEl} class="relative h-full w-full overflow-hidden bg-zinc-950 select-none">
  <canvas
    bind:this={canvasEl}
    onmousemove={handleMouseMove}
    onmouseleave={handleMouseLeave}
    onclick={handleClick}
    ondblclick={handleDblClick}
    oncontextmenu={handleContextMenu}
    class="block h-full w-full cursor-pointer"
  ></canvas>

  {#if rects.length === 0}
    <div
      class="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-zinc-500"
    >
      No files match the current view or filter.
    </div>
  {/if}

  <!-- Floating Rich Tooltip -->
  {#if hoveredRect && hoveredDetails}
    {@const cat = getCategoryByIndex(hoveredRect.categoryIndex)}
    <div
      class="pointer-events-none fixed z-50 w-72 rounded-xl border border-zinc-700/90 bg-zinc-900/95 p-3 shadow-2xl backdrop-blur-md"
      style="left: {Math.min(pointerX + 16, window.innerWidth - 304)}px; top: {Math.min(
        pointerY + 16,
        window.innerHeight - 165,
      )}px;"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="truncate text-xs font-semibold text-zinc-100">{hoveredDetails.name}</span>
        <span
          class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-medium {cat.badgeClass}"
        >
          {cat.label}
        </span>
      </div>
      <div class="mt-1 truncate text-[11px] text-zinc-400" title={hoveredDetails.path}>
        {hoveredDetails.path}
      </div>

      <div class="mt-2.5 grid grid-cols-2 gap-2 border-t border-zinc-800 pt-2 text-[11px]">
        <div>
          <div class="text-zinc-500">Disk Usage</div>
          <div class="font-semibold text-zinc-100 tabular-nums">
            {formatBytes(hoveredDetails.disk_size)}
          </div>
        </div>
        <div>
          <div class="text-zinc-500">Logical Size</div>
          <div class="font-medium text-zinc-300 tabular-nums">
            {formatBytes(hoveredDetails.logical_size)}
          </div>
        </div>
        <div>
          <div class="text-zinc-500">Modified</div>
          <div class="text-zinc-300 tabular-nums">{formatDate(hoveredDetails.modified_ts)}</div>
        </div>
        <div>
          <div class="text-zinc-500">
            {hoveredDetails.kind === 'directory' ? 'Files Inside' : 'Type'}
          </div>
          <div class="text-zinc-300 tabular-nums">
            {hoveredDetails.kind === 'directory'
              ? hoveredDetails.file_count.toLocaleString()
              : hoveredDetails.ext
                ? `.${hoveredDetails.ext}`
                : hoveredDetails.kind}
          </div>
        </div>
      </div>

      {#if hoveredDetails.bloat_reason}
        <div class="mt-2 rounded bg-cyan-500/10 px-2 py-1 text-[10px] text-cyan-300">
          {hoveredDetails.bloat_reason}
        </div>
      {:else if hoveredDetails.is_cloud_dataless}
        <div class="mt-2 rounded bg-blue-500/10 px-2 py-1 text-[10px] text-blue-300">
          Cloud-only placeholder (0 B on local disk)
        </div>
      {:else if hoveredDetails.is_hardlink_dup}
        <div class="mt-2 rounded bg-amber-500/10 px-2 py-1 text-[10px] text-amber-300">
          Hardlink (disk blocks already counted elsewhere)
        </div>
      {/if}
    </div>
  {/if}
</div>
