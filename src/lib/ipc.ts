import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  BreadcrumbItem,
  DeleteResult,
  FileCategory,
  GroupBy,
  GroupEntry,
  NodeItem,
  PagedNodes,
  ScanProgress,
  ScanSummary,
  ScanWarning,
  SizeMetric,
  SortField,
  TreemapRect,
  VolumesPayload,
} from './types';

const RECT_STRIDE = 32;

export async function getVolumes(): Promise<VolumesPayload> {
  return invoke<VolumesPayload>('get_volumes');
}

export async function startScan(
  path: string,
  onProgress: (progress: ScanProgress) => void,
  options?: { sameFilesystem?: boolean; skipHidden?: boolean },
): Promise<ScanSummary> {
  const channel = new Channel<ScanProgress>();
  channel.onmessage = onProgress;
  return invoke<ScanSummary>('start_scan', {
    path,
    sameFilesystem: options?.sameFilesystem ?? true,
    skipHidden: options?.skipHidden ?? false,
    onProgress: channel,
  });
}

export async function cancelScan(): Promise<void> {
  return invoke<void>('cancel_scan');
}

/**
 * Decodes a 32-byte-stride binary treemap buffer regardless of whether Tauri v2
 * delivered it as an ArrayBuffer, Uint8Array/ArrayBufferView, or number[].
 */
export function decodeTreemapBinary(raw: unknown): TreemapRect[] {
  let view: DataView | null = null;

  if (raw instanceof ArrayBuffer) {
    if (raw.byteLength >= RECT_STRIDE) {
      view = new DataView(raw);
    }
  } else if (ArrayBuffer.isView(raw)) {
    if (raw.byteLength >= RECT_STRIDE) {
      view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    }
  } else if (Array.isArray(raw)) {
    if (raw.length >= RECT_STRIDE) {
      const bytes = new Uint8Array(raw);
      view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    }
  }

  if (!view) {
    return [];
  }

  const count = Math.floor(view.byteLength / RECT_STRIDE);
  const rects: TreemapRect[] = new Array(count);

  for (let i = 0; i < count; i++) {
    const o = i * RECT_STRIDE;
    const x = view.getFloat32(o, true);
    const y = view.getFloat32(o + 4, true);
    const w = view.getFloat32(o + 8, true);
    const h = view.getFloat32(o + 12, true);
    const nodeId = view.getUint32(o + 16, true);
    const sizeLo = view.getUint32(o + 20, true);
    const sizeHi = view.getUint32(o + 24, true);
    const size = sizeHi * 4294967296 + sizeLo;
    const categoryIndex = view.getUint8(o + 28);
    const depth = view.getUint8(o + 29);
    const kindIndex = view.getUint8(o + 30);
    const flags = view.getUint8(o + 31);

    rects[i] = { x, y, w, h, nodeId, size, categoryIndex, depth, kindIndex, flags };
  }

  return rects;
}

/**
 * Fetches binary squarified treemap layout directly from the backend and decodes it via DataView
 * with zero JSON parsing overhead.
 */
export async function getTreemapLayout(
  rootId: number,
  width: number,
  height: number,
  metric: SizeMetric,
  categoryFilter: FileCategory | null,
  maxDepth = 3,
): Promise<TreemapRect[]> {
  const raw = await invoke<unknown>('get_treemap_layout', {
    rootId,
    width,
    height,
    metric,
    categoryFilter,
    maxDepth,
  });

  return decodeTreemapBinary(raw);
}

export async function getNodeDetails(nodeId: number, metric: SizeMetric): Promise<NodeItem> {
  return invoke<NodeItem>('get_node_details', { nodeId, metric });
}

export async function getNodeNames(nodeIds: number[]): Promise<string[]> {
  if (nodeIds.length === 0) return [];
  return invoke<string[]>('get_node_names', { nodeIds });
}

export async function getBreadcrumbs(nodeId: number): Promise<BreadcrumbItem[]> {
  return invoke<BreadcrumbItem[]>('get_breadcrumbs', { nodeId });
}

export async function getChildren(params: {
  parentId: number;
  metric: SizeMetric;
  sortBy: SortField;
  descending: boolean;
  categoryFilter: FileCategory | null;
  search: string;
  offset: number;
  limit: number;
}): Promise<PagedNodes> {
  return invoke<PagedNodes>('get_children', {
    parentId: params.parentId,
    metric: params.metric,
    sortBy: params.sortBy,
    descending: params.descending,
    categoryFilter: params.categoryFilter,
    search: params.search || null,
    offset: params.offset,
    limit: params.limit,
  });
}

export async function getLargestFiles(params: {
  rootId: number;
  metric: SizeMetric;
  categoryFilter: FileCategory | null;
  extFilter: string | null;
  search: string;
  minAgeDays: number | null;
  offset: number;
  limit: number;
}): Promise<PagedNodes> {
  return invoke<PagedNodes>('get_largest_files', {
    rootId: params.rootId,
    metric: params.metric,
    categoryFilter: params.categoryFilter,
    extFilter: params.extFilter,
    search: params.search || null,
    minAgeDays: params.minAgeDays,
    offset: params.offset,
    limit: params.limit,
  });
}

export async function getGroupSummary(
  rootId: number,
  groupBy: GroupBy,
  metric: SizeMetric,
): Promise<GroupEntry[]> {
  return invoke<GroupEntry[]>('get_group_summary', { rootId, groupBy, metric });
}

export async function getBloatDirs(
  rootId: number,
  metric: SizeMetric,
  limit = 100,
): Promise<NodeItem[]> {
  return invoke<NodeItem[]>('get_bloat_dirs', { rootId, metric, limit });
}

export async function getScanWarnings(offset = 0, limit = 200): Promise<ScanWarning[]> {
  return invoke<ScanWarning[]>('get_scan_warnings', { offset, limit });
}

export async function revealInFileManager(nodeId: number): Promise<void> {
  return invoke<void>('reveal_in_file_manager', { nodeId });
}

export async function trashNodes(nodeIds: number[]): Promise<DeleteResult> {
  return invoke<DeleteResult>('trash_nodes', { nodeIds });
}

export async function deleteNodesPermanently(nodeIds: number[]): Promise<DeleteResult> {
  return invoke<DeleteResult>('delete_nodes_permanently', { nodeIds });
}
