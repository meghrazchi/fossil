use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::arena::{FileArena, NONE_NODE, NodeFlags, NodeId, NodeKind, SizeMetric};
use crate::classify::{FileCategory, bloat_dir_reason};

const PARALLEL_SUBTREE_THRESHOLD: usize = 2_048;

#[derive(Clone, Debug, Serialize)]
pub struct NodeItem {
    pub id: NodeId,
    pub parent: NodeId,
    pub name: String,
    pub path: String,
    pub ext: String,
    pub kind: NodeKind,
    pub category: FileCategory,
    pub disk_size: u64,
    pub logical_size: u64,
    pub file_count: u32,
    pub modified_ts: i64,
    pub percent_of_parent: f64,
    pub is_bloat: bool,
    pub is_hardlink_dup: bool,
    pub is_cloud_dataless: bool,
    pub has_perm_err: bool,
    pub bloat_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroupEntry {
    pub key: String,
    pub label: String,
    pub category: Option<FileCategory>,
    pub disk_size: u64,
    pub logical_size: u64,
    pub file_count: u64,
    pub percent: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    DiskSize,
    LogicalSize,
    Name,
    Modified,
    FileCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    Category,
    Extension,
    SizeTier,
    Age,
}

impl GroupBy {
    #[must_use]
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "category" | "cat" => Some(Self::Category),
            "extension" | "ext" => Some(Self::Extension),
            "size_tier" | "size-tier" | "sizetier" | "size" => Some(Self::SizeTier),
            "age" => Some(Self::Age),
            _ => None,
        }
    }
}

#[inline]
fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if haystack.is_ascii() && needle_lower.is_ascii() {
        let h = haystack.as_bytes();
        let n = needle_lower.as_bytes();
        return h.len() >= n.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    haystack.to_lowercase().contains(needle_lower)
}

#[inline]
fn matches_name_or_path(
    arena: &FileArena,
    id: NodeId,
    query_lower: &str,
    query_has_sep: bool,
) -> bool {
    if query_lower.is_empty() {
        return true;
    }
    if !query_has_sep {
        if contains_ignore_case(arena.node_name(id), query_lower) {
            return true;
        }
        // Check ancestor directory names without allocating a PathBuf
        let mut curr = arena.nodes[id as usize].parent;
        while curr != NONE_NODE {
            if contains_ignore_case(arena.node_name(curr), query_lower) {
                return true;
            }
            curr = arena.nodes[curr as usize].parent;
        }
        return contains_ignore_case(&arena.root_path.to_string_lossy(), query_lower);
    }
    let full = arena.full_path(id);
    let full_str = full.to_string_lossy();
    if query_lower.contains('\\') || full_str.contains('\\') {
        let normalized_path = full_str.replace('\\', "/");
        let normalized_query = query_lower.replace('\\', "/");
        contains_ignore_case(&normalized_path, &normalized_query)
    } else {
        contains_ignore_case(&full_str, query_lower)
    }
}

#[inline]
fn collect_subtree_nodes(arena: &FileArena, root_id: NodeId) -> Vec<NodeId> {
    let mut nodes = Vec::with_capacity(256);
    let mut stack = vec![root_id];
    while let Some(curr) = stack.pop() {
        let Some(node) = arena.nodes.get(curr as usize) else {
            continue;
        };
        if node.is_deleted() {
            continue;
        }
        if node.is_dir() {
            stack.extend_from_slice(&node.children);
        } else {
            nodes.push(curr);
        }
    }
    nodes
}

#[inline]
fn has_bloat_ancestor(arena: &FileArena, id: NodeId, stop_at: NodeId) -> bool {
    let mut curr = arena.nodes[id as usize].parent;
    while curr != stop_at && curr != NONE_NODE {
        if arena.nodes[curr as usize]
            .flags
            .contains(NodeFlags::BLOAT_DIR)
        {
            return true;
        }
        curr = arena.nodes[curr as usize].parent;
    }
    false
}

