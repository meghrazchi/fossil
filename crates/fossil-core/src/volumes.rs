use serde::Serialize;
use sysinfo::Disks;

#[derive(Clone, Debug, Serialize)]
pub struct VolumeInfo {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub is_removable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct QuickTarget {
    pub label: String,
    pub path: String,
    pub kind: String,
}

#[must_use]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen_mounts = std::collections::HashSet::new();
    let mut result = Vec::new();

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().into_owned();
        if mount.is_empty() || !seen_mounts.insert(mount.clone()) {
            continue;
        }

        // Skip virtual/pseudo mounts on Unix
        #[cfg(target_os = "macos")]
        if mount.starts_with("/System/Volumes/") && mount != "/System/Volumes/Data" {
            continue;
        }
        #[cfg(target_os = "linux")]
        if mount.starts_with("/snap/") || mount.starts_with("/boot") {
            continue;
        }

        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let available = disk.available_space();
        let used = total.saturating_sub(available);

        let raw_name = disk.name().to_string_lossy().into_owned();
        let name = if raw_name.is_empty() {
            if mount == "/" {
                "System Drive".to_owned()
            } else {
                mount.clone()
            }
        } else {
            raw_name
        };

        result.push(VolumeInfo {
            name,
            mount_point: mount,
            fs_type: disk.file_system().to_string_lossy().into_owned(),
            total_bytes: total,
            available_bytes: available,
            used_bytes: used,
            is_removable: disk.is_removable(),
        });
    }

    result.sort_by_key(|b| std::cmp::Reverse(b.total_bytes));
    result
}

#[must_use]
pub fn list_quick_targets() -> Vec<QuickTarget> {
    let mut targets = Vec::new();

    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);

    if let Some(home_dir) = home {
        if home_dir.exists() {
            targets.push(QuickTarget {
                label: "Home".to_owned(),
                path: home_dir.to_string_lossy().into_owned(),
                kind: "home".to_owned(),
            });
        }
        for (label, subdir, kind) in [
            ("Downloads", "Downloads", "downloads"),
            ("Documents", "Documents", "documents"),
            ("Desktop", "Desktop", "desktop"),
        ] {
            let p = home_dir.join(subdir);
            if p.exists() {
                targets.push(QuickTarget {
                    label: label.to_owned(),
                    path: p.to_string_lossy().into_owned(),
                    kind: kind.to_owned(),
                });
            }
        }
        #[cfg(target_os = "macos")]
        {
            let caches = home_dir.join("Library/Caches");
            if caches.exists() {
                targets.push(QuickTarget {
                    label: "User Caches".to_owned(),
                    path: caches.to_string_lossy().into_owned(),
                    kind: "cache".to_owned(),
                });
            }
        }
    }

    targets
}
