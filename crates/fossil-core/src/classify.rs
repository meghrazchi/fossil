use phf::phf_map;
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Video = 0,
    DiskImage = 1,
    Archive = 2,
    BuildArtifact = 3,
    Image = 4,
    Audio = 5,
    Application = 6,
    Document = 7,
    SystemCache = 8,
    Other = 9,
}

impl FileCategory {
    pub const COUNT: usize = 10;

    pub const ALL: [FileCategory; Self::COUNT] = [
        Self::Video,
        Self::DiskImage,
        Self::Archive,
        Self::BuildArtifact,
        Self::Image,
        Self::Audio,
        Self::Application,
        Self::Document,
        Self::SystemCache,
        Self::Other,
    ];

    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Video,
            1 => Self::DiskImage,
            2 => Self::Archive,
            3 => Self::BuildArtifact,
            4 => Self::Image,
            5 => Self::Audio,
            6 => Self::Application,
            7 => Self::Document,
            8 => Self::SystemCache,
            _ => Self::Other,
        }
    }

    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::DiskImage => "disk_image",
            Self::Archive => "archive",
            Self::BuildArtifact => "build_artifact",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Application => "application",
            Self::Document => "document",
            Self::SystemCache => "system_cache",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "video" | "videos" => Some(Self::Video),
            "disk_image" | "disk-image" | "diskimage" => Some(Self::DiskImage),
            "archive" | "archives" => Some(Self::Archive),
            "build_artifact" | "build-artifact" | "buildartifact" | "build" | "code" => {
                Some(Self::BuildArtifact)
            }
            "image" | "images" | "photo" | "photos" => Some(Self::Image),
            "audio" | "music" => Some(Self::Audio),
            "application" | "applications" | "app" | "apps" => Some(Self::Application),
            "document" | "documents" | "doc" | "docs" => Some(Self::Document),
            "system_cache" | "system-cache" | "cache" | "logs" => Some(Self::SystemCache),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Video => "Videos",
            Self::DiskImage => "Disk Images & VMs",
            Self::Archive => "Archives",
            Self::BuildArtifact => "Code & Build Artifacts",
            Self::Image => "Images & Photos",
            Self::Audio => "Audio",
            Self::Application => "Apps & Binaries",
            Self::Document => "Documents",
            Self::SystemCache => "Logs & Caches",
            Self::Other => "Other",
        }
    }
}