#[must_use]
pub fn build_node_item(
    arena: &FileArena,
    id: NodeId,
    parent_size: u64,
    metric: SizeMetric,
) -> NodeItem {
    let node = &arena.nodes[id as usize];
    let name = arena.node_name(id).to_owned();
    let size = node.size(metric);
    let percent_of_parent = if parent_size > 0 {
        (size as f64 / parent_size as f64) * 100.0
    } else {
        0.0
    };
    let bloat_reason = if node.flags.contains(NodeFlags::BLOAT_DIR) {
        bloat_dir_reason(&name).map(str::to_owned)
    } else {
        None
    };

    NodeItem {
        id,
        parent: node.parent,
        name,
        path: arena.full_path(id).to_string_lossy().into_owned(),
        ext: arena.node_ext(id).to_owned(),
        kind: node.kind,
        category: node.category,
        disk_size: node.disk_size,
        logical_size: node.logical_size,
        file_count: node.file_count,
        modified_ts: node.modified_ts,
        percent_of_parent,
        is_bloat: node.flags.contains(NodeFlags::BLOAT_DIR),
        is_hardlink_dup: node.flags.contains(NodeFlags::HARDLINK_DUP),
        is_cloud_dataless: node.flags.contains(NodeFlags::CLOUD_DATALESS),
        has_perm_err: node.flags.contains(NodeFlags::PERMISSION_ERR),
        bloat_reason,
    }
}

/// Returns paginated/sorted direct children of `parent_id`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn get_children(
    arena: &FileArena,
    parent_id: NodeId,
    metric: SizeMetric,
    sort_by: SortField,
    descending: bool,
    category_filter: Option<FileCategory>,
    search: Option<&str>,
    offset: usize,
    limit: usize,
) -> (usize, Vec<NodeItem>) {
    let Some(parent) = arena.nodes.get(parent_id as usize) else {
        return (0, Vec::new());
    };
    let parent_size = parent.size(metric);
    let search_lower = search
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);

    let mut matching: Vec<NodeId> = parent
        .children
        .iter()
        .copied()
        .filter(|&cid| {
            let n = &arena.nodes[cid as usize];
            if n.is_deleted() {
                return false;
            }
            if let Some(cat) = category_filter
                && n.category != cat
            {
                return false;
            }
            if let Some(ref q) = search_lower
                && !contains_ignore_case(arena.node_name(cid), q)
            {
                return false;
            }
            true
        })
        .collect();

    matching.sort_unstable_by(|&a, &b| {
        let na = &arena.nodes[a as usize];
        let nb = &arena.nodes[b as usize];
        let ord = match sort_by {
            SortField::DiskSize => na.disk_size.cmp(&nb.disk_size),
            SortField::LogicalSize => na.logical_size.cmp(&nb.logical_size),
            SortField::Modified => na.modified_ts.cmp(&nb.modified_ts),
            SortField::FileCount => na.file_count.cmp(&nb.file_count),
            SortField::Name => arena
                .node_name(a)
                .to_ascii_lowercase()
                .cmp(&arena.node_name(b).to_ascii_lowercase()),
        };
        if descending { ord.reverse() } else { ord }
    });

    let total = matching.len();
    let page = matching
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|id| build_node_item(arena, id, parent_size, metric))
        .collect();

    (total, page)
}

