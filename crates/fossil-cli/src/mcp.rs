use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use fossil_core::arena::{FileArena, NodeId, SizeMetric};
use fossil_core::classify::FileCategory;
use fossil_core::grouping::{self, GroupBy, SortField};
use fossil_core::ops;
use fossil_core::scanner::{self, ScanOptions};
use fossil_core::volumes;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub struct McpServer {
    arena: RwLock<Option<FileArena>>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            arena: RwLock::new(None),
        }
    }

    /// Processes a single JSON-RPC 2.0 request line, returning `Some(json_line)` for requests
    /// or `None` for notifications (requests without `id`).
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(Value::Null, -32700, format!("Parse error: {e}"));
                return serde_json::to_string(&resp).ok();
            }
        };

        // JSON-RPC notifications (e.g. `notifications/initialized`) have no `id` and expect no reply.
        let id = req.id?;
        let resp = self.dispatch(id, &req.method, req.params.unwrap_or(Value::Null));
        serde_json::to_string(&resp).ok()
    }

    fn dispatch(&self, id: Value, method: &str, params: Value) -> JsonRpcResponse {
        match method {
            "initialize" => JsonRpcResponse::ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "fossil",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
            "ping" => JsonRpcResponse::ok(id, json!({})),
            "tools/list" => JsonRpcResponse::ok(id, json!({ "tools": tool_definitions() })),
            "tools/call" => self.handle_tool_call(id, &params),
            _ => JsonRpcResponse::err(id, -32601, format!("Method not found: {method}")),
        }
    }

    fn handle_tool_call(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return JsonRpcResponse::err(id, -32602, "Missing required 'name' in tools/call");
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match self.execute_tool(name, &args) {
            Ok(payload) => {
                let text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }),
                )
            }
            Err(err_msg) => JsonRpcResponse::ok(
                id,
                json!({
                    "content": [{ "type": "text", "text": err_msg }],
                    "isError": true
                }),
            ),
        }
    }

    fn execute_tool(&self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "scan_directory" => self.tool_scan_directory(args),
            "search_files" => self.tool_search_files(args),
            "get_largest_files" => self.tool_get_largest_files(args),
            "get_group_summary" => self.tool_get_group_summary(args),
            "get_bloat_dirs" => self.tool_get_bloat_dirs(args),
            "list_volumes" => Ok(json!({
                "volumes": volumes::list_volumes(),
                "quick_targets": volumes::list_quick_targets()
            })),
            "trash_paths" => self.tool_trash_paths(args),
            "reveal_in_file_manager" => self.tool_reveal(args),
            _ => Err(format!("Unknown tool: {name}")),
        }
    }

    fn scan_and_cache(
        &self,
        path: PathBuf,
        same_filesystem: bool,
        skip_hidden: bool,
        threads: Option<usize>,
    ) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let options = ScanOptions {
            same_filesystem,
            skip_hidden,
            threads,
        };
        let arena = scanner::scan_directory(path, &options, &cancel, |_| {})?;
        *self.arena.write() = Some(arena);
        Ok(())
    }

    /// Ensures an arena is loaded in cache. If `path_arg` is provided and either no arena is
    /// cached or `path_arg` is outside the cached root, automatically scans `path_arg`.
    fn resolve_arena_root(&self, path_arg: Option<&str>) -> Result<NodeId, String> {
        if let Some(p_str) = path_arg {
            let target = PathBuf::from(p_str);
            {
                let guard = self.arena.read();
                if let Some(ref arena) = *guard {
                    if paths_match(&arena.root_path, &target) {
                        return Ok(0);
                    }
                    if let Some(sub_id) = arena.find_node_by_path(&target) {
                        return Ok(sub_id);
                    }
                    if let (Ok(canon_root), Ok(canon_target)) =
                        (arena.root_path.canonicalize(), target.canonicalize())
                    {
                        if canon_root == canon_target {
                            return Ok(0);
                        }
                        if let Some(sub_id) = arena.find_node_by_path(&canon_target) {
                            return Ok(sub_id);
                        }
                    }
                }
            }
            self.scan_and_cache(target, true, false, None)?;
            return Ok(0);
        }

        if self.arena.read().is_some() {
            Ok(0)
        } else {
            Err(
                "No scanned directory in cache. Call scan_directory first or pass a 'path' argument."
                    .to_owned(),
            )
        }
    }

    fn tool_scan_directory(&self, args: &Value) -> Result<Value, String> {
        let path_str = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'path'")?;
        let same_filesystem = args
            .get("same_filesystem")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let skip_hidden = args
            .get("skip_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let threads = args
            .get("threads")
            .and_then(Value::as_u64)
            .map(|t| t as usize);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        self.scan_and_cache(
            PathBuf::from(path_str),
            same_filesystem,
            skip_hidden,
            threads,
        )?;

        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("Scan failed to populate cache")?;
        let (_, top_children) = grouping::get_children(
            arena,
            0,
            metric,
            match metric {
                SizeMetric::Disk => SortField::DiskSize,
                SizeMetric::Logical => SortField::LogicalSize,
            },
            true,
            None,
            None,
            0,
            20,
        );

        Ok(json!({
            "root_id": 0,
            "root_path": arena.root_path.to_string_lossy(),
            "root_name": arena.node_name(0),
            "total_disk_bytes": arena.nodes[0].disk_size,
            "total_logical_bytes": arena.nodes[0].logical_size,
            "total_files": arena.nodes[0].file_count,
            "total_nodes": arena.nodes.len(),
            "warnings_count": arena.warnings.len(),
            "scan_duration_ms": arena.scan_duration_ms,
            "backend_used": arena.backend_used,
            "top_children": top_children
        }))
    }

    fn tool_search_files(&self, args: &Value) -> Result<Value, String> {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'query'")?;
        let path = args.get("path").and_then(Value::as_str);
        let category = parse_category(args.get("category").and_then(Value::as_str))?;
        let ext = args.get("ext").and_then(Value::as_str);
        let min_size = args.get("min_size").and_then(Value::as_u64);
        let max_size = args.get("max_size").and_then(Value::as_u64);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(50, |l| l as usize)
            .min(1000);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        let root_id = self.resolve_arena_root(path)?;
        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("No scanned directory in cache")?;
        let (total, items) = grouping::search_files(
            arena, root_id, query, category, ext, min_size, max_size, metric, limit,
        );

        Ok(json!({
            "total": total,
            "items": items
        }))
    }

    fn tool_get_largest_files(&self, args: &Value) -> Result<Value, String> {
        let path = args.get("path").and_then(Value::as_str);
        let category = parse_category(args.get("category").and_then(Value::as_str))?;
        let ext = args.get("ext").and_then(Value::as_str);
        let min_age_days = args.get("min_age_days").and_then(Value::as_u64);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(50, |l| l as usize)
            .min(1000);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        let root_id = self.resolve_arena_root(path)?;
        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("No scanned directory in cache")?;
        let (total, items) = grouping::get_largest_files(
            arena,
            root_id,
            metric,
            category,
            ext,
            None,
            min_age_days,
            0,
            limit,
        );

        Ok(json!({
            "total": total,
            "items": items
        }))
    }

    fn tool_get_group_summary(&self, args: &Value) -> Result<Value, String> {
        let group_str = args
            .get("group_by")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'group_by'")?;
        let group_by = GroupBy::from_key(group_str).ok_or_else(|| {
            format!(
                "Invalid group_by '{group_str}'; expected category, extension, size_tier, or age"
            )
        })?;
        let path = args.get("path").and_then(Value::as_str);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        let root_id = self.resolve_arena_root(path)?;
        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("No scanned directory in cache")?;
        let groups = grouping::get_group_summary(arena, root_id, group_by, metric);

        Ok(json!({
            "group_by": group_str,
            "groups": groups
        }))
    }

    fn tool_get_bloat_dirs(&self, args: &Value) -> Result<Value, String> {
        let path = args.get("path").and_then(Value::as_str);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(50, |l| l as usize)
            .min(1000);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        let root_id = self.resolve_arena_root(path)?;
        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("No scanned directory in cache")?;
        let dirs = grouping::get_bloat_dirs(arena, root_id, metric, limit);

        Ok(json!({ "dirs": dirs }))
    }

    fn tool_trash_paths(&self, args: &Value) -> Result<Value, String> {
        let raw_paths = args
            .get("paths")
            .and_then(Value::as_array)
            .ok_or("Missing required array argument: 'paths'")?;
        let mut paths = Vec::with_capacity(raw_paths.len());
        for val in raw_paths {
            let s = val
                .as_str()
                .ok_or("All entries in 'paths' must be strings")?;
            paths.push(PathBuf::from(s));
        }
        if paths.is_empty() {
            return Err("'paths' must not be empty".to_owned());
        }

        ops::trash_paths(&paths)?;

        let mut reclaimed_disk = 0u64;
        let mut reclaimed_logical = 0u64;
        if let Some(ref mut arena) = *self.arena.write() {
            for p in &paths {
                if let Some(id) = arena.find_node_by_path(p) {
                    let (d, l, _) = arena.mark_deleted(id);
                    reclaimed_disk = reclaimed_disk.saturating_add(d);
                    reclaimed_logical = reclaimed_logical.saturating_add(l);
                }
            }
        }

        Ok(json!({
            "trashed_count": paths.len(),
            "reclaimed_disk_bytes": reclaimed_disk,
            "reclaimed_logical_bytes": reclaimed_logical,
            "paths": paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>()
        }))
    }

    fn tool_reveal(&self, args: &Value) -> Result<Value, String> {
        let path_str = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'path'")?;
        let path = Path::new(path_str);
        ops::reveal_in_file_manager(path)?;
        Ok(json!({ "revealed": path_str }))
    }
}

