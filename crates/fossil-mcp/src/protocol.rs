use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    #[must_use]
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Negotiates protocol version against supported versions.
#[must_use]
pub fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    if let Some(req) = requested {
        for &v in PROTOCOL_VERSIONS {
            if req == v {
                return v;
            }
        }
    }
    DEFAULT_PROTOCOL_VERSION
}

/// Returns definitions for all supported MCP tools including tool annotations.
#[must_use]
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "scan_directory",
            "description": "Scan a directory in parallel across all CPU cores, caching the index in memory for instant follow-up queries and returning storage totals plus top entries.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to scan (supports ~/ and environment variables)" },
                    "same_filesystem": { "type": "boolean", "description": "Stay on the same filesystem/mount (default true)" },
                    "skip_hidden": { "type": "boolean", "description": "Skip hidden dotfiles/directories (default false)" },
                    "threads": { "type": "integer", "minimum": 1, "description": "Optional worker thread count (defaults to all CPU cores)" },
                    "metric": { "type": "string", "enum": ["disk", "logical"], "description": "Size metric (default disk)" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "list_children",
            "description": "List direct children of a directory from the cached scan arena with pagination and sorting, without rescanning disk.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list (defaults to cached root if omitted)" },
                    "sort_by": { "type": "string", "enum": ["size", "name", "files"], "description": "Sort order (default size)" },
                    "order": { "type": "string", "enum": ["desc", "asc"], "description": "Sort direction (default desc)" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "Maximum items to return (default 50)" },
                    "offset": { "type": "integer", "minimum": 0, "description": "Pagination offset (default 0)" },
                    "metric": { "type": "string", "enum": ["disk", "logical"] }
                }
            }
        },
        {
            "name": "search_files",
            "description": "Fast multi-core filename and path search over the scanned directory (auto-scans 'path' if no scan is cached).",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Case-insensitive filename or path substring" },
                    "path": { "type": "string", "description": "Optional directory path (uses cached scan if omitted)" },
                    "category": { "type": "string", "description": "Optional category filter (video, disk_image, archive, build_artifact, image, audio, application, document, system_cache, other)" },
                    "ext": { "type": "string", "description": "Optional file extension filter (e.g. mp4, zip)" },
                    "min_size": { "type": "integer", "minimum": 0, "description": "Minimum file size in bytes" },
                    "max_size": { "type": "integer", "minimum": 0, "description": "Maximum file size in bytes" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Maximum results to return (default 50)" },
                    "metric": { "type": "string", "enum": ["disk", "logical"] }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_largest_files",
            "description": "Return the largest files sorted descending by size, with optional category, extension, and minimum age filters.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional directory path (uses cached scan if omitted)" },
                    "category": { "type": "string" },
                    "ext": { "type": "string" },
                    "min_age_days": { "type": "integer", "minimum": 0, "description": "Only include files not modified in at least N days" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "metric": { "type": "string", "enum": ["disk", "logical"] }
                }
            }
        },
        {
            "name": "get_group_summary",
            "description": "Group storage usage by category, extension, size_tier, or age across all CPU cores.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "group_by": { "type": "string", "enum": ["category", "extension", "size_tier", "age"] },
                    "path": { "type": "string", "description": "Optional directory path (uses cached scan if omitted)" },
                    "metric": { "type": "string", "enum": ["disk", "logical"] }
                },
                "required": ["group_by"]
            }
        },
        {
            "name": "get_bloat_dirs",
            "description": "Find regeneratable build, dependency, and cache directories (node_modules, target, DerivedData, .venv, etc.) sorted by size.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional directory path (uses cached scan if omitted)" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "metric": { "type": "string", "enum": ["disk", "logical"] }
                }
            }
        },
        {
            "name": "list_volumes",
            "description": "List mounted disks/volumes and common user quick targets with storage usage.",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "get_cache_status",
            "description": "Get status of the currently cached scan in memory (root path, node/file counts, memory usage, timestamp).",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "trash_paths",
            "description": "Safely move files or directories to the OS Trash / Recycle Bin (protected paths blocked) with optional dry-run preview and incremental cache update.",
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "List of paths to move to OS Trash (supports ~/ and relative paths)"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, simulates deletion and returns exact items and bytes that would be reclaimed without deleting anything"
                    }
                },
                "required": ["paths"]
            }
        },
        {
            "name": "reveal_in_file_manager",
            "description": "Reveal a file or folder in the native OS file manager (Finder, Explorer, or xdg-open).",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to reveal" }
                },
                "required": ["path"]
            }
        }
    ])
}

/// Returns definitions for all supported MCP resources.
#[must_use]
pub fn resource_definitions() -> Value {
    json!([
        {
            "uri": "fossil://volumes",
            "name": "Mounted Storage Volumes",
            "description": "Current mounted storage disks, volumes, and common user target folders with disk space metrics.",
            "mimeType": "application/json"
        },
        {
            "uri": "fossil://cache/summary",
            "name": "Cached Scan Summary",
            "description": "Summary metadata of the currently scanned and cached directory tree in memory.",
            "mimeType": "application/json"
        }
    ])
}

/// Returns definitions for all supported MCP prompts.
#[must_use]
pub fn prompt_definitions() -> Value {
    json!([
        {
            "name": "audit-disk-space",
            "description": "Perform an end-to-end disk space audit of a folder or drive, identifying top hogs, category distribution, and reclaimable space.",
            "arguments": [
                {
                    "name": "path",
                    "description": "Directory path to audit (e.g. ~, ~/Projects, /Volumes/External)",
                    "required": false
                },
                {
                    "name": "metric",
                    "description": "Size metric (disk or logical)",
                    "required": false
                }
            ]
        },
        {
            "name": "clean-dev-bloat",
            "description": "Scan and analyze regeneratable developer bloat (node_modules, target, .venv, DerivedData, build caches) with safe dry-run preview.",
            "arguments": [
                {
                    "name": "path",
                    "description": "Root project directory (e.g. ~/Projects)",
                    "required": false
                },
                {
                    "name": "limit",
                    "description": "Maximum number of bloat directories to return",
                    "required": false
                }
            ]
        },
        {
            "name": "find-old-large-files",
            "description": "Identify large files that have not been modified for an extended period of time (e.g. old installers, videos, archives).",
            "arguments": [
                {
                    "name": "path",
                    "description": "Directory to inspect (e.g. ~/Downloads)",
                    "required": false
                },
                {
                    "name": "min_age_days",
                    "description": "Minimum untouched age in days (default: 90)",
                    "required": false
                },
                {
                    "name": "limit",
                    "description": "Maximum files to list (default: 25)",
                    "required": false
                }
            ]
        }
    ])
}
