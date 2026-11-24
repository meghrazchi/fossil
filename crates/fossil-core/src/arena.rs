use std::path::{Path, PathBuf};

use bitflags::bitflags;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::classify::{FileCategory, bloat_dir_reason};

pub type NodeId = u32;
pub const NONE_NODE: NodeId = u32::MAX;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File = 0,
    Directory = 1,
    Symlink = 2,
    Other = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeMetric {
    Disk,
    Logical,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct NodeFlags: u16 {
        const DELETED         = 1 << 0;
        const HARDLINK_DUP    = 1 << 1;
        const PERMISSION_ERR  = 1 << 2;
        const BLOAT_DIR       = 1 << 3;
        const CLOUD_DATALESS  = 1 << 4;
        const HIDDEN          = 1 << 5;
    }
}

/// Compact node stored contiguously in a flat Vec for high cache locality and low RAM usage.
#[derive(Clone, Debug)]
pub struct FileNode {
    pub parent: NodeId,
    pub name_offset: u32,
    pub name_len: u16,
    pub ext_id: u16,
    pub kind: NodeKind,
    pub category: FileCategory,
    pub flags: NodeFlags,
    pub logical_size: u64,
    pub disk_size: u64,
    pub file_count: u32,
    pub modified_ts: i64,
    /// Indices of direct children (empty for files). Sorted descending by `disk_size` after scan.
    pub children: Vec<NodeId>,
}

