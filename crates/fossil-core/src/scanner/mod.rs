pub mod compat;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jwalk::{Parallelism, WalkDir};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::arena::{FileArena, NodeFlags, NodeId, NodeKind, ScanWarning};
use compat::{BackendCapabilities, EntryMeta, default_thread_count, probe_backend};

#[derive(Clone, Debug, Serialize)]
pub struct ScanProgress {
    pub files_scanned: u64,
    pub dirs_scanned: u64,
    pub total_disk_bytes: u64,
    pub total_logical_bytes: u64,
    pub warnings_count: u64,
    pub elapsed_ms: u64,
    pub current_path: String,
    pub done: bool,
    pub cancelled: bool,
    pub backend_used: String,
}

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub same_filesystem: bool,
    pub skip_hidden: bool,
    pub threads: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            same_filesystem: true,
            skip_hidden: false,
            threads: None,
        }
    }
}

/// Selects `RayonNewPool(num_threads)` (which falls back internally to Rayon's default pool if
/// OS thread creation fails) or `RayonDefaultPool` when `num_threads == 0`.
#[inline]
fn select_parallelism(num_threads: usize) -> Parallelism {
    if num_threads == 0 {
        Parallelism::RayonDefaultPool {
            busy_timeout: Duration::from_secs(1),
        }
    } else {
        Parallelism::RayonNewPool(num_threads)
    }
}

