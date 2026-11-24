export type FileCategory =
  | 'video'
  | 'disk_image'
  | 'archive'
  | 'build_artifact'
  | 'image'
  | 'audio'
  | 'application'
  | 'document'
  | 'system_cache'
  | 'other';

export type NodeKind = 'file' | 'directory' | 'symlink' | 'other';
export type SizeMetric = 'disk' | 'logical';
export type SortField = 'disk_size' | 'logical_size' | 'name' | 'modified' | 'file_count';
export type GroupBy = 'category' | 'extension' | 'size_tier' | 'age';

export interface VolumeInfo {
  name: string;
  mount_point: string;
  fs_type: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  is_removable: boolean;
}

export interface QuickTarget {
  label: string;
  path: string;
  kind: string;
}

export interface VolumesPayload {
  volumes: VolumeInfo[];
  quick_targets: QuickTarget[];
}

export interface ScanProgress {
  files_scanned: number;
  dirs_scanned: number;
  total_disk_bytes: number;
  total_logical_bytes: number;
  warnings_count: number;
  elapsed_ms: number;
  current_path: string;
  done: boolean;
  cancelled: boolean;
  backend_used?: string;
}

export interface ScanSummary {
  root_id: number;
  root_path: string;
  root_name: string;
  total_disk_bytes: number;
  total_logical_bytes: number;
  total_files: number;
  total_nodes: number;
  warnings_count: number;
  scan_duration_ms: number;
}

export interface McpClientStatus {
  key: string;
  name: string;
  is_configured: boolean;
  config_path?: string;
}

export interface NodeItem {
  id: number;
  parent: number;
  name: string;
  path: string;
  ext: string;
  kind: NodeKind;
  category: FileCategory;
  disk_size: number;
  logical_size: number;
  file_count: number;
  modified_ts: number;
  percent_of_parent: number;
  is_bloat: boolean;
  is_hardlink_dup: boolean;
  is_cloud_dataless: boolean;
  has_perm_err: boolean;
  bloat_reason: string | null;
}

export interface PagedNodes {
  total: number;
  items: NodeItem[];
}

export interface BreadcrumbItem {
  id: number;
  name: string;
}

export interface GroupEntry {
  key: string;
  label: string;
  category: FileCategory | null;
  disk_size: number;
  logical_size: number;
  file_count: number;
  percent: number;
}

export interface ScanWarning {
  path: string;
  message: string;
}

export interface DeleteResult {
  deleted_count: number;
  reclaimed_disk_bytes: number;
  reclaimed_logical_bytes: number;
  errors: string[];
}

export interface TreemapRect {
  x: number;
  y: number;
  w: number;
  h: number;
  nodeId: number;
  size: number;
  categoryIndex: number;
  depth: number;
  kindIndex: number;
  flags: number;
}