/// Multi-core parallel filename/path search across the scanned arena using `rayon`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn search_files(
    arena: &FileArena,
    root_id: NodeId,
    query: &str,
    category_filter: Option<FileCategory>,
    ext_filter: Option<&str>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    metric: SizeMetric,
    limit: usize,
) -> (usize, Vec<NodeItem>) {
    let Some(root) = arena.nodes.get(root_id as usize) else {
        return (0, Vec::new());
    };
    let root_size = root.size(metric);
    let query_lower = query.trim().to_lowercase();
    let query_has_sep = query_lower.contains('/') || query_lower.contains('\\');
    let ext_lower = ext_filter
        .map(|s| s.trim().trim_start_matches('.'))
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase);

    let matches_filter = |id: NodeId| -> bool {
        let node = &arena.nodes[id as usize];
        if node.is_deleted() || node.is_dir() {
            return false;
        }
        if let Some(cat) = category_filter
            && node.category != cat
        {
            return false;
        }
        if let Some(ref ext) = ext_lower
            && arena.node_ext(id) != ext
        {
            return false;
        }
        let sz = node.size(metric);
        if let Some(min_sz) = min_size
            && sz < min_sz
        {
            return false;
        }
        if let Some(max_sz) = max_size
            && sz > max_sz
        {
            return false;
        }
        matches_name_or_path(arena, id, &query_lower, query_has_sep)
    };

    let mut files: Vec<NodeId> = if root_id == 0 {
        (0..arena.nodes.len() as u32)
            .into_par_iter()
            .filter(|&id| matches_filter(id))
            .collect()
    } else {
        let subtree = collect_subtree_nodes(arena, root_id);
        if subtree.len() > PARALLEL_SUBTREE_THRESHOLD {
            subtree
                .into_par_iter()
                .filter(|&id| matches_filter(id))
                .collect()
        } else {
            subtree
                .into_iter()
                .filter(|&id| matches_filter(id))
                .collect()
        }
    };

    let total = files.len();
    if total == 0 || limit == 0 {
        return (total, Vec::new());
    }

    if files.len() > PARALLEL_SUBTREE_THRESHOLD {
        files.par_sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    } else {
        files.sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    }

    let items = files
        .into_iter()
        .take(limit)
        .map(|id| build_node_item(arena, id, root_size, metric))
        .collect();

    (total, items)
}

/// Traverses the subtree under `root_id` in parallel across CPU cores and returns the largest files.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn get_largest_files(
    arena: &FileArena,
    root_id: NodeId,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    ext_filter: Option<&str>,
    search: Option<&str>,
    min_age_days: Option<u64>,
    offset: usize,
    limit: usize,
) -> (usize, Vec<NodeItem>) {
    let Some(root) = arena.nodes.get(root_id as usize) else {
        return (0, Vec::new());
    };
    let root_size = root.size(metric);
    let search_lower = search
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);
    let ext_lower = ext_filter
        .map(|s| s.trim().trim_start_matches('.'))
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase);

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let cutoff_ts = min_age_days.map(|days| now_ts - (days as i64 * 86_400));

    let matches_file = |curr: NodeId| -> bool {
        let node = &arena.nodes[curr as usize];
        if node.is_deleted() || node.is_dir() {
            return false;
        }
        if let Some(cat) = category_filter
            && node.category != cat
        {
            return false;
        }
        if let Some(ref ext) = ext_lower
            && arena.node_ext(curr) != ext
        {
            return false;
        }
        if let Some(cutoff) = cutoff_ts
            && (node.modified_ts == 0 || node.modified_ts > cutoff)
        {
            return false;
        }
        if let Some(ref q) = search_lower
            && !contains_ignore_case(arena.node_name(curr), q)
        {
            return false;
        }
        true
    };

    let mut files: Vec<NodeId> = if root_id == 0 {
        (0..arena.nodes.len() as u32)
            .into_par_iter()
            .filter(|&id| matches_file(id))
            .collect()
    } else {
        let subtree = collect_subtree_nodes(arena, root_id);
        if subtree.len() > PARALLEL_SUBTREE_THRESHOLD {
            subtree
                .into_par_iter()
                .filter(|&id| matches_file(id))
                .collect()
        } else {
            subtree.into_iter().filter(|&id| matches_file(id)).collect()
        }
    };

    let total = files.len();
    if offset >= total {
        return (total, Vec::new());
    }

    if files.len() > PARALLEL_SUBTREE_THRESHOLD {
        files.par_sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    } else {
        files.sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    }

    let page = files
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|id| build_node_item(arena, id, root_size, metric))
        .collect();

    (total, page)
}

