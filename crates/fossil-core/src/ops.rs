use std::path::{Path, PathBuf};
use std::process::Command;

/// Reveals a real file or folder in the native OS file manager, selecting it when supported.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path no longer exists: {}", path.display()));
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let arg = format!("/select,{}", path.display());
        Command::new("explorer")
            .arg(arg)
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {e}"))?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
        Ok(())
    }
}

/// Hard safety guard to prevent accidental deletion of critical root or OS directories.
#[must_use]
pub fn is_protected_path(path: &Path) -> bool {
    if path.parent().is_none() {
        return true; // Root directory (/ or C:\)
    }

    let s = path.to_string_lossy();
    let normalized = s.trim_end_matches(['/', '\\']);

    matches!(
        normalized,
        "" | "/"
            | "/System"
            | "/Library"
            | "/Users"
            | "/Applications"
            | "/usr"
            | "/bin"
            | "/sbin"
            | "/etc"
            | "/var"
            | "/home"
            | "/root"
            | "C:"
            | "C:\\Windows"
            | "C:\\Program Files"
            | "C:\\Program Files (x86)"
            | "C:\\Users"
    )
}

/// Moves real paths to the OS Trash / Recycle Bin.
pub fn trash_paths(paths: &[PathBuf]) -> Result<(), String> {
    for p in paths {
        if is_protected_path(p) {
            return Err(format!(
                "Refusing to delete protected system path: {}",
                p.display()
            ));
        }
    }
    trash::delete_all(paths).map_err(|e| format!("Failed to move to Trash: {e}"))
}

/// Permanently deletes a real file or directory from disk.
pub fn delete_permanently(path: &Path) -> Result<(), String> {
    if is_protected_path(path) {
        return Err(format!(
            "Refusing to delete protected system path: {}",
            path.display()
        ));
    }
    if !path.exists() {
        return Err(format!("Path no longer exists: {}", path.display()));
    }
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| format!("Cannot inspect {}: {e}", path.display()))?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("Failed to delete directory {}: {e}", path.display()))
    } else {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to delete file {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_critical_system_paths() {
        assert!(is_protected_path(Path::new("/")));
        assert!(is_protected_path(Path::new("/System")));
        assert!(is_protected_path(Path::new("/Users")));
        assert!(!is_protected_path(Path::new(
            "/Users/alice/Downloads/big.iso"
        )));
    }
}
