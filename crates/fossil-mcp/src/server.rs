use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fossil_core::arena::{FileArena, NodeId, SizeMetric};
use fossil_core::classify::FileCategory;
use fossil_core::grouping::{self, GroupBy, SortField};
use fossil_core::ops;
use fossil_core::scanner::{self, ScanOptions};
use fossil_core::volumes;
use parking_lot::RwLock;
use serde_json::{Value, json};

use crate::protocol::{
    DEFAULT_PROTOCOL_VERSION, JsonRpcRequest, JsonRpcResponse, negotiate_protocol_version,
    prompt_definitions, resource_definitions, tool_definitions,
};

#[derive(Debug, Clone, Default)]
pub struct ServerSecurityPolicy {
    pub read_only: bool,
    pub allowed_dirs: Vec<PathBuf>,
}

impl ServerSecurityPolicy {
    #[must_use]
    pub fn new(read_only: bool, allowed_dirs: Vec<PathBuf>) -> Self {
        let canonical_allowed: Vec<PathBuf> = allowed_dirs
            .into_iter()
            .map(|p| expand_path(&p))
            .filter_map(|p| p.canonicalize().ok().or(Some(p)))
            .collect();

        Self {
            read_only,
            allowed_dirs: canonical_allowed,
        }
    }

    pub fn check_write_allowed(&self) -> Result<(), String> {
        if self.read_only {
            Err("Operation rejected: Fossil MCP server is running in read-only mode.".to_owned())
        } else {
            Ok(())
        }
    }

    pub fn check_path_allowed(&self, path: &Path) -> Result<PathBuf, String> {
        let expanded = expand_path(path);
        if self.allowed_dirs.is_empty() {
            return Ok(expanded);
        }

        let canonical = expanded.canonicalize().unwrap_or_else(|_| expanded.clone());

        for allowed in &self.allowed_dirs {
            if canonical.starts_with(allowed) {
                return Ok(expanded);
            }
        }

        Err(format!(
            "Access denied: Path '{}' is outside configured allowed directories: {:?}",
            path.display(),
            self.allowed_dirs
        ))
    }
}

pub struct McpServer {
    arena: RwLock<Option<FileArena>>,
    cached_timestamp_ms: RwLock<u64>,
    cancel_token: Arc<AtomicBool>,
    policy: ServerSecurityPolicy,
    negotiated_version: RwLock<String>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new(ServerSecurityPolicy::default())
    }
}

impl McpServer {
    #[must_use]
    pub fn new(policy: ServerSecurityPolicy) -> Self {
        Self {
            arena: RwLock::new(None),
            cached_timestamp_ms: RwLock::new(0),
            cancel_token: Arc::new(AtomicBool::new(false)),
            policy,
            negotiated_version: RwLock::new(DEFAULT_PROTOCOL_VERSION.to_owned()),
        }
    }