/// Returns known regeneratable build/cache bloat directories under `root_id`, sorted descending by size.
#[must_use]
pub fn get_bloat_dirs(
    arena: &FileArena,
    root_id: NodeId,
    metric: SizeMetric,
    limit: usize,
) -> Vec<NodeItem> {
    let Some(root) = arena.nodes.get(root_id as usize) else {
        return Vec::new();
    };
    let root_size = root.size(metric);

    let mut dirs: Vec<NodeId> = if root_id == 0 {
        (1..arena.nodes.len() as u32)
            .into_par_iter()
            .filter(|&curr| {
                let node = &arena.nodes[curr as usize];
                node.is_dir()
                    && !node.is_deleted()
                    && node.flags.contains(NodeFlags::BLOAT_DIR)
                    && node.size(metric) > 0
                    && !has_bloat_ancestor(arena, curr, 0)
            })
            .collect()
    } else {
        let mut collected = Vec::new();
        let mut stack = vec![root_id];
        while let Some(curr) = stack.pop() {
            let Some(node) = arena.nodes.get(curr as usize) else {
                continue;
            };
            if node.is_deleted() || !node.is_dir() {
                continue;
            }
            if curr != root_id && node.flags.contains(NodeFlags::BLOAT_DIR) && node.size(metric) > 0
            {
                collected.push(curr);
                // Do not double-count nested bloat dirs (e.g. node_modules inside node_modules)
                continue;
            }
            stack.extend_from_slice(&node.children);
        }
        collected
    };

    if dirs.len() > PARALLEL_SUBTREE_THRESHOLD {
        dirs.par_sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    } else {
        dirs.sort_unstable_by(|&a, &b| {
            arena.nodes[b as usize]
                .size(metric)
                .cmp(&arena.nodes[a as usize].size(metric))
        });
    }

    dirs.into_iter()
        .take(limit)
        .map(|id| build_node_item(arena, id, root_size, metric))
        .collect()
}