/// Returns true if a directory path is a virtual/pseudo filesystem or APFS firmlink that should be skipped
/// when scanning from root to avoid infinite loops or double-counting.
#[inline]
fn should_skip_dir(path: &Path, root_is_slash: bool, extra_skip_mounts: &[String]) -> bool {
    #[cfg(target_os = "linux")]
    {
        if root_is_slash {
            if let Some(s) = path.to_str() {
                if matches!(s, "/proc" | "/sys" | "/dev" | "/run") {
                    return true;
                }
                if extra_skip_mounts.iter().any(|m| m == s) {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        let _ = extra_skip_mounts;
        if root_is_slash && let Some(s) = path.to_str() {
            return matches!(s, "/dev" | "/System/Volumes/Data" | "/Volumes");
        }
        false
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (path, root_is_slash, extra_skip_mounts);
        false
    }
}

#[inline]
fn read_entry_meta(
    path: &Path,
    entry: &jwalk::DirEntry<((), ())>,
    caps: BackendCapabilities,
) -> std::io::Result<EntryMeta> {
    #[cfg(target_os = "linux")]
    {
        if caps.linux_statx_supported && compat::statx_supported() {
            return compat::fetch_linux_meta(path);
        }
        let meta = entry.metadata().map_err(std::io::Error::other)?;
        Ok(compat::extract_unix_meta(&meta, false))
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let _ = path;
        let meta = entry.metadata().map_err(std::io::Error::other)?;
        Ok(compat::extract_unix_meta(
            &meta,
            caps.macos_attrlist_supported,
        ))
    }

    #[cfg(not(unix))]
    {
        let _ = (path, caps);
        let meta = entry.metadata().map_err(std::io::Error::other)?;
        Ok(compat::extract_non_unix_meta(&meta))
    }
}

#[inline]
fn read_root_meta(path: &Path, caps: BackendCapabilities) -> Result<EntryMeta, String> {
    #[cfg(target_os = "linux")]
    {
        if caps.linux_statx_supported && compat::statx_supported() {
            return compat::fetch_linux_meta(path)
                .map_err(|e| format!("Cannot access {}: {e}", path.display()));
        }
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot access {}: {e}", path.display()))?;
        Ok(compat::extract_unix_meta(&meta, false))
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot access {}: {e}", path.display()))?;
        Ok(compat::extract_unix_meta(
            &meta,
            caps.macos_attrlist_supported,
        ))
    }

    #[cfg(not(unix))]
    {
        let _ = caps;
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot access {}: {e}", path.display()))?;
        Ok(compat::extract_non_unix_meta(&meta))
    }
}

/// Scans `root_path` in parallel across CPU cores using `jwalk` + `rayon`,
/// building a cache-friendly `FileArena` while streaming progress updates.
pub fn scan_directory<F>(
    root_path: PathBuf,
    options: &ScanOptions,
    cancel: &Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<FileArena, String>
where
    F: FnMut(ScanProgress),
{
    if !root_path.exists() {
        return Err(format!("Path does not exist: {}", root_path.display()));
    }

    let start = Instant::now();
    let caps = probe_backend(&root_path);
    let root_meta = read_root_meta(&root_path, caps)?;
    let root_dev = root_meta.dev;
    let root_is_slash = root_path == Path::new("/");

    #[cfg(target_os = "linux")]
    let extra_skip_mounts: Arc<Vec<String>> = Arc::new(if root_is_slash {
        compat::read_linux_pseudo_mounts()
    } else {
        Vec::new()
    });
    #[cfg(not(target_os = "linux"))]
    let extra_skip_mounts: Arc<Vec<String>> = Arc::new(Vec::new());

    let num_threads = options
        .threads
        .filter(|&t| t > 0)
        .unwrap_or_else(default_thread_count);

    let mut arena = FileArena::new(root_path.clone());
    arena.backend_used = caps.label.to_owned();

    let mut dir_map: FxHashMap<PathBuf, NodeId> = FxHashMap::default();
    dir_map.insert(root_path.clone(), 0);

    let mut seen_hardlinks: FxHashSet<(u64, u64)> = FxHashSet::default();
    let mut files_scanned: u64 = 0;
    let mut dirs_scanned: u64 = 1;
    let mut total_disk: u64 = 0;
    let mut total_logical: u64 = 0;
    let mut last_emit = Instant::now();

    let same_fs = options.same_filesystem;
    let cancel_clone = Arc::clone(cancel);
    let skip_mounts_clone = Arc::clone(&extra_skip_mounts);

    let walker = WalkDir::new(&root_path)
        .skip_hidden(options.skip_hidden)
        .follow_links(false)
        .sort(false)
        .parallelism(select_parallelism(num_threads))
        .process_read_dir(move |_depth, path, _state, children| {
            if cancel_clone.load(Ordering::Relaxed) {
                children.clear();
                return;
            }
            if should_skip_dir(path, root_is_slash, &skip_mounts_clone) {
                children.clear();
                return;
            }
            // Filter out child directories that should be skipped before descending
            children.retain(|entry_res| match entry_res {
                Ok(entry) => {
                    if entry.file_type.is_dir()
                        && should_skip_dir(&entry.path(), root_is_slash, &skip_mounts_clone)
                    {
                        return false;
                    }
                    true
                }
                Err(_) => true, // Keep errors so main loop records permission warnings
            });
        });

    for entry_res in walker {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let entry = match entry_res {
            Ok(e) => e,
            Err(err) => {
                let err_path = err.path().map_or_else(
                    || root_path.to_string_lossy().into_owned(),
                    |p| p.to_string_lossy().into_owned(),
                );
                arena.warnings.push(ScanWarning {
                    path: err_path,
                    message: err.to_string(),
                });
                continue;
            }
        };

        if entry.depth == 0 {
            continue;
        }

        let path = entry.path();
        let parent_id = path
            .parent()
            .and_then(|p| dir_map.get(p).copied())
            .unwrap_or(0);

        let name = entry.file_name.to_string_lossy();
        let ft = entry.file_type;

        if ft.is_symlink() {
            let modified_ts = entry
                .metadata()
                .ok()
                .map_or(0, |m| compat::metadata_modified_ts(&m));
            arena.push_node(
                parent_id,
                &name,
                NodeKind::Symlink,
                0,
                0,
                modified_ts,
                NodeFlags::empty(),
            );
            files_scanned += 1;
            continue;
        }

        let meta = match read_entry_meta(&path, &entry, caps) {
            Ok(m) => m,
            Err(err) => {
                arena.warnings.push(ScanWarning {
                    path: path.to_string_lossy().into_owned(),
                    message: err.to_string(),
                });
                if let Some(p) = arena.nodes.get_mut(parent_id as usize) {
                    p.flags.insert(NodeFlags::PERMISSION_ERR);
                }
                continue;
            }
        };

        // Stay on same filesystem if requested (prevents crossing into network mounts/pseudo FS)
        #[cfg(unix)]
        if same_fs && root_dev != 0 && meta.dev != root_dev {
            continue;
        }
        #[cfg(not(unix))]
        let _ = (same_fs, root_dev);

        if ft.is_dir() {
            let id = arena.push_node(
                parent_id,
                &name,
                NodeKind::Directory,
                0,
                0,
                meta.modified_ts,
                NodeFlags::empty(),
            );
            dir_map.insert(path, id);
            dirs_scanned += 1;
        } else {
            let mut flags = NodeFlags::empty();
            let mut disk = meta.disk;
            if meta.is_dataless {
                flags.insert(NodeFlags::CLOUD_DATALESS);
                disk = 0;
            } else if meta.nlink > 1
                && meta.ino != 0
                && !seen_hardlinks.insert((meta.dev, meta.ino))
            {
                flags.insert(NodeFlags::HARDLINK_DUP);
                disk = 0;
            }

            let kind = if ft.is_file() {
                NodeKind::File
            } else {
                NodeKind::Other
            };

            arena.push_node(
                parent_id,
                &name,
                kind,
                meta.logical,
                disk,
                meta.modified_ts,
                flags,
            );
            files_scanned += 1;
            total_disk = total_disk.saturating_add(disk);
            total_logical = total_logical.saturating_add(meta.logical);
        }

        // Emit throttled progress updates (~10 times/sec) so IPC never slows down scanning
        if last_emit.elapsed().as_millis() >= 100 {
            on_progress(ScanProgress {
                files_scanned,
                dirs_scanned,
                total_disk_bytes: total_disk,
                total_logical_bytes: total_logical,
                warnings_count: arena.warnings.len() as u64,
                elapsed_ms: start.elapsed().as_millis() as u64,
                current_path: arena.full_path(parent_id).to_string_lossy().into_owned(),
                done: false,
                cancelled: false,
                backend_used: arena.backend_used.clone(),
            });
            last_emit = Instant::now();
        }
    }

    let was_cancelled = cancel.load(Ordering::Relaxed);
    arena.finalize();
    arena.scan_duration_ms = start.elapsed().as_millis() as u64;

    on_progress(ScanProgress {
        files_scanned,
        dirs_scanned,
        total_disk_bytes: arena.nodes[0].disk_size,
        total_logical_bytes: arena.nodes[0].logical_size,
        warnings_count: arena.warnings.len() as u64,
        elapsed_ms: arena.scan_duration_ms,
        current_path: arena.root_path.to_string_lossy().into_owned(),
        done: true,
        cancelled: was_cancelled,
        backend_used: arena.backend_used.clone(),
    });

    Ok(arena)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scans_temp_directory_and_aggregates_sizes() {
        let dir = tempdir().expect("create temp dir");
        let sub = dir.path().join("media");
        fs::create_dir(&sub).expect("create subdir");
        fs::write(sub.join("clip.mp4"), vec![0u8; 4096]).expect("write mp4");
        fs::write(dir.path().join("notes.md"), vec![0u8; 512]).expect("write md");

        let cancel = Arc::new(AtomicBool::new(false));
        let mut got_done = false;
        let mut progress_backend = String::new();
        let arena = scan_directory(
            dir.path().to_path_buf(),
            &ScanOptions::default(),
            &cancel,
            |p| {
                if p.done {
                    got_done = true;
                    progress_backend = p.backend_used;
                }
            },
        )
        .expect("scan succeeds");

        assert!(got_done);
        assert!(!arena.backend_used.is_empty());
        assert_eq!(arena.backend_used, progress_backend);
        assert_eq!(arena.nodes[0].file_count, 2);
        assert_eq!(arena.nodes[0].logical_size, 4608);
    }

    #[test]
    fn scans_with_custom_thread_count_and_zero_fallback() {
        let dir = tempdir().expect("create temp dir");
        fs::write(dir.path().join("a.bin"), vec![1u8; 1024]).expect("write a.bin");
        fs::write(dir.path().join("b.bin"), vec![2u8; 2048]).expect("write b.bin");

        let cancel = Arc::new(AtomicBool::new(false));
        let opts = ScanOptions {
            same_filesystem: true,
            skip_hidden: false,
            threads: Some(2),
        };
        let arena = scan_directory(dir.path().to_path_buf(), &opts, &cancel, |_| {})
            .expect("custom thread scan succeeds");
        assert_eq!(arena.nodes[0].file_count, 2);
        assert_eq!(arena.nodes[0].logical_size, 3072);

        let fallback_opts = ScanOptions {
            same_filesystem: true,
            skip_hidden: false,
            threads: Some(0),
        };
        let arena2 = scan_directory(dir.path().to_path_buf(), &fallback_opts, &cancel, |_| {})
            .expect("zero thread fallback succeeds");
        assert_eq!(arena2.nodes[0].file_count, 2);
    }
}
