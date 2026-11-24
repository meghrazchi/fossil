use std::path::Path;
use std::time::UNIX_EPOCH;

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};

/// Default worker thread count using all available CPU cores, falling back safely to 4
/// on older OSes or restricted environments where `available_parallelism` fails.
#[must_use]
pub fn default_thread_count() -> usize {
    std::thread::available_parallelism().map_or(4, usize::from)
}

#[derive(Clone, Copy, Debug)]
pub struct EntryMeta {
    pub logical: u64,
    pub disk: u64,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
    pub modified_ts: i64,
    pub is_dataless: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BackendCapabilities {
    pub label: &'static str,
    pub macos_attrlist_supported: bool,
    pub linux_statx_supported: bool,
}

#[cfg(target_os = "linux")]
static STATX_SUPPORTED: AtomicBool = AtomicBool::new(true);

#[cfg(target_os = "linux")]
#[must_use]
pub fn statx_supported() -> bool {
    STATX_SUPPORTED.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn statx_supported() -> bool {
    false
}

/// Probes OS/filesystem capabilities at scan start so modern kernels use fast paths while
/// older kernels/filesystems fall back transparently with zero wasted syscalls.
#[must_use]
pub fn probe_backend(root_path: &Path) -> BackendCapabilities {
    #[cfg(target_os = "macos")]
    {
        let macos_ok = probe_macos_attrlist(root_path);
        BackendCapabilities {
            label: if macos_ok {
                "macos-attrlist+rayon"
            } else {
                "posix-lstat+rayon"
            },
            macos_attrlist_supported: macos_ok,
            linux_statx_supported: false,
        }
    }

    #[cfg(target_os = "linux")]
    {
        let statx_ok = if STATX_SUPPORTED.load(Ordering::Relaxed) {
            match statx_try_path(root_path) {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => STATX_SUPPORTED.load(Ordering::Relaxed),
            }
        } else {
            false
        };
        BackendCapabilities {
            label: if statx_ok {
                "linux-statx+rayon"
            } else {
                "posix-lstat+rayon"
            },
            macos_attrlist_supported: false,
            linux_statx_supported: statx_ok,
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = root_path;
        BackendCapabilities {
            label: "win32-parallel+rayon",
            macos_attrlist_supported: false,
            linux_statx_supported: false,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = root_path;
        BackendCapabilities {
            label: "posix-lstat+rayon",
            macos_attrlist_supported: false,
            linux_statx_supported: false,
        }
    }
}

#[inline]
pub(crate) fn metadata_modified_ts(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(target_os = "macos")]
const SF_DATALESS: u32 = 0x4000_0000;

/// Checks at runtime whether `getattrlist` / BSD `st_flags` metadata attributes are supported
/// on `path` (APFS/HFS+ return true; older macOS, SMB, exFAT, FAT32, or FUSE return false on
/// `ENOTSUP`/`EINVAL`/`EOPNOTSUPP`).
#[cfg(target_os = "macos")]
#[must_use]
pub fn probe_macos_attrlist(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    #[repr(C)]
    struct FlagsAttrBuf {
        length: u32,
        flags: u32,
    }

    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };

    let mut attr_list = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_FLAGS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buf = FlagsAttrBuf {
        length: 0,
        flags: 0,
    };

    let ret = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&raw mut attr_list).cast::<libc::c_void>(),
            (&raw mut buf).cast::<libc::c_void>(),
            std::mem::size_of::<FlagsAttrBuf>(),
            libc::FSOPT_NOFOLLOW,
        )
    };

    if ret == 0 {
        return true;
    }

    let err = std::io::Error::last_os_error().raw_os_error();
    !matches!(
        err,
        Some(libc::ENOTSUP | libc::EINVAL | libc::EOPNOTSUPP | libc::ENOSYS)
    )
}