/// Computes grouped storage breakdowns (Category, Extension, SizeTier, Age) in parallel across CPU cores.
#[must_use]
pub fn get_group_summary(
    arena: &FileArena,
    root_id: NodeId,
    group_by: GroupBy,
    metric: SizeMetric,
) -> Vec<GroupEntry> {
    let Some(root) = arena.nodes.get(root_id as usize) else {
        return Vec::new();
    };
    let total_size = root.size(metric).max(1) as f64;
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);

    let subtree_files = if root_id == 0 {
        None
    } else {
        Some(collect_subtree_nodes(arena, root_id))
    };

    match group_by {
        GroupBy::Category => {
            let acc = if let Some(ref files) = subtree_files {
                if files.len() > PARALLEL_SUBTREE_THRESHOLD {
                    files
                        .par_iter()
                        .fold(
                            || [(0u64, 0u64, 0u64); FileCategory::COUNT],
                            |mut acc, &id| {
                                let n = &arena.nodes[id as usize];
                                let slot = &mut acc[n.category as usize];
                                slot.0 = slot.0.saturating_add(n.disk_size);
                                slot.1 = slot.1.saturating_add(n.logical_size);
                                slot.2 += 1;
                                acc
                            },
                        )
                        .reduce(
                            || [(0u64, 0u64, 0u64); FileCategory::COUNT],
                            |mut a, b| {
                                for i in 0..FileCategory::COUNT {
                                    a[i].0 = a[i].0.saturating_add(b[i].0);
                                    a[i].1 = a[i].1.saturating_add(b[i].1);
                                    a[i].2 += b[i].2;
                                }
                                a
                            },
                        )
                } else {
                    let mut acc = [(0u64, 0u64, 0u64); FileCategory::COUNT];
                    for &id in files {
                        let n = &arena.nodes[id as usize];
                        let slot = &mut acc[n.category as usize];
                        slot.0 = slot.0.saturating_add(n.disk_size);
                        slot.1 = slot.1.saturating_add(n.logical_size);
                        slot.2 += 1;
                    }
                    acc
                }
            } else {
                (0..arena.nodes.len() as u32)
                    .into_par_iter()
                    .fold(
                        || [(0u64, 0u64, 0u64); FileCategory::COUNT],
                        |mut acc, id| {
                            let n = &arena.nodes[id as usize];
                            if !n.is_deleted() && !n.is_dir() {
                                let slot = &mut acc[n.category as usize];
                                slot.0 = slot.0.saturating_add(n.disk_size);
                                slot.1 = slot.1.saturating_add(n.logical_size);
                                slot.2 += 1;
                            }
                            acc
                        },
                    )
                    .reduce(
                        || [(0u64, 0u64, 0u64); FileCategory::COUNT],
                        |mut a, b| {
                            for i in 0..FileCategory::COUNT {
                                a[i].0 = a[i].0.saturating_add(b[i].0);
                                a[i].1 = a[i].1.saturating_add(b[i].1);
                                a[i].2 += b[i].2;
                            }
                            a
                        },
                    )
            };

            let mut groups: Vec<GroupEntry> = FileCategory::ALL
                .iter()
                .filter_map(|&cat| {
                    let (d, l, c) = acc[cat as usize];
                    if c == 0 {
                        return None;
                    }
                    let sz = match metric {
                        SizeMetric::Disk => d,
                        SizeMetric::Logical => l,
                    };
                    Some(GroupEntry {
                        key: cat.as_key().to_owned(),
                        label: cat.label().to_owned(),
                        category: Some(cat),
                        disk_size: d,
                        logical_size: l,
                        file_count: c,
                        percent: (sz as f64 / total_size) * 100.0,
                    })
                })
                .collect();
            sort_group_entries(&mut groups, metric);
            groups
        }
        GroupBy::Extension => {
            let map: FxHashMap<u16, (u64, u64, u64, FileCategory)> = if let Some(ref files) =
                subtree_files
            {
                if files.len() > PARALLEL_SUBTREE_THRESHOLD {
                    files
                        .par_iter()
                        .fold(
                            FxHashMap::<u16, (u64, u64, u64, FileCategory)>::default,
                            |mut map, &id| {
                                let n = &arena.nodes[id as usize];
                                let entry = map.entry(n.ext_id).or_insert((0, 0, 0, n.category));
                                entry.0 = entry.0.saturating_add(n.disk_size);
                                entry.1 = entry.1.saturating_add(n.logical_size);
                                entry.2 += 1;
                                map
                            },
                        )
                        .reduce(FxHashMap::default, merge_ext_maps)
                } else {
                    let mut map: FxHashMap<u16, (u64, u64, u64, FileCategory)> =
                        FxHashMap::default();
                    for &id in files {
                        let n = &arena.nodes[id as usize];
                        let entry = map.entry(n.ext_id).or_insert((0, 0, 0, n.category));
                        entry.0 = entry.0.saturating_add(n.disk_size);
                        entry.1 = entry.1.saturating_add(n.logical_size);
                        entry.2 += 1;
                    }
                    map
                }
            } else {
                (0..arena.nodes.len() as u32)
                    .into_par_iter()
                    .fold(
                        FxHashMap::<u16, (u64, u64, u64, FileCategory)>::default,
                        |mut map, id| {
                            let n = &arena.nodes[id as usize];
                            if !n.is_deleted() && !n.is_dir() {
                                let entry = map.entry(n.ext_id).or_insert((0, 0, 0, n.category));
                                entry.0 = entry.0.saturating_add(n.disk_size);
                                entry.1 = entry.1.saturating_add(n.logical_size);
                                entry.2 += 1;
                            }
                            map
                        },
                    )
                    .reduce(FxHashMap::default, merge_ext_maps)
            };

            let mut groups: Vec<GroupEntry> = map
                .into_iter()
                .map(|(ext_id, (d, l, c, cat))| {
                    let ext = arena
                        .extensions
                        .get(ext_id as usize)
                        .map_or("", String::as_str);
                    let label = if ext.is_empty() {
                        "(no extension)".to_owned()
                    } else {
                        format!(".{ext}")
                    };
                    let sz = match metric {
                        SizeMetric::Disk => d,
                        SizeMetric::Logical => l,
                    };
                    GroupEntry {
                        key: ext.to_owned(),
                        label,
                        category: Some(cat),
                        disk_size: d,
                        logical_size: l,
                        file_count: c,
                        percent: (sz as f64 / total_size) * 100.0,
                    }
                })
                .collect();
            sort_group_entries(&mut groups, metric);
            groups.truncate(100);
            groups
        }
        GroupBy::SizeTier => {
            const TIERS: [(&str, &str); 5] = [
                ("giant", ">= 1 GB"),
                ("large", "100 MB – 1 GB"),
                ("medium", "10 MB – 100 MB"),
                ("small", "1 MB – 10 MB"),
                ("tiny", "< 1 MB"),
            ];
            let acc = if let Some(ref files) = subtree_files {
                if files.len() > PARALLEL_SUBTREE_THRESHOLD {
                    files
                        .par_iter()
                        .fold(
                            || [(0u64, 0u64, 0u64); 5],
                            |mut acc, &id| {
                                accum_size_tier(&mut acc, &arena.nodes[id as usize], metric);
                                acc
                            },
                        )
                        .reduce(
                            || [(0u64, 0u64, 0u64); 5],
                            |mut a, b| {
                                for i in 0..5 {
                                    a[i].0 = a[i].0.saturating_add(b[i].0);
                                    a[i].1 = a[i].1.saturating_add(b[i].1);
                                    a[i].2 += b[i].2;
                                }
                                a
                            },
                        )
                } else {
                    let mut acc = [(0u64, 0u64, 0u64); 5];
                    for &id in files {
                        accum_size_tier(&mut acc, &arena.nodes[id as usize], metric);
                    }
                    acc
                }
            } else {
                (0..arena.nodes.len() as u32)
                    .into_par_iter()
                    .fold(
                        || [(0u64, 0u64, 0u64); 5],
                        |mut acc, id| {
                            let n = &arena.nodes[id as usize];
                            if !n.is_deleted() && !n.is_dir() {
                                accum_size_tier(&mut acc, n, metric);
                            }
                            acc
                        },
                    )
                    .reduce(
                        || [(0u64, 0u64, 0u64); 5],
                        |mut a, b| {
                            for i in 0..5 {
                                a[i].0 = a[i].0.saturating_add(b[i].0);
                                a[i].1 = a[i].1.saturating_add(b[i].1);
                                a[i].2 += b[i].2;
                            }
                            a
                        },
                    )
            };

            TIERS
                .iter()
                .enumerate()
                .map(|(i, &(key, label))| {
                    let (d, l, c) = acc[i];
                    let sz = match metric {
                        SizeMetric::Disk => d,
                        SizeMetric::Logical => l,
                    };
                    GroupEntry {
                        key: key.to_owned(),
                        label: label.to_owned(),
                        category: None,
                        disk_size: d,
                        logical_size: l,
                        file_count: c,
                        percent: (sz as f64 / total_size) * 100.0,
                    }
                })
                .collect()
        }
        GroupBy::Age => {
            const BUCKETS: [(&str, &str); 4] = [
                ("ancient", "> 1 year old"),
                ("stale", "6 – 12 months old"),
                ("recent", "1 – 6 months old"),
                ("fresh", "< 30 days old"),
            ];
            let acc = if let Some(ref files) = subtree_files {
                if files.len() > PARALLEL_SUBTREE_THRESHOLD {
                    files
                        .par_iter()
                        .fold(
                            || [(0u64, 0u64, 0u64); 4],
                            |mut acc, &id| {
                                accum_age_bucket(&mut acc, &arena.nodes[id as usize], now_ts);
                                acc
                            },
                        )
                        .reduce(
                            || [(0u64, 0u64, 0u64); 4],
                            |mut a, b| {
                                for i in 0..4 {
                                    a[i].0 = a[i].0.saturating_add(b[i].0);
                                    a[i].1 = a[i].1.saturating_add(b[i].1);
                                    a[i].2 += b[i].2;
                                }
                                a
                            },
                        )
                } else {
                    let mut acc = [(0u64, 0u64, 0u64); 4];
                    for &id in files {
                        accum_age_bucket(&mut acc, &arena.nodes[id as usize], now_ts);
                    }
                    acc
                }
            } else {
                (0..arena.nodes.len() as u32)
                    .into_par_iter()
                    .fold(
                        || [(0u64, 0u64, 0u64); 4],
                        |mut acc, id| {
                            let n = &arena.nodes[id as usize];
                            if !n.is_deleted() && !n.is_dir() {
                                accum_age_bucket(&mut acc, n, now_ts);
                            }
                            acc
                        },
                    )
                    .reduce(
                        || [(0u64, 0u64, 0u64); 4],
                        |mut a, b| {
                            for i in 0..4 {
                                a[i].0 = a[i].0.saturating_add(b[i].0);
                                a[i].1 = a[i].1.saturating_add(b[i].1);
                                a[i].2 += b[i].2;
                            }
                            a
                        },
                    )
            };

            BUCKETS
                .iter()
                .enumerate()
                .map(|(i, &(key, label))| {
                    let (d, l, c) = acc[i];
                    let sz = match metric {
                        SizeMetric::Disk => d,
                        SizeMetric::Logical => l,
                    };
                    GroupEntry {
                        key: key.to_owned(),
                        label: label.to_owned(),
                        category: None,
                        disk_size: d,
                        logical_size: l,
                        file_count: c,
                        percent: (sz as f64 / total_size) * 100.0,
                    }
                })
                .collect()
        }
    }
}

