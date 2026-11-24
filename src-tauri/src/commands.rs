use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use fossil_core::arena::{FileArena, NodeId, ScanWarning, SizeMetric};
use fossil_core::classify::FileCategory;
use fossil_core::grouping::{self, GroupBy, GroupEntry, NodeItem, SortField};
use fossil_core::ops;
use fossil_core::scanner::{self, ScanOptions, ScanProgress};
use fossil_core::treemap;
use fossil_core::volumes::{self, QuickTarget, VolumeInfo};
use parking_lot::RwLock;
use serde::Serialize;
use tauri::State;
use tauri::ipc::{Channel, Response};

pub struct AppState {
    pub arena: RwLock<Option<FileArena>>,
    pub cancel_token: RwLock<Option<Arc<AtomicBool>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            arena: RwLock::new(None),
            cancel_token: RwLock::new(None),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanSummary {
    pub root_id: NodeId,
    pub root_path: String,
    pub root_name: String,
    pub total_disk_bytes: u64,
    pub total_logical_bytes: u64,
    pub total_files: u32,
    pub total_nodes: usize,
    pub warnings_count: usize,
    pub scan_duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PagedNodes {
    pub total: usize,
    pub items: Vec<NodeItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BreadcrumbItem {
    pub id: NodeId,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeleteResult {
    pub deleted_count: usize,
    pub reclaimed_disk_bytes: u64,
    pub reclaimed_logical_bytes: u64,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VolumesPayload {
    pub volumes: Vec<VolumeInfo>,
    pub quick_targets: Vec<QuickTarget>,
}

#[tauri::command]
pub fn get_volumes() -> VolumesPayload {
    VolumesPayload {
        volumes: volumes::list_volumes(),
        quick_targets: volumes::list_quick_targets(),
    }
}

#[tauri::command]
pub async fn start_scan(
    path: String,
    same_filesystem: Option<bool>,
    skip_hidden: Option<bool>,
    on_progress: Channel<ScanProgress>,
    state: State<'_, AppState>,
) -> Result<ScanSummary, String> {
    // Cancel any existing scan before starting a new one
    if let Some(prev) = state.cancel_token.write().take() {
        prev.store(true, Ordering::Relaxed);
    }

    let cancel = Arc::new(AtomicBool::new(false));
    *state.cancel_token.write() = Some(Arc::clone(&cancel));

    let options = ScanOptions {
        same_filesystem: same_filesystem.unwrap_or(true),
        skip_hidden: skip_hidden.unwrap_or(false),
        threads: None,
    };

    let target = PathBuf::from(path);
    let arena = tauri::async_runtime::spawn_blocking(move || {
        scanner::scan_directory(target, &options, &cancel, |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|e| format!("Scanner task failed: {e}"))??;

    let summary = ScanSummary {
        root_id: 0,
        root_path: arena.root_path.to_string_lossy().into_owned(),
        root_name: arena.node_name(0).to_owned(),
        total_disk_bytes: arena.nodes[0].disk_size,
        total_logical_bytes: arena.nodes[0].logical_size,
        total_files: arena.nodes[0].file_count,
        total_nodes: arena.nodes.len(),
        warnings_count: arena.warnings.len(),
        scan_duration_ms: arena.scan_duration_ms,
    };

    *state.arena.write() = Some(arena);
    *state.cancel_token.write() = None;

    Ok(summary)
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    if let Some(token) = state.cancel_token.read().as_ref() {
        token.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn get_treemap_layout(
    root_id: NodeId,
    width: f32,
    height: f32,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    max_depth: Option<u8>,
    state: State<'_, AppState>,
) -> Response {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Response::new(Vec::<u8>::new());
    };
    let bytes = treemap::compute_treemap_binary(
        arena,
        root_id,
        width,
        height,
        metric,
        category_filter,
        max_depth.unwrap_or(3),
    );
    Response::new(bytes)
}

#[tauri::command]
pub fn get_node_details(
    node_id: NodeId,
    metric: SizeMetric,
    state: State<'_, AppState>,
) -> Result<NodeItem, String> {
    let guard = state.arena.read();
    let arena = guard.as_ref().ok_or("No active scan")?;
    let node = arena.nodes.get(node_id as usize).ok_or("Invalid node id")?;
    let parent_size = arena
        .nodes
        .get(node.parent as usize)
        .map_or(node.size(metric), |p| p.size(metric));
    Ok(grouping::build_node_item(
        arena,
        node_id,
        parent_size,
        metric,
    ))
}

#[tauri::command]
pub fn get_node_names(node_ids: Vec<NodeId>, state: State<'_, AppState>) -> Vec<String> {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Vec::new();
    };
    node_ids
        .into_iter()
        .map(|id| arena.node_name(id).to_owned())
        .collect()
}

#[tauri::command]
pub fn get_breadcrumbs(node_id: NodeId, state: State<'_, AppState>) -> Vec<BreadcrumbItem> {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Vec::new();
    };
    arena
        .breadcrumbs(node_id)
        .into_iter()
        .map(|(id, name)| BreadcrumbItem { id, name })
        .collect()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn get_children(
    parent_id: NodeId,
    metric: SizeMetric,
    sort_by: SortField,
    descending: bool,
    category_filter: Option<FileCategory>,
    search: Option<String>,
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> PagedNodes {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return PagedNodes {
            total: 0,
            items: Vec::new(),
        };
    };
    let (total, items) = grouping::get_children(
        arena,
        parent_id,
        metric,
        sort_by,
        descending,
        category_filter,
        search.as_deref(),
        offset,
        limit.min(500),
    );
    PagedNodes { total, items }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn get_largest_files(
    root_id: NodeId,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    ext_filter: Option<String>,
    search: Option<String>,
    min_age_days: Option<u64>,
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> PagedNodes {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return PagedNodes {
            total: 0,
            items: Vec::new(),
        };
    };
    let (total, items) = grouping::get_largest_files(
        arena,
        root_id,
        metric,
        category_filter,
        ext_filter.as_deref(),
        search.as_deref(),
        min_age_days,
        offset,
        limit.min(500),
    );
    PagedNodes { total, items }
}

#[tauri::command]
pub fn get_group_summary(
    root_id: NodeId,
    group_by: GroupBy,
    metric: SizeMetric,
    state: State<'_, AppState>,
) -> Vec<GroupEntry> {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Vec::new();
    };
    grouping::get_group_summary(arena, root_id, group_by, metric)
}

#[tauri::command]
pub fn get_bloat_dirs(
    root_id: NodeId,
    metric: SizeMetric,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Vec<NodeItem> {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Vec::new();
    };
    grouping::get_bloat_dirs(arena, root_id, metric, limit.unwrap_or(100))
}

#[tauri::command]
pub fn get_scan_warnings(
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Vec<ScanWarning> {
    let guard = state.arena.read();
    let Some(arena) = guard.as_ref() else {
        return Vec::new();
    };
    arena
        .warnings
        .iter()
        .skip(offset)
        .take(limit.min(500))
        .cloned()
        .collect()
}

#[tauri::command]
pub fn reveal_in_file_manager(node_id: NodeId, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.arena.read();
    let arena = guard.as_ref().ok_or("No active scan")?;
    let path = arena.full_path(node_id);
    ops::reveal_in_file_manager(&path)
}

#[tauri::command]
pub fn trash_nodes(
    node_ids: Vec<NodeId>,
    state: State<'_, AppState>,
) -> Result<DeleteResult, String> {
    let mut guard = state.arena.write();
    let arena = guard.as_mut().ok_or("No active scan")?;

    let mut deleted_count = 0usize;
    let mut reclaimed_disk = 0u64;
    let mut reclaimed_logical = 0u64;
    let mut errors = Vec::new();

    for id in node_ids {
        let path = arena.full_path(id);
        match ops::trash_paths(&[path]) {
            Ok(()) => {
                let (d, l, _) = arena.mark_deleted(id);
                deleted_count += 1;
                reclaimed_disk = reclaimed_disk.saturating_add(d);
                reclaimed_logical = reclaimed_logical.saturating_add(l);
            }
            Err(err) => errors.push(err),
        }
    }

    Ok(DeleteResult {
        deleted_count,
        reclaimed_disk_bytes: reclaimed_disk,
        reclaimed_logical_bytes: reclaimed_logical,
        errors,
    })
}

#[tauri::command]
pub fn delete_nodes_permanently(
    node_ids: Vec<NodeId>,
    state: State<'_, AppState>,
) -> Result<DeleteResult, String> {
    let mut guard = state.arena.write();
    let arena = guard.as_mut().ok_or("No active scan")?;

    let mut deleted_count = 0usize;
    let mut reclaimed_disk = 0u64;
    let mut reclaimed_logical = 0u64;
    let mut errors = Vec::new();

    for id in node_ids {
        let path = arena.full_path(id);
        match ops::delete_permanently(&path) {
            Ok(()) => {
                let (d, l, _) = arena.mark_deleted(id);
                deleted_count += 1;
                reclaimed_disk = reclaimed_disk.saturating_add(d);
                reclaimed_logical = reclaimed_logical.saturating_add(l);
            }
            Err(err) => errors.push(err),
        }
    }

    Ok(DeleteResult {
        deleted_count,
        reclaimed_disk_bytes: reclaimed_disk,
        reclaimed_logical_bytes: reclaimed_logical,
        errors,
    })
}