/// Detects cloud-only dataless stub files on macOS when `attrlist`/BSD flags are supported;
/// safely returns `false` on legacy filesystems where `st_flags` is unsupported.
#[cfg(target_os = "macos")]
#[inline]
#[must_use]
pub fn detect_macos_dataless(meta: &std::fs::Metadata, supports_macos_flags: bool) -> bool {
    if !supports_macos_flags {
        return false;
    }
    use std::os::macos::fs::MetadataExt as MacMetadataExt;
    (meta.st_flags() & SF_DATALESS) != 0
}

#[cfg(target_os = "linux")]
fn statx_try_path(path: &Path) -> std::io::Result<Option<EntryMeta>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return Ok(None);
    };

    let mut stx: libc::statx = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        libc::syscall(
            libc::SYS_statx,
            libc::AT_FDCWD,
            c_path.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW | libc::AT_STATX_SYNC_AS_STAT,
            libc::STATX_BASIC_STATS,
            &raw mut stx,
        )
    };

    if ret == 0 {
        #[allow(clippy::unnecessary_cast)]
        let dev = libc::makedev(stx.stx_dev_major, stx.stx_dev_minor) as u64;
        return Ok(Some(EntryMeta {
            logical: stx.stx_size,
            disk: stx.stx_blocks.saturating_mul(512),
            dev,
            ino: stx.stx_ino,
            nlink: u64::from(stx.stx_nlink),
            modified_ts: stx.stx_mtime.tv_sec,
            is_dataless: false,
        }));
    }

    let err = std::io::Error::last_os_error();
    if matches!(err.raw_os_error(), Some(libc::ENOSYS | libc::EPERM)) {
        STATX_SUPPORTED.store(false, Ordering::Relaxed);
        return Ok(None);
    }

    Err(err)
}

/// Fetches Linux entry metadata via `statx` when supported, automatically falling back to
/// `std::fs::symlink_metadata` (`lstat`) on older kernels (< 4.11) or restricted seccomp sandboxes.
#[cfg(target_os = "linux")]
pub fn fetch_linux_meta(path: &Path) -> std::io::Result<EntryMeta> {
    if STATX_SUPPORTED.load(Ordering::Relaxed)
        && let Some(meta) = statx_try_path(path)?
    {
        return Ok(meta);
    }
    let meta = std::fs::symlink_metadata(path)?;
    Ok(extract_unix_meta(&meta, false))
}

#[cfg(unix)]
#[inline]
#[must_use]
pub fn extract_unix_meta(meta: &std::fs::Metadata, supports_macos_flags: bool) -> EntryMeta {
    use std::os::unix::fs::MetadataExt;
    #[cfg(target_os = "macos")]
    let is_dataless = detect_macos_dataless(meta, supports_macos_flags);
    #[cfg(not(target_os = "macos"))]
    let is_dataless = {
        let _ = supports_macos_flags;
        false
    };

    EntryMeta {
        logical: meta.len(),
        disk: meta.blocks().saturating_mul(512),
        dev: meta.dev(),
        ino: meta.ino(),
        nlink: meta.nlink(),
        modified_ts: metadata_modified_ts(meta),
        is_dataless,
    }
}

/// Cluster-size fallback for Windows `disk_size` when compressed allocation size is unavailable.
#[inline]
#[must_use]
pub const fn windows_cluster_disk_size(logical: u64, cluster_bytes: u64) -> u64 {
    if logical == 0 {
        return 0;
    }
    let unit = if cluster_bytes == 0 {
        4096
    } else {
        cluster_bytes
    };
    logical.div_ceil(unit).saturating_mul(unit)
}

#[cfg(not(unix))]
#[inline]
#[must_use]
pub fn extract_non_unix_meta(meta: &std::fs::Metadata) -> EntryMeta {
    let logical = meta.len();
    #[cfg(target_os = "windows")]
    let (disk, is_dataless) = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
        const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        let attrs = meta.file_attributes();
        let cloud = (attrs
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
            != 0;
        let d = if cloud {
            0
        } else {
            windows_cluster_disk_size(logical, 4096)
        };
        (d, cloud)
    };
    #[cfg(not(target_os = "windows"))]
    let (disk, is_dataless) = (windows_cluster_disk_size(logical, 4096), false);

    EntryMeta {
        logical,
        disk,
        dev: 0,
        ino: 0,
        nlink: 1,
        modified_ts: metadata_modified_ts(meta),
        is_dataless,
    }
}