fn paths_match(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    let sa = a.to_string_lossy();
    let sb = b.to_string_lossy();
    sa.trim_end_matches(['/', '\\']) == sb.trim_end_matches(['/', '\\'])
}

fn parse_metric(raw: Option<&str>) -> Result<SizeMetric, String> {
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        None | Some("disk") => Ok(SizeMetric::Disk),
        Some("logical") => Ok(SizeMetric::Logical),
        Some(other) => Err(format!(
            "Invalid metric '{other}'; expected disk or logical"
        )),
    }
}

fn parse_category(raw: Option<&str>) -> Result<Option<FileCategory>, String> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    FileCategory::from_key(s)
        .map(Some)
        .ok_or_else(|| format!("Unknown file category: '{s}'"))
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "scan_directory",
            "description": "Scan a directory in parallel across all CPU cores, caching the index for instant follow-up queries and returning storage totals plus top children.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to scan" },
                    "same_filesystem": { "type": "boolean", "description": "Stay on the same filesystem/mount (default true)" },
                    "skip_hidden": { "type": "boolean", "description": "Skip hidden dotfiles/directories (default false)" },
                    "threads": { "type": "integer", "minimum": 1, "description": "Optional worker thread count (defaults to all CPU cores)" },
                    "metric": { "type": "string", "enum": ["disk", "logical"], "description": "Size metric (default disk)" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "search_files",
            "description": "Fast multi-core filename and path search over the scanned directory (auto-scans 'path' if no scan is cached).",
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
            "description": "Find known regeneratable build/dependency/cache directories (node_modules, target, DerivedData, .venv, etc.) sorted by size.",
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
            "description": "List mounted disks/volumes and common user quick targets.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "trash_paths",
            "description": "Safely move files or directories to the OS Trash / Recycle Bin (protected system paths are blocked) and update the cached scan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "Paths to move to OS Trash"
                    }
                },
                "required": ["paths"]
            }
        },
        {
            "name": "reveal_in_file_manager",
            "description": "Reveal a file or folder in the native OS file manager (Finder, Explorer, or xdg-open).",
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

/// Runs the line-delimited JSON-RPC 2.0 MCP server over `stdin`/`stdout`.
pub fn run_stdio_server() -> Result<(), String> {
    let server = McpServer::new();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line_res in stdin.lock().lines() {
        let line = line_res.map_err(|e| format!("Failed reading stdin: {e}"))?;
        if let Some(reply) = server.handle_line(&line) {
            writeln!(out, "{reply}").map_err(|e| format!("Failed writing stdout: {e}"))?;
            out.flush()
                .map_err(|e| format!("Failed flushing stdout: {e}"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn mcp_initialize_and_tools_list() {
        let server = McpServer::new();

        let init_raw = server
            .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            .expect("initialize returns response");
        let init: Value = serde_json::from_str(&init_raw).expect("valid json");
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(init["result"]["serverInfo"]["name"], "fossil");

        // Notification has no id and must produce no response
        assert!(
            server
                .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .is_none()
        );

        let list_raw = server
            .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .expect("tools/list returns response");
        let list: Value = serde_json::from_str(&list_raw).expect("valid json");
        let tools = list["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 8);
    }

    #[test]
    fn mcp_scan_and_cached_search_on_tempdir() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("node_modules");
        fs::create_dir(&sub).expect("create node_modules");
        fs::write(sub.join("chunk.js"), vec![b'a'; 2048]).expect("write js");
        fs::write(dir.path().join("movie_clip.mp4"), vec![b'b'; 8192]).expect("write mp4");

        let server = McpServer::new();

        let scan_req = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "scan_directory",
                "arguments": {
                    "path": dir.path().to_string_lossy()
                }
            }
        });
        let scan_resp_raw = server
            .handle_line(&scan_req.to_string())
            .expect("scan response");
        let scan_resp: Value = serde_json::from_str(&scan_resp_raw).expect("valid json");
        assert_eq!(scan_resp["result"]["isError"], false);
        let scan_text = scan_resp["result"]["content"][0]["text"]
            .as_str()
            .expect("text payload");
        let scan_data: Value = serde_json::from_str(scan_text).expect("valid payload json");
        assert_eq!(scan_data["total_files"], 2);
        assert_eq!(scan_data["total_logical_bytes"], 10240);

        // Now call search_files WITHOUT path, proving in-memory cached arena works
        let search_req = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "search_files",
                "arguments": {
                    "query": "movie_clip",
                    "metric": "logical"
                }
            }
        });
        let search_resp_raw = server
            .handle_line(&search_req.to_string())
            .expect("search response");
        let search_resp: Value = serde_json::from_str(&search_resp_raw).expect("valid json");
        assert_eq!(search_resp["result"]["isError"], false);
        let search_text = search_resp["result"]["content"][0]["text"]
            .as_str()
            .expect("text payload");
        let search_data: Value = serde_json::from_str(search_text).expect("valid payload json");
        assert_eq!(search_data["total"], 1);
        assert_eq!(search_data["items"][0]["name"], "movie_clip.mp4");

        // Verify bloat detection from cache
        let bloat_req = json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "get_bloat_dirs",
                "arguments": {}
            }
        });
        let bloat_raw = server
            .handle_line(&bloat_req.to_string())
            .expect("bloat response");
        let bloat_resp: Value = serde_json::from_str(&bloat_raw).expect("valid json");
        let bloat_data: Value =
            serde_json::from_str(bloat_resp["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(bloat_data["dirs"][0]["name"], "node_modules");

        // Verify protected-path guard via trash_paths
        let trash_req = json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "trash_paths",
                "arguments": { "paths": ["/"] }
            }
        });
        let trash_raw = server
            .handle_line(&trash_req.to_string())
            .expect("trash response");
        let trash_resp: Value = serde_json::from_str(&trash_raw).expect("valid json");
        assert_eq!(trash_resp["result"]["isError"], true);
    }
}