#[inline]
fn merge_ext_maps(
    mut a: FxHashMap<u16, (u64, u64, u64, FileCategory)>,
    b: FxHashMap<u16, (u64, u64, u64, FileCategory)>,
) -> FxHashMap<u16, (u64, u64, u64, FileCategory)> {
    for (k, v) in b {
        let entry = a.entry(k).or_insert((0, 0, 0, v.3));
        entry.0 = entry.0.saturating_add(v.0);
        entry.1 = entry.1.saturating_add(v.1);
        entry.2 += v.2;
    }
    a
}

#[inline]
fn accum_size_tier(acc: &mut [(u64, u64, u64); 5], n: &crate::arena::FileNode, metric: SizeMetric) {
    let sz = n.size(metric);
    let idx = if sz >= 1_000_000_000 {
        0
    } else if sz >= 100_000_000 {
        1
    } else if sz >= 10_000_000 {
        2
    } else if sz >= 1_000_000 {
        3
    } else {
        4
    };
    acc[idx].0 = acc[idx].0.saturating_add(n.disk_size);
    acc[idx].1 = acc[idx].1.saturating_add(n.logical_size);
    acc[idx].2 += 1;
}

#[inline]
fn accum_age_bucket(acc: &mut [(u64, u64, u64); 4], n: &crate::arena::FileNode, now_ts: i64) {
    let age_days = (now_ts - n.modified_ts).max(0) / 86_400;
    let idx = if age_days > 365 {
        0
    } else if age_days > 180 {
        1
    } else if age_days > 30 {
        2
    } else {
        3
    };
    acc[idx].0 = acc[idx].0.saturating_add(n.disk_size);
    acc[idx].1 = acc[idx].1.saturating_add(n.logical_size);
    acc[idx].2 += 1;
}