    #[must_use]
    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_token)
    }

    #[must_use]
    pub fn policy(&self) -> &ServerSecurityPolicy {
        &self.policy
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

        // Handle notifications
        if req.id.is_none() {
            self.handle_notification(&req.method, req.params.unwrap_or(Value::Null));
            return None;
        }

        let id = req.id.unwrap_or(Value::Null);
        let resp = self.dispatch(id, &req.method, req.params.unwrap_or(Value::Null));
        serde_json::to_string(&resp).ok()
    }

    fn handle_notification(&self, method: &str, _params: Value) {
        match method {
            "notifications/initialized" => {
                // Client confirmed initialization.
            }
            "notifications/cancelled" => {
                self.cancel_token.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    pub fn dispatch(&self, id: Value, method: &str, params: Value) -> JsonRpcResponse {
        match method {
            "initialize" => {
                let requested_ver = params.get("protocolVersion").and_then(Value::as_str);
                let negotiated = negotiate_protocol_version(requested_ver);
                *self.negotiated_version.write() = negotiated.to_owned();

                JsonRpcResponse::ok(
                    id,
                    json!({
                        "protocolVersion": negotiated,
                        "capabilities": {
                            "tools": { "listChanged": false },
                            "resources": { "subscribe": false, "listChanged": false },
                            "prompts": { "listChanged": false }
                        },
                        "serverInfo": {
                            "name": "fossil",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                )
            }
            "ping" => JsonRpcResponse::ok(id, json!({})),
            "tools/list" => JsonRpcResponse::ok(id, json!({ "tools": tool_definitions() })),
            "tools/call" => self.handle_tool_call(id, &params),
            "resources/list" => {
                JsonRpcResponse::ok(id, json!({ "resources": resource_definitions() }))
            }
            "resources/read" => self.handle_resource_read(id, &params),
            "prompts/list" => JsonRpcResponse::ok(id, json!({ "prompts": prompt_definitions() })),
            "prompts/get" => self.handle_prompt_get(id, &params),
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
            Ok((summary_text, structured)) => {
                let json_text =
                    serde_json::to_string_pretty(&structured).unwrap_or_else(|_| "{}".to_owned());
                let combined_text = format!("{summary_text}\n\n```json\n{json_text}\n```");

                JsonRpcResponse::ok(
                    id,
                    json!({
                        "content": [
                            {
                                "type": "text",
                                "text": combined_text
                            }
                        ],
                        "structuredContent": structured,
                        "isError": false
                    }),
                )
            }
            Err(err_msg) => JsonRpcResponse::ok(
                id,
                json!({
                    "content": [{ "type": "text", "text": format!("Error: {err_msg}") }],
                    "isError": true
                }),
            ),
        }
    }

    fn execute_tool(&self, name: &str, args: &Value) -> Result<(String, Value), String> {
        match name {
            "scan_directory" => self.tool_scan_directory(args),
            "list_children" => self.tool_list_children(args),
            "search_files" => self.tool_search_files(args),
            "get_largest_files" => self.tool_get_largest_files(args),
            "get_group_summary" => self.tool_get_group_summary(args),
            "get_bloat_dirs" => self.tool_get_bloat_dirs(args),
            "list_volumes" => self.tool_list_volumes(),
            "get_cache_status" => self.tool_get_cache_status(),
            "trash_paths" => self.tool_trash_paths(args),
            "reveal_in_file_manager" => self.tool_reveal(args),
            _ => Err(format!("Unknown tool: {name}")),
        }
    }

    fn handle_resource_read(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return JsonRpcResponse::err(id, -32602, "Missing required 'uri' in resources/read");
        };

        match uri {
            "fossil://volumes" => {
                let vols = volumes::list_volumes();
                let targets = volumes::list_quick_targets();
                let payload = json!({
                    "volumes": vols,
                    "quick_targets": targets
                });
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": serde_json::to_string_pretty(&payload).unwrap_or_default()
                        }]
                    }),
                )
            }
            "fossil://cache/summary" => {
                let guard = self.arena.read();
                let ts = *self.cached_timestamp_ms.read();
                let payload = if let Some(ref arena) = *guard {
                    json!({
                        "cached": true,
                        "scanned_at_epoch_ms": ts,
                        "root_path": arena.root_path.to_string_lossy(),
                        "root_name": arena.node_name(0),
                        "total_disk_bytes": arena.nodes[0].disk_size,
                        "total_logical_bytes": arena.nodes[0].logical_size,
                        "total_files": arena.nodes[0].file_count,
                        "total_nodes": arena.nodes.len(),
                        "backend_used": arena.backend_used
                    })
                } else {
                    json!({
                        "cached": false,
                        "message": "No scan cached in memory. Run scan_directory first."
                    })
                };
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": serde_json::to_string_pretty(&payload).unwrap_or_default()
                        }]
                    }),
                )
            }
            _ => JsonRpcResponse::err(id, -32602, format!("Resource not found: {uri}")),
        }
    }

    fn handle_prompt_get(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return JsonRpcResponse::err(id, -32602, "Missing required 'name' in prompts/get");
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match name {
            "audit-disk-space" => {
                let target = args.get("path").and_then(Value::as_str).unwrap_or("~");
                let metric = args.get("metric").and_then(Value::as_str).unwrap_or("disk");
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "description": format!("Perform disk space audit for {target}"),
                        "messages": [
                            {
                                "role": "user",
                                "content": {
                                    "type": "text",
                                    "text": format!(
                                        "Please audit the storage usage of '{target}' using Fossil MCP tools.\n\
                                         1. First call `scan_directory` with path='{target}' and metric='{metric}'.\n\
                                         2. Call `get_group_summary` grouped by 'category' and 'extension' to identify major space consumers.\n\
                                         3. Call `get_bloat_dirs` to discover regeneratable caches or dependency folders.\n\
                                         4. Call `get_largest_files` (limit=20) to spotlight individual large files.\n\
                                         5. Provide a clear summary and actionable cleanup recommendations with dry-run options."
                                    )
                                }
                            }
                        ]
                    }),
                )
            }
            "clean-dev-bloat" => {
                let target = args.get("path").and_then(Value::as_str).unwrap_or(".");
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(25);
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "description": format!("Analyze regeneratable developer bloat in {target}"),
                        "messages": [
                            {
                                "role": "user",
                                "content": {
                                    "type": "text",
                                    "text": format!(
                                        "Please analyze regeneratable build artifacts and dependency bloat in '{target}'.\n\
                                         1. Call `get_bloat_dirs` with path='{target}' and limit={limit}.\n\
                                         2. Group by category and compute total reclaimable disk space.\n\
                                         3. Recommend safe cleanup steps (e.g. `cargo clean`, `npm/pnpm/yarn clean`, or `trash_paths` with dry_run=true)."
                                    )
                                }
                            }
                        ]
                    }),
                )
            }
            "find-old-large-files" => {
                let target = args
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("~/Downloads");
                let min_age = args
                    .get("min_age_days")
                    .and_then(Value::as_u64)
                    .unwrap_or(90);
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(25);
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "description": format!("Find large untouched files in {target}"),
                        "messages": [
                            {
                                "role": "user",
                                "content": {
                                    "type": "text",
                                    "text": format!(
                                        "Please find large files in '{target}' that have not been modified in at least {min_age} days.\n\
                                         1. Call `get_largest_files` with path='{target}', min_age_days={min_age}, and limit={limit}.\n\
                                         2. Highlight any installer (.dmg, .pkg, .iso, .exe), archive (.zip, .tar, .tgz), or video files.\n\
                                         3. Show their size and last modified age, and ask before moving any to Trash."
                                    )
                                }
                            }
                        ]
                    }),
                )
            }
            _ => JsonRpcResponse::err(id, -32602, format!("Unknown prompt: {name}")),
        }
    }

    fn scan_and_cache(
        &self,
        path: PathBuf,
        same_filesystem: bool,
        skip_hidden: bool,
        threads: Option<usize>,
    ) -> Result<(), String> {
        self.cancel_token.store(false, Ordering::SeqCst);
        let options = ScanOptions {
            same_filesystem,
            skip_hidden,
            threads,
        };
        let arena = scanner::scan_directory(path, &options, &self.cancel_token, |_| {})?;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);

        *self.cached_timestamp_ms.write() = now_ms;
        *self.arena.write() = Some(arena);
        Ok(())
    }

    fn resolve_arena_root(&self, path_arg: Option<&str>) -> Result<NodeId, String> {
        if let Some(p_str) = path_arg {
            let target = self.policy.check_path_allowed(Path::new(p_str))?;
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
            Err("No scanned directory in cache. Call scan_directory first or provide a 'path' argument.".to_owned())
        }
    }

    fn tool_scan_directory(&self, args: &Value) -> Result<(String, Value), String> {
        let path_str = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'path'")?;
        let target_path = self.policy.check_path_allowed(Path::new(path_str))?;
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

        self.scan_and_cache(target_path, same_filesystem, skip_hidden, threads)?;

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
            15,
        );

        let root_node = &arena.nodes[0];
        let summary_text = format!(
            "Scanned: '{}' ({} files, {} nodes) in {} ms [{}]\nTotal Storage: {} disk / {} logical",
            arena.root_path.display(),
            root_node.file_count,
            arena.nodes.len(),
            arena.scan_duration_ms,
            arena.backend_used,
            format_bytes(root_node.disk_size),
            format_bytes(root_node.logical_size)
        );

        let payload = json!({
            "root_id": 0,
            "root_path": arena.root_path.to_string_lossy(),
            "root_name": arena.node_name(0),
            "total_disk_bytes": root_node.disk_size,
            "total_logical_bytes": root_node.logical_size,
            "total_files": root_node.file_count,
            "total_nodes": arena.nodes.len(),
            "warnings_count": arena.warnings.len(),
            "scan_duration_ms": arena.scan_duration_ms,
            "backend_used": arena.backend_used,
            "top_children": top_children
        });

        Ok((summary_text, payload))
    }

    fn tool_list_children(&self, args: &Value) -> Result<(String, Value), String> {
        let path_arg = args.get("path").and_then(Value::as_str);
        let sort_by_str = args
            .get("sort_by")
            .and_then(Value::as_str)
            .unwrap_or("size");
        let order_str = args.get("order").and_then(Value::as_str).unwrap_or("desc");
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(50, |l| l as usize)
            .min(500);
        let offset = args
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |o| o as usize);
        let metric = parse_metric(args.get("metric").and_then(Value::as_str))?;

        let root_id = self.resolve_arena_root(path_arg)?;
        let guard = self.arena.read();
        let arena = guard.as_ref().ok_or("No scanned directory in cache")?;

        let sort_field = match sort_by_str {
            "name" => SortField::Name,
            "files" => SortField::FileCount,
            _ => match metric {
                SizeMetric::Disk => SortField::DiskSize,
                SizeMetric::Logical => SortField::LogicalSize,
            },
        };
        let descending = order_str.eq_ignore_ascii_case("desc");

        let (total_children, items) = grouping::get_children(
            arena, root_id, metric, sort_field, descending, None, None, offset, limit,
        );

        let parent_path = if root_id == 0 {
            arena.root_path.to_string_lossy().into_owned()
        } else {
            arena.full_path(root_id).to_string_lossy().into_owned()
        };

        let summary_text = format!(
            "Directory: '{}' has {} entries (showing {}..{})",
            parent_path,
            total_children,
            offset + 1,
            (offset + items.len()).min(total_children)
        );

        let payload = json!({
            "parent_path": parent_path,
            "total_children": total_children,
            "offset": offset,
            "limit": limit,
            "items": items
        });

        Ok((summary_text, payload))
    }

    fn tool_search_files(&self, args: &Value) -> Result<(String, Value), String> {
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

        let summary_text = format!(
            "Search for '{}': found {} matching files (showing top {})",
            query,
            total,
            items.len()
        );

        let payload = json!({
            "total": total,
            "items": items
        });

        Ok((summary_text, payload))
    }

    fn tool_get_largest_files(&self, args: &Value) -> Result<(String, Value), String> {
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

        let summary_text = format!(
            "Largest files: {} matching files (showing top {})",
            total,
            items.len()
        );

        let payload = json!({
            "total": total,
            "items": items
        });

        Ok((summary_text, payload))
    }

    fn tool_get_group_summary(&self, args: &Value) -> Result<(String, Value), String> {
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

        let summary_text = format!(
            "Group breakdown by '{}': {} groups found",
            group_str,
            groups.len()
        );

        let payload = json!({
            "group_by": group_str,
            "groups": groups
        });

        Ok((summary_text, payload))
    }

    fn tool_get_bloat_dirs(&self, args: &Value) -> Result<(String, Value), String> {
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

        let total_bloat_bytes: u64 = dirs
            .iter()
            .map(|d| match metric {
                SizeMetric::Disk => d.disk_size,
                SizeMetric::Logical => d.logical_size,
            })
            .sum();

        let summary_text = format!(
            "Found {} regeneratable bloat directories totaling {}",
            dirs.len(),
            format_bytes(total_bloat_bytes)
        );

        let payload = json!({
            "dirs": dirs,
            "total_bloat_bytes": total_bloat_bytes
        });

        Ok((summary_text, payload))
    }

    fn tool_list_volumes(&self) -> Result<(String, Value), String> {
        let vols = volumes::list_volumes();
        let targets = volumes::list_quick_targets();

        let summary_text = format!(
            "Found {} mounted volumes and {} quick targets",
            vols.len(),
            targets.len()
        );

        let payload = json!({
            "volumes": vols,
            "quick_targets": targets
        });

        Ok((summary_text, payload))
    }

    fn tool_get_cache_status(&self) -> Result<(String, Value), String> {
        let guard = self.arena.read();
        let ts = *self.cached_timestamp_ms.read();

        if let Some(ref arena) = *guard {
            let summary_text = format!(
                "Cached root: '{}' ({} files, {} nodes, {} ms scan)",
                arena.root_path.display(),
                arena.nodes[0].file_count,
                arena.nodes.len(),
                arena.scan_duration_ms
            );

            let payload = json!({
                "cached": true,
                "scanned_at_epoch_ms": ts,
                "root_path": arena.root_path.to_string_lossy(),
                "total_files": arena.nodes[0].file_count,
                "total_nodes": arena.nodes.len(),
                "total_disk_bytes": arena.nodes[0].disk_size,
                "total_logical_bytes": arena.nodes[0].logical_size,
                "backend_used": arena.backend_used,
                "warnings_count": arena.warnings.len()
            });

            Ok((summary_text, payload))
        } else {
            let summary_text = "No directory is currently scanned in memory cache.".to_owned();
            let payload = json!({
                "cached": false
            });
            Ok((summary_text, payload))
        }
    }

    fn tool_trash_paths(&self, args: &Value) -> Result<(String, Value), String> {
        self.policy.check_write_allowed()?;

        let raw_paths = args
            .get("paths")
            .and_then(Value::as_array)
            .ok_or("Missing required array argument: 'paths'")?;
        let dry_run = args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut paths = Vec::with_capacity(raw_paths.len());
        for val in raw_paths {
            let s = val
                .as_str()
                .ok_or("All entries in 'paths' must be strings")?;
            let checked_path = self.policy.check_path_allowed(Path::new(s))?;
            paths.push(checked_path);
        }
        if paths.is_empty() {
            return Err("'paths' must not be empty".to_owned());
        }

        // Verify protected path guard
        for p in &paths {
            if ops::is_protected_path(p) {
                return Err(format!(
                    "Refusing to delete protected system path: {}",
                    p.display()
                ));
            }
        }

        let mut estimated_disk = 0u64;
        let mut estimated_logical = 0u64;

        if dry_run {
            let guard = self.arena.read();
            for p in &paths {
                if let Some(ref arena) = *guard
                    && let Some(id) = arena.find_node_by_path(p)
                {
                    estimated_disk =
                        estimated_disk.saturating_add(arena.nodes[id as usize].disk_size);
                    estimated_logical =
                        estimated_logical.saturating_add(arena.nodes[id as usize].logical_size);
                    continue;
                }
                if let Ok(meta) = p.metadata() {
                    estimated_disk = estimated_disk.saturating_add(meta.len());
                    estimated_logical = estimated_logical.saturating_add(meta.len());
                }
            }

            let summary_text = format!(
                "[DRY RUN] Would move {} item(s) to Trash, reclaiming approximately {} disk / {} logical",
                paths.len(),
                format_bytes(estimated_disk),
                format_bytes(estimated_logical)
            );

            let payload = json!({
                "dry_run": true,
                "paths_count": paths.len(),
                "estimated_reclaimed_disk_bytes": estimated_disk,
                "estimated_reclaimed_logical_bytes": estimated_logical,
                "paths": paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>()
            });

            return Ok((summary_text, payload));
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

        let summary_text = format!(
            "Safely moved {} item(s) to OS Trash, reclaimed {} disk / {} logical",
            paths.len(),
            format_bytes(reclaimed_disk),
            format_bytes(reclaimed_logical)
        );

        let payload = json!({
            "dry_run": false,
            "trashed_count": paths.len(),
            "reclaimed_disk_bytes": reclaimed_disk,
            "reclaimed_logical_bytes": reclaimed_logical,
            "paths": paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>()
        });

        Ok((summary_text, payload))
    }

    fn tool_reveal(&self, args: &Value) -> Result<(String, Value), String> {
        self.policy.check_write_allowed()?;

        let path_str = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Missing required argument: 'path'")?;
        let path = self.policy.check_path_allowed(Path::new(path_str))?;
        ops::reveal_in_file_manager(&path)?;

        let summary_text = format!("Revealed in file manager: {}", path.display());
        let payload = json!({ "revealed": path.to_string_lossy() });
        Ok((summary_text, payload))
    }
}

pub fn expand_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if (s == "~" || s.starts_with("~/") || s.starts_with("~\\"))
        && let Some(home) = home_dir()
    {
        if s == "~" {
            return home;
        }
        return home.join(&s[2..]);
    }
    p.to_path_buf()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .map(PathBuf::from)
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

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes as f64;
    let mut unit = 0usize;
    while val >= 1024.0 && unit + 1 < UNITS.len() {
        val /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{val:.1} {}", UNITS[unit])
    }
}