impl FileNode {
    #[inline]
    #[must_use]
    pub fn size(&self, metric: SizeMetric) -> u64 {
        match metric {
            SizeMetric::Disk => self.disk_size,
            SizeMetric::Logical => self.logical_size,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.flags.contains(NodeFlags::DELETED)
    }

    #[inline]
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Directory
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanWarning {
    pub path: String,
    pub message: String,
}

/// Flat arena holding an entire scanned directory tree in memory.
pub struct FileArena {
    pub root_path: PathBuf,
    pub nodes: Vec<FileNode>,
    pub names: Vec<u8>,
    pub extensions: Vec<String>,
    ext_intern: FxHashMap<String, u16>,
    pub warnings: Vec<ScanWarning>,
    pub scan_duration_ms: u64,
    pub backend_used: String,
}

impl FileArena {
    #[must_use]
    pub fn new(root_path: PathBuf) -> Self {
        let mut arena = Self {
            root_path: root_path.clone(),
            nodes: Vec::with_capacity(4096),
            names: Vec::with_capacity(65536),
            extensions: vec![String::new()], // 0 = no extension
            ext_intern: FxHashMap::default(),
            warnings: Vec::new(),
            scan_duration_ms: 0,
            backend_used: String::new(),
        };

        let root_name = root_path
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map_or_else(|| root_path.to_string_lossy().into_owned(), str::to_owned);

        arena.push_node(
            NONE_NODE,
            &root_name,
            NodeKind::Directory,
            0,
            0,
            0,
            NodeFlags::empty(),
        );
        arena
    }

    #[inline]
    pub fn intern_name(&mut self, name: &str) -> (u32, u16) {
        let offset = self.names.len() as u32;
        let bytes = name.as_bytes();
        let len = bytes.len().min(u16::MAX as usize);
        self.names.extend_from_slice(&bytes[..len]);
        (offset, len as u16)
    }

    #[inline]
    pub fn intern_ext(&mut self, ext_lower: &str) -> u16 {
        if ext_lower.is_empty() {
            return 0;
        }
        if let Some(&id) = self.ext_intern.get(ext_lower) {
            return id;
        }
        if self.extensions.len() >= u16::MAX as usize {
            return 0;
        }
        let id = self.extensions.len() as u16;
        let owned = ext_lower.to_owned();
        self.extensions.push(owned.clone());
        self.ext_intern.insert(owned, id);
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_node(
        &mut self,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        logical_size: u64,
        disk_size: u64,
        modified_ts: i64,
        mut flags: NodeFlags,
    ) -> NodeId {
        if name.starts_with('.') {
            flags.insert(NodeFlags::HIDDEN);
        }

        let (ext_id, category) = if kind == NodeKind::Directory {
            if bloat_dir_reason(name).is_some() {
                flags.insert(NodeFlags::BLOAT_DIR);
                (0, FileCategory::BuildArtifact)
            } else {
                (0, FileCategory::Other)
            }
        } else {
            let ext_lower = Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            match ext_lower {
                Some(ref ext) if !ext.is_empty() => {
                    let cat = crate::classify::classify_extension(ext);
                    let id = self.intern_ext(ext);
                    (id, cat)
                }
                _ => (0, FileCategory::Other),
            }
        };

        let (name_offset, name_len) = self.intern_name(name);
        let id = self.nodes.len() as NodeId;
        let file_count = u32::from(kind != NodeKind::Directory);

        self.nodes.push(FileNode {
            parent,
            name_offset,
            name_len,
            ext_id,
            kind,
            category,
            flags,
            logical_size,
            disk_size,
            file_count,
            modified_ts,
            children: Vec::new(),
        });

        if parent != NONE_NODE
            && let Some(p) = self.nodes.get_mut(parent as usize)
        {
            p.children.push(id);
        }

        id
    }

    #[inline]
    #[must_use]
    pub fn node_name(&self, id: NodeId) -> &str {
        let Some(node) = self.nodes.get(id as usize) else {
            return "";
        };
        let start = node.name_offset as usize;
        let end = start + node.name_len as usize;
        std::str::from_utf8(&self.names[start..end]).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn node_ext(&self, id: NodeId) -> &str {
        let Some(node) = self.nodes.get(id as usize) else {
            return "";
        };
        self.extensions
            .get(node.ext_id as usize)
            .map_or("", String::as_str)
    }

    /// Reconstructs the exact absolute `PathBuf` on disk by walking parent links to root.
    #[must_use]
    pub fn full_path(&self, id: NodeId) -> PathBuf {
        if id == 0 {
            return self.root_path.clone();
        }
        let mut segments: Vec<&str> = Vec::with_capacity(16);
        let mut curr = id;
        while curr != 0 && curr != NONE_NODE {
            segments.push(self.node_name(curr));
            curr = self.nodes[curr as usize].parent;
        }
        let mut path = self.root_path.clone();
        for seg in segments.into_iter().rev() {
            path.push(seg);
        }
        path
    }

    /// Resolves a path on disk back to its `NodeId` if it lives within this arena.
    #[must_use]
    pub fn find_node_by_path(&self, target: &Path) -> Option<NodeId> {
        let rel = target.strip_prefix(&self.root_path).ok()?;
        let mut curr: NodeId = 0;
        for comp in rel.components() {
            let seg = comp.as_os_str().to_str()?;
            if seg.is_empty() || seg == "." {
                continue;
            }
            let next = self.nodes[curr as usize]
                .children
                .iter()
                .copied()
                .find(|&cid| {
                    !self.nodes[cid as usize].is_deleted() && self.node_name(cid) == seg
                })?;
            curr = next;
        }
        Some(curr)
    }

    /// Bottom-up aggregation of sizes, file counts, dominant directory category, and parallel child sort.
    pub fn finalize(&mut self) {
        // Because parents are always inserted before their children during BFS/top-down scan,
        // reverse index order guarantees children are aggregated before their parents.
        for idx in (0..self.nodes.len()).rev() {
            if !self.nodes[idx].is_dir() {
                continue;
            }
            let children = &self.nodes[idx].children;
            if children.is_empty() {
                continue;
            }
            let mut sum_logical: u64 = 0;
            let mut sum_disk: u64 = 0;
            let mut sum_files: u32 = 0;
            let mut cat_sizes = [0u64; FileCategory::COUNT];

            for &cid in children {
                let child = &self.nodes[cid as usize];
                if child.is_deleted() {
                    continue;
                }
                sum_logical = sum_logical.saturating_add(child.logical_size);
                sum_disk = sum_disk.saturating_add(child.disk_size);
                sum_files = sum_files.saturating_add(child.file_count);
                cat_sizes[child.category as usize] =
                    cat_sizes[child.category as usize].saturating_add(child.disk_size);
            }

            let node = &mut self.nodes[idx];
            node.logical_size = sum_logical;
            node.disk_size = sum_disk;
            node.file_count = sum_files;

            if !node.flags.contains(NodeFlags::BLOAT_DIR) {
                // Assign dominant category to directory for intuitive treemap coloring
                let mut best_cat = FileCategory::Other;
                let mut best_size = 0u64;
                for (c_idx, &sz) in cat_sizes.iter().enumerate() {
                    if sz > best_size {
                        best_size = sz;
                        best_cat = FileCategory::from_u8(c_idx as u8);
                    }
                }
                node.category = best_cat;
            }
        }

        // Parallel sort of each directory's children descending by disk_size.
        let sizes: Vec<u64> = self.nodes.iter().map(|n| n.disk_size).collect();
        self.nodes.par_iter_mut().for_each(|node| {
            if node.children.len() > 1 {
                node.children
                    .sort_unstable_by(|&a, &b| sizes[b as usize].cmp(&sizes[a as usize]));
            }
        });
    }

    /// Incrementally marks a node (and its subtree) as deleted and subtracts its sizes from ancestors.
    /// Returns the `(disk_delta, logical_delta, file_count_delta)` removed.
    pub fn mark_deleted(&mut self, id: NodeId) -> (u64, u64, u32) {
        let Some(node) = self.nodes.get(id as usize) else {
            return (0, 0, 0);
        };
        if node.is_deleted() || id == 0 {
            return (0, 0, 0);
        }

        let disk_delta = node.disk_size;
        let logical_delta = node.logical_size;
        let file_delta = node.file_count;
        let mut parent = node.parent;

        // Mark entire subtree as deleted using an explicit stack (no recursion limit).
        let mut stack = vec![id];
        while let Some(curr) = stack.pop() {
            let n = &mut self.nodes[curr as usize];
            if !n.is_deleted() {
                n.flags.insert(NodeFlags::DELETED);
                stack.extend_from_slice(&n.children);
            }
        }

        // Subtract from all ancestors up to root.
        while parent != NONE_NODE {
            let p = &mut self.nodes[parent as usize];
            p.disk_size = p.disk_size.saturating_sub(disk_delta);
            p.logical_size = p.logical_size.saturating_sub(logical_delta);
            p.file_count = p.file_count.saturating_sub(file_delta);
            parent = p.parent;
        }

        (disk_delta, logical_delta, file_delta)
    }

    /// Returns breadcrumbs from root down to `id`.
    #[must_use]
    pub fn breadcrumbs(&self, id: NodeId) -> Vec<(NodeId, String)> {
        let mut chain = Vec::with_capacity(12);
        let mut curr = id;
        while curr != NONE_NODE {
            chain.push((curr, self.node_name(curr).to_owned()));
            curr = self.nodes[curr as usize].parent;
        }
        chain.reverse();
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_path_aggregation_and_incremental_delete() {
        let mut arena = FileArena::new(PathBuf::from("/tmp/demo"));
        let dir_a = arena.push_node(
            0,
            "videos",
            NodeKind::Directory,
            0,
            0,
            100,
            NodeFlags::empty(),
        );
        let file_1 = arena.push_node(
            dir_a,
            "movie.mp4",
            NodeKind::File,
            1000,
            1024,
            200,
            NodeFlags::empty(),
        );
        let _file_2 = arena.push_node(
            dir_a,
            "clip.mov",
            NodeKind::File,
            500,
            512,
            300,
            NodeFlags::empty(),
        );

        arena.finalize();

        assert_eq!(
            arena.full_path(file_1),
            PathBuf::from("/tmp/demo/videos/movie.mp4")
        );
        assert_eq!(
            arena.find_node_by_path(Path::new("/tmp/demo/videos/movie.mp4")),
            Some(file_1)
        );
        assert_eq!(arena.nodes[dir_a as usize].disk_size, 1536);
        assert_eq!(arena.nodes[0].disk_size, 1536);
        assert_eq!(arena.nodes[0].file_count, 2);
        assert_eq!(arena.nodes[dir_a as usize].category, FileCategory::Video);

        let (d, l, c) = arena.mark_deleted(file_1);
        assert_eq!((d, l, c), (1024, 1000, 1));
        assert_eq!(arena.nodes[dir_a as usize].disk_size, 512);
        assert_eq!(arena.nodes[0].disk_size, 512);
        assert_eq!(arena.nodes[0].file_count, 1);
    }
}