static EXT_MAP: phf::Map<&'static str, FileCategory> = phf_map! {
    // Video
    "mp4" => FileCategory::Video,
    "mkv" => FileCategory::Video,
    "mov" => FileCategory::Video,
    "avi" => FileCategory::Video,
    "webm" => FileCategory::Video,
    "m4v" => FileCategory::Video,
    "wmv" => FileCategory::Video,
    "flv" => FileCategory::Video,
    "mts" => FileCategory::Video,
    "m2ts" => FileCategory::Video,
    "mpeg" => FileCategory::Video,
    "mpg" => FileCategory::Video,
    "prores" => FileCategory::Video,
    "ogv" => FileCategory::Video,

    // Disk Images & VMs
    "iso" => FileCategory::DiskImage,
    "dmg" => FileCategory::DiskImage,
    "img" => FileCategory::DiskImage,
    "vmdk" => FileCategory::DiskImage,
    "vdi" => FileCategory::DiskImage,
    "qcow2" => FileCategory::DiskImage,
    "vhd" => FileCategory::DiskImage,
    "vhdx" => FileCategory::DiskImage,
    "ova" => FileCategory::DiskImage,
    "ovf" => FileCategory::DiskImage,
    "sparseimage" => FileCategory::DiskImage,
    "sparsebundle" => FileCategory::DiskImage,
    "raw" => FileCategory::DiskImage,
    "docker" => FileCategory::DiskImage,

    // Archives
    "zip" => FileCategory::Archive,
    "tar" => FileCategory::Archive,
    "gz" => FileCategory::Archive,
    "tgz" => FileCategory::Archive,
    "bz2" => FileCategory::Archive,
    "xz" => FileCategory::Archive,
    "zst" => FileCategory::Archive,
    "7z" => FileCategory::Archive,
    "rar" => FileCategory::Archive,
    "cab" => FileCategory::Archive,
    "deb" => FileCategory::Archive,
    "rpm" => FileCategory::Archive,
    "pkg" => FileCategory::Archive,
    "whl" => FileCategory::Archive,
    "jar" => FileCategory::Archive,
    "war" => FileCategory::Archive,
    "lz4" => FileCategory::Archive,

    // Code & Build Artifacts
    "rs" => FileCategory::BuildArtifact,
    "ts" => FileCategory::BuildArtifact,
    "tsx" => FileCategory::BuildArtifact,
    "js" => FileCategory::BuildArtifact,
    "jsx" => FileCategory::BuildArtifact,
    "svelte" => FileCategory::BuildArtifact,
    "py" => FileCategory::BuildArtifact,
    "go" => FileCategory::BuildArtifact,
    "c" => FileCategory::BuildArtifact,
    "cpp" => FileCategory::BuildArtifact,
    "h" => FileCategory::BuildArtifact,
    "hpp" => FileCategory::BuildArtifact,
    "java" => FileCategory::BuildArtifact,
    "kt" => FileCategory::BuildArtifact,
    "swift" => FileCategory::BuildArtifact,
    "o" => FileCategory::BuildArtifact,
    "a" => FileCategory::BuildArtifact,
    "rlib" => FileCategory::BuildArtifact,
    "rmeta" => FileCategory::BuildArtifact,
    "pyc" => FileCategory::BuildArtifact,
    "class" => FileCategory::BuildArtifact,
    "map" => FileCategory::BuildArtifact,
    "lock" => FileCategory::BuildArtifact,
    "wasm" => FileCategory::BuildArtifact,

    // Images
    "jpg" => FileCategory::Image,
    "jpeg" => FileCategory::Image,
    "png" => FileCategory::Image,
    "webp" => FileCategory::Image,
    "heic" => FileCategory::Image,
    "heif" => FileCategory::Image,
    "gif" => FileCategory::Image,
    "tiff" => FileCategory::Image,
    "tif" => FileCategory::Image,
    "bmp" => FileCategory::Image,
    "svg" => FileCategory::Image,
    "ico" => FileCategory::Image,
    "icns" => FileCategory::Image,
    "psd" => FileCategory::Image,
    "ai" => FileCategory::Image,
    "cr2" => FileCategory::Image,
    "cr3" => FileCategory::Image,
    "nef" => FileCategory::Image,
    "arw" => FileCategory::Image,
    "dng" => FileCategory::Image,
    "avif" => FileCategory::Image,

    // Audio
    "mp3" => FileCategory::Audio,
    "flac" => FileCategory::Audio,
    "wav" => FileCategory::Audio,
    "aac" => FileCategory::Audio,
    "m4a" => FileCategory::Audio,
    "ogg" => FileCategory::Audio,
    "opus" => FileCategory::Audio,
    "aiff" => FileCategory::Audio,
    "aif" => FileCategory::Audio,
    "alac" => FileCategory::Audio,
    "wma" => FileCategory::Audio,
    "mid" => FileCategory::Audio,
    "midi" => FileCategory::Audio,

    // Applications & Binaries
    "app" => FileCategory::Application,
    "exe" => FileCategory::Application,
    "dll" => FileCategory::Application,
    "dylib" => FileCategory::Application,
    "so" => FileCategory::Application,
    "msi" => FileCategory::Application,
    "appimage" => FileCategory::Application,
    "apk" => FileCategory::Application,
    "ipa" => FileCategory::Application,
    "bin" => FileCategory::Application,
    "elf" => FileCategory::Application,

    // Documents
    "pdf" => FileCategory::Document,
    "doc" => FileCategory::Document,
    "docx" => FileCategory::Document,
    "xls" => FileCategory::Document,
    "xlsx" => FileCategory::Document,
    "ppt" => FileCategory::Document,
    "pptx" => FileCategory::Document,
    "txt" => FileCategory::Document,
    "md" => FileCategory::Document,
    "rtf" => FileCategory::Document,
    "csv" => FileCategory::Document,
    "tsv" => FileCategory::Document,
    "json" => FileCategory::Document,
    "yaml" => FileCategory::Document,
    "yml" => FileCategory::Document,
    "toml" => FileCategory::Document,
    "xml" => FileCategory::Document,
    "epub" => FileCategory::Document,
    "key" => FileCategory::Document,
    "pages" => FileCategory::Document,
    "numbers" => FileCategory::Document,
    "parquet" => FileCategory::Document,
    "sqlite" => FileCategory::Document,
    "db" => FileCategory::Document,

    // System, Logs & Caches
    "log" => FileCategory::SystemCache,
    "tmp" => FileCategory::SystemCache,
    "temp" => FileCategory::SystemCache,
    "cache" => FileCategory::SystemCache,
    "bak" => FileCategory::SystemCache,
    "dmp" => FileCategory::SystemCache,
    "crash" => FileCategory::SystemCache,
    "swp" => FileCategory::SystemCache,
    "wal" => FileCategory::SystemCache,
    "shm" => FileCategory::SystemCache,
};

/// Classify a lowercase file extension in O(1) using a compile-time perfect hash map.
#[inline]
#[must_use]
pub fn classify_extension(ext_lower: &str) -> FileCategory {
    EXT_MAP
        .get(ext_lower)
        .copied()
        .unwrap_or(FileCategory::Other)
}

/// Returns a short description if a directory name is a known regeneratable build/cache artifact.
#[inline]
#[must_use]
pub fn bloat_dir_reason(name: &str) -> Option<&'static str> {
    match name {
        "node_modules" => Some("Node.js dependencies (regeneratable via pnpm/npm install)"),
        "target" => Some("Rust/Cargo or Maven build output (regeneratable via cargo build)"),
        ".venv" | "venv" => Some("Python virtual environment"),
        "__pycache__" => Some("Python bytecode cache"),
        "DerivedData" => Some("Xcode build & index cache"),
        ".gradle" => Some("Gradle build cache"),
        "Pods" => Some("CocoaPods iOS/macOS dependencies"),
        ".next" | ".nuxt" | ".svelte-kit" => Some("Frontend framework build cache"),
        "dist" | "build" => Some("Compiled build output"),
        "Caches" | ".cache" => Some("Application cache directory"),
        ".cargo" => Some("Cargo registry & crate cache"),
        "vendor" => Some("Vendored project dependencies"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_extensions() {
        assert_eq!(classify_extension("mp4"), FileCategory::Video);
        assert_eq!(classify_extension("dmg"), FileCategory::DiskImage);
        assert_eq!(classify_extension("zip"), FileCategory::Archive);
        assert_eq!(classify_extension("rs"), FileCategory::BuildArtifact);
        assert_eq!(classify_extension("unknown_ext_xyz"), FileCategory::Other);
    }

    #[test]
    fn detects_bloat_directories() {
        assert!(bloat_dir_reason("node_modules").is_some());
        assert!(bloat_dir_reason("target").is_some());
        assert!(bloat_dir_reason("Documents").is_none());
    }
}