/// Parses Linux `/proc/self/mountinfo` if available, or falls back to `/proc/mounts` on older kernels,
/// returning virtual/pseudo filesystem mount points to skip when scanning `/`.
#[must_use]
pub fn parse_pseudo_mounts(mountinfo: Option<&str>, mounts: Option<&str>) -> Vec<String> {
    const PSEUDO_FS: &[&str] = &[
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "cgroup",
        "cgroup2",
        "pstore",
        "securityfs",
        "debugfs",
        "tracefs",
        "configfs",
        "fusectl",
        "bpf",
        "autofs",
        "binfmt_misc",
        "mqueue",
        "hugetlbfs",
    ];

    let mut out = Vec::new();
    if let Some(info) = mountinfo {
        for line in info.lines() {
            let Some((left, right)) = line.split_once(" - ") else {
                continue;
            };
            let mut left_parts = left.split_whitespace();
            let Some(mount_point) = left_parts.nth(4) else {
                continue;
            };
            let Some(fs_type) = right.split_whitespace().next() else {
                continue;
            };
            if PSEUDO_FS.contains(&fs_type) {
                out.push(mount_point.to_owned());
            }
        }
        return out;
    }

    if let Some(legacy) = mounts {
        for line in legacy.lines() {
            let mut parts = line.split_whitespace();
            let _dev = parts.next();
            let Some(mount_point) = parts.next() else {
                continue;
            };
            let Some(fs_type) = parts.next() else {
                continue;
            };
            if PSEUDO_FS.contains(&fs_type) {
                out.push(mount_point.to_owned());
            }
        }
    }

    out
}

/// Reads Linux pseudo-filesystem mount points from `/proc/self/mountinfo` with automatic
/// fallback to `/proc/mounts`.
#[cfg(target_os = "linux")]
#[must_use]
pub fn read_linux_pseudo_mounts() -> Vec<String> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok();
    if mountinfo.is_some() {
        return parse_pseudo_mounts(mountinfo.as_deref(), None);
    }
    let mounts = std::fs::read_to_string("/proc/mounts").ok();
    parse_pseudo_mounts(None, mounts.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thread_count_is_positive() {
        assert!(default_thread_count() >= 1);
    }

    #[test]
    fn linux_mountinfo_falls_back_to_proc_mounts() {
        let mountinfo = "22 1 0:20 / /proc rw,nosuid - proc proc rw\n\
                         23 1 0:21 / /sys rw,nosuid - sysfs sysfs rw\n\
                         24 1 8:1 / / ext4 rw,relatime - ext4 /dev/sda1 rw\n";
        let parsed = parse_pseudo_mounts(Some(mountinfo), None);
        assert_eq!(parsed, vec!["/proc", "/sys"]);

        let legacy_mounts = "proc /proc proc rw 0 0\n\
                             sysfs /sys sysfs rw 0 0\n\
                             /dev/sda1 / ext4 rw 0 0\n";
        let fallback = parse_pseudo_mounts(None, Some(legacy_mounts));
        assert_eq!(fallback, vec!["/proc", "/sys"]);
    }

    #[test]
    fn windows_cluster_fallback_rounds_up_to_cluster_boundary() {
        assert_eq!(windows_cluster_disk_size(0, 4096), 0);
        assert_eq!(windows_cluster_disk_size(1, 4096), 4096);
        assert_eq!(windows_cluster_disk_size(4096, 4096), 4096);
        assert_eq!(windows_cluster_disk_size(4097, 4096), 8192);
    }

    #[test]
    fn probe_backend_returns_known_backend_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let caps = probe_backend(dir.path());
        assert!(!caps.label.is_empty());
        assert!(caps.label.ends_with("+rayon"));
    }
}