#[inline]
fn sort_group_entries(groups: &mut [GroupEntry], metric: SizeMetric) {
    groups.sort_by(|a, b| {
        let sa = match metric {
            SizeMetric::Disk => a.disk_size,
            SizeMetric::Logical => a.logical_size,
        };
        let sb = match metric {
            SizeMetric::Disk => b.disk_size,
            SizeMetric::Logical => b.logical_size,
        };
        sb.cmp(&sa)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn build_sample_arena() -> FileArena {
        let mut arena = FileArena::new(PathBuf::from("/tmp/sample"));
        let media = arena.push_node(
            0,
            "media",
            NodeKind::Directory,
            0,
            0,
            100,
            NodeFlags::empty(),
        );
        arena.push_node(
            media,
            "holiday_video.mp4",
            NodeKind::File,
            5_000_000,
            5_120_000,
            100,
            NodeFlags::empty(),
        );
        arena.push_node(
            media,
            "backup.zip",
            NodeKind::File,
            2_000_000,
            2_048_000,
            100,
            NodeFlags::empty(),
        );

        let nm = arena.push_node(
            0,
            "node_modules",
            NodeKind::Directory,
            0,
            0,
            100,
            NodeFlags::empty(),
        );
        let nested_nm = arena.push_node(
            nm,
            "node_modules",
            NodeKind::Directory,
            0,
            0,
            100,
            NodeFlags::empty(),
        );
        arena.push_node(
            nested_nm,
            "bundle.js",
            NodeKind::File,
            500_000,
            512_000,
            100,
            NodeFlags::empty(),
        );
        arena.finalize();
        arena
    }

    #[test]
    fn parallel_search_matches_name_and_path_and_filters() {
        let arena = build_sample_arena();
        let (total, items) = search_files(
            &arena,
            0,
            "holiday",
            None,
            None,
            None,
            None,
            SizeMetric::Disk,
            10,
        );
        assert_eq!(total, 1);
        assert_eq!(items[0].name, "holiday_video.mp4");

        let (total_path, path_items) = search_files(
            &arena,
            0,
            "media/backup",
            None,
            Some("zip"),
            Some(1_000_000),
            None,
            SizeMetric::Disk,
            10,
        );
        assert_eq!(total_path, 1);
        assert_eq!(path_items[0].name, "backup.zip");
    }

    #[test]
    fn parallel_grouping_and_bloat_dedup() {
        let arena = build_sample_arena();

        let groups = get_group_summary(&arena, 0, GroupBy::Category, SizeMetric::Disk);
        assert_eq!(groups[0].key, "video");
        assert_eq!(groups[0].file_count, 1);

        let ext_groups = get_group_summary(&arena, 0, GroupBy::Extension, SizeMetric::Disk);
        assert_eq!(ext_groups.len(), 3);

        // Nested node_modules should not be double-counted when root_id == 0
        let bloat = get_bloat_dirs(&arena, 0, SizeMetric::Disk, 10);
        assert_eq!(bloat.len(), 1);
        assert_eq!(bloat[0].name, "node_modules");
        assert_eq!(bloat[0].parent, 0);
    }

    #[test]
    fn zoomed_subtree_parallel_threshold_works() {
        let mut arena = FileArena::new(PathBuf::from("/tmp/big"));
        let sub = arena.push_node(0, "sub", NodeKind::Directory, 0, 0, 100, NodeFlags::empty());
        for i in 0..2500 {
            let name = format!("file_{i:04}.log");
            arena.push_node(
                sub,
                &name,
                NodeKind::File,
                100 + i as u64,
                100 + i as u64,
                100,
                NodeFlags::empty(),
            );
        }
        arena.finalize();

        let (total, top) = get_largest_files(
            &arena,
            sub,
            SizeMetric::Logical,
            Some(FileCategory::SystemCache),
            Some("log"),
            None,
            None,
            0,
            5,
        );
        assert_eq!(total, 2500);
        assert_eq!(top.len(), 5);
        assert_eq!(top[0].name, "file_2499.log");

        let groups = get_group_summary(&arena, sub, GroupBy::Category, SizeMetric::Logical);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].file_count, 2500);
    }
}
