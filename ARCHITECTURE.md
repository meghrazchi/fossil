# Fossil - Technical Architecture & Engineering Guide

This document describes the internal engineering architecture of **Fossil**, including its Cargo workspace structure, cache-friendly arena memory model, all-core parallel scanning and search engine, runtime OS compatibility fallbacks, binary IPC treemap pipeline, MCP server design, and CI/CD release automation.

For end-user usage and quick start guides, see [README.md](./README.md).

---

## 1. Workspace & Crate Architecture

Fossil is structured as a **Cargo Workspace** with a clean separation between the pure-Rust filesystem engine (`fossil-core`), the shared MCP integration engine (`fossil-mcp`), and its two frontends (`src-tauri` desktop shell and `fossil-cli` standalone CLI binary):

```text
fossil/
├── Cargo.toml                     # Workspace root, shared lints, release LTO profile
├── crates/
│   ├── fossil-core/               # Zero-GUI pure-Rust core library
│   │   └── src/
│   │       ├── arena.rs           # Flat FileNode arena, StringPool, PathBuf reconstruction
│   │       ├── classify.rs        # Compile-time phf extension map + bloat heuristics
│   │       ├── grouping.rs        # Multi-core rayon search, largest files, grouping & bloat
│   │       ├── ops.rs             # Native OS reveal, Trash-first delete, protected-path guard
│   │       ├── treemap.rs         # Rust squarified treemap layout + 32-byte binary encoder
│   │       ├── volumes.rs         # Cross-platform volume & quick-target discovery
│   │       └── scanner/
│   │           ├── mod.rs         # Multi-core jwalk + rayon scanner + progress streaming
│   │           └── compat.rs      # Runtime OS capability probing & legacy OS fallbacks
│   ├── fossil-mcp/                # Shared Model Context Protocol (MCP) library
│   │   └── src/
│   │       ├── protocol.rs        # JSON-RPC 2.0, version negotiation, tool annotations
│   │       ├── server.rs          # Stateful McpServer, tools, resources, prompts, security
│   │       ├── config/            # 1-click config installer (Claude, Gemini, Codex, etc.)
│   │       └── transport/         # stdio and Streamable HTTP (/mcp + Bearer auth + SSE)
│   └── fossil-cli/                # Standalone zero-GUI binary (`fossil`)
│       └── src/
│           └── main.rs            # Clap CLI subcommands (scan, search, largest, bloat, mcp)
├── src-tauri/                     # Thin Tauri v2 desktop wrapper over fossil-core & fossil-mcp
│   └── src/
│       ├── lib.rs                 # MiMalloc global allocator + Tauri plugin/command wiring
│       └── commands.rs            # IPC commands, Channel<ScanProgress>, binary Response, MCP
├── src/                           # Frontend (Svelte 5 + TypeScript + Tailwind CSS v4)
│   ├── lib/                       # Typed IPC wrappers, DataView binary decoder, formatters
│   └── components/                # DrivePicker, CategoryBar, TreemapCanvas, FolderTree, McpModal
└── .github/workflows/             # Multi-OS CI (ci.yml) & Binary Release pipeline (release.yml)
```

### Why This Split Matters

- **Zero GUI Bloat in CLI/MCP:** `fossil-cli` links only `fossil-core`, `clap`, `serde_json`, `parking_lot`, and `mimalloc` - no WebKit, GTK, or Tauri dependencies. It runs natively on headless Linux servers and older OSes.
- **Single Source of Truth:** Every bug fix, speed optimization, and edge-case protection in `fossil-core` immediately benefits the Desktop App, CLI, and MCP server simultaneously.
- **Release Optimization:** The root `[profile.release]` enables `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, and `strip = true` alongside the `mimalloc` global allocator for minimal binary size and low multi-threaded allocation contention.

---

## 2. Memory Model: Flat Arena & Contiguous String Pool

Storing a `PathBuf` and heap-allocated `Box<Node>` per file consumes ~200+ bytes/node and causes severe CPU cache misses on million-file scans. Instead, `crates/fossil-core/src/arena.rs` uses a **flat arena**:

- **`nodes: Vec<FileNode>`**: Indexed by `NodeId = u32` (supports up to 4.29 billion nodes).
- **`names: Vec<u8>`**: Single contiguous byte buffer storing only file/folder basenames referenced by `(name_offset: u32, name_len: u16)`.
- **`extensions: Vec<String>`**: Interned extension table indexed by `ext_id: u16`.
- **On-Demand `PathBuf` Reconstruction:** Walking `parent: NodeId` pointers up to root (`id == 0`) reconstructs the exact real path on disk in nanoseconds with zero duplicate path storage in RAM.
- **Bottom-Up Aggregation & Parallel Sort (`finalize()`):**
  1. Iterates in reverse index order (parents are always inserted before children) to sum `disk_size`, `logical_size`, `file_count`, and dominant `FileCategory` in a single linear pass.
  2. Sorts every directory's `children: Vec<NodeId>` descending by `disk_size` in parallel using `rayon::par_iter_mut()`.
- **`< 1 ms` Incremental Deletion (`mark_deleted()`):** Marks the deleted subtree via bitflag `NodeFlags::DELETED` and walks the `parent` chain up to root subtracting `disk_size`, `logical_size`, and `file_count`. No rescan is required.

---

## 3. Classification & All-Core Parallel Search / Grouping

### Inline O(1) Classification (`classify.rs`)

- Extensions are classified during scan with a compile-time perfect hash map (`phf::phf_map!`) - zero heap allocation and O(1) lookup across 10 `FileCategory` buckets.
- Known regeneratable directories (`node_modules`, `target`, `.venv`, `DerivedData`, `.gradle`, `Pods`, `.next`, `Caches`, etc.) are flagged with `NodeFlags::BLOAT_DIR`.

### All-Core `rayon` Search & Grouping (`grouping.rs`)

- **Full-Root Fast Path (`root_id == 0`):** `search_files`, `get_largest_files`, `get_group_summary`, and `get_bloat_dirs` iterate directly over `(0..arena.nodes.len() as u32).into_par_iter()` across all CPU cores with zero tree traversal overhead.
- **Zoomed Subtree Path (`root_id != 0`):** Collects subtree node IDs and automatically switches to `rayon` parallel processing when the subtree exceeds `2,048` nodes.
- **Fast Top-K & Sorting:** Uses `par_sort_unstable_by` and `select_nth_unstable_by` so querying the top 100 largest files out of millions completes in milliseconds.
- **Zero-Allocation ASCII Case-Insensitive Search:** `contains_ignore_case` uses byte-slice `.windows().any(|w| w.eq_ignore_ascii_case(...))` for ASCII filenames and checks ancestor names before falling back to full-path allocation.

---

## 4. Parallel Scanner & Runtime Legacy OS Fallbacks

`crates/fossil-core/src/scanner/` pairs `jwalk` + `rayon` with runtime capability probing in `compat.rs` so modern OSes run at full speed while older OSes and non-native filesystems fall back seamlessly:

### Thread Scaling

- Defaults to `std::thread::available_parallelism()` (all logical CPU cores), falling back safely to `4` on restricted/older environments where `available_parallelism` is unavailable.
- Configurable via `ScanOptions::threads` / CLI `--threads N`. Uses `Parallelism::RayonNewPool(n)` with fallback to `RayonDefaultPool`.

### OS-Specific Fast Paths & Fallbacks

| OS          | Runtime Probe / Fast Path                                                                                                                                                                                                                                                                                     | Automatic Fallback                                                                                                                                                                                                                          |
| :---------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **macOS**   | `probe_macos_attrlist()` calls `libc::getattrlist(..., ATTR_CMN_FLAGS, FSOPT_NOFOLLOW)`. When supported (APFS/HFS+), reads `st_flags & SF_DATALESS` (`0x4000_0000`) to detect iCloud stubs (`disk_size = 0`) without triggering downloads, and skips `/System/Volumes/Data`, `/dev`, `/Volumes` on `/` scans. | On `ENOTSUP` / `EINVAL` / `EOPNOTSUPP` / `ENOSYS` (older macOS, SMB, exFAT, FAT32, FUSE), sets `macos_attrlist_supported = false` and runs `posix-lstat+rayon` cleanly.                                                                     |
| **Linux**   | Calls `libc::syscall(libc::SYS_statx, AT_FDCWD, ..., AT_SYMLINK_NOFOLLOW, STATX_BASIC_STATS, ...)` guarded by `static STATX_SUPPORTED: AtomicBool`. Reads `/proc/self/mountinfo` to skip virtual/pseudo filesystems (`proc`, `sysfs`, `devtmpfs`, `cgroup*`, `tracefs`, etc.).                                | On `ENOSYS` or `EPERM` (kernels `< 4.11` or restrictive seccomp sandboxes), flips `STATX_SUPPORTED = false` permanently and falls back to `symlink_metadata` (`lstat`). Falls back from `/proc/self/mountinfo` to `/proc/mounts` if absent. |
| **Windows** | Uses standard Win32 metadata APIs (`file_attributes()`) to detect cloud placeholders (`FILE_ATTRIBUTE_OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS`) without hard-linking Win11-only DLL exports.                                                                                                       | Uses `windows_cluster_disk_size(logical, 4096)` 4 KiB cluster-boundary rounding fallback for `disk_size`.                                                                                                                                   |

### Edge-Case Protections

- **Hardlink Deduplication:** Tracks `(dev_id, inode)` in an `FxHashSet` when `nlink > 1`; subsequent hardlinks are flagged `HARDLINK_DUP` with `disk_size = 0` to prevent double-counting.
- **Symlink Safety:** Never follows directory symlinks (`follow_links(false)`).
- **Permission Errors:** Logged to `arena.warnings` and flagged on parent directories (`PERMISSION_ERR`) without aborting the scan.
- **Same-Filesystem Guard:** Compares `dev` against `root_dev` on Unix when `same_filesystem = true`.

---

## 5. Rust Squarified Treemap & Raw Binary IPC

To maintain **60–120 FPS** in the desktop UI without shipping thousands of JSON objects across IPC:

1. `crates/fossil-core/src/treemap.rs` computes the nested squarified treemap layout directly in Rust for the current viewport `(width, height)` and zoom root, culling sub-pixel rectangles (`< 2.0 px`) and capping output at `12,000` visible rectangles.
2. Each rectangle is packed into a **32-byte little-endian binary record**:
   - `0..16`: `f32 x, f32 y, f32 w, f32 h`
   - `16..20`: `u32 node_id`
   - `20..28`: `u64 size`
   - `28..32`: `u8 category, u8 depth, u8 kind, u8 flags`
3. Tauri v2 returns `tauri::ipc::Response::new(bytes)` (`ArrayBuffer`), which `src/lib/ipc.ts` decodes via `DataView` with zero JSON parsing.
4. `src/components/TreemapCanvas.svelte` renders directly to a HiDPI HTML5 Canvas 2D context (`alpha: false`), batch-fetches only labels for rectangles large enough to hold text (`get_node_names`), and performs O(N) reverse hit-testing for instant hover tooltips.

---

## 6. MCP Server Architecture (`crates/fossil-mcp`)

- **Transports:**
  - **`stdio`:** Line-delimited **JSON-RPC 2.0** over `stdin`/`stdout` (diagnostic logs strictly to `stderr`). Used by Claude Desktop, Claude Code, Gemini CLI, Antigravity, and Codex CLI.
  - **Streamable HTTP:** `POST /mcp` + Server-Sent Events (`GET /mcp`) over HTTP with Bearer Token auth, DNS rebinding (`Host`) protection, and `Origin` validation. Enables remote and cloud-hosted assistants like **ChatGPT (Web & Desktop)** via secure tunnels.
- **Protocol Compliance:** Supports `initialize` with dynamic version negotiation (`2024-11-05`, `2025-03-26`, `2025-06-18`), `notifications/initialized`, `notifications/cancelled`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `prompts/list`, and `prompts/get`.
- **Tool Annotations:** Every tool specifies `readOnlyHint`, `destructiveHint`, and `openWorldHint` metadata so LLMs understand which tools are read-only inspections versus destructive mutations.
- **Stateful In-Memory Cache:** Holds `RwLock<Option<FileArena>>`. Calling `scan_directory` populates the cache; subsequent `list_children`, `search_files`, `get_largest_files`, `get_group_summary`, `get_bloat_dirs`, `get_cache_status`, and `trash_paths` execute directly against the cached arena.
- **Safety & Sandboxing:**
  - `trash_paths` enforces `fossil_core::ops::is_protected_path()` (protecting system roots and `$HOME`) before calling `trash::delete_all`.
  - Supports `dry_run: true` on `trash_paths` to preview exact items and reclaimable bytes before modifying disk.
  - Optional `--read-only` flag to block mutations entirely.
  - Optional `--allow-dir <PATH>` sandboxing to confine operations to specific canonicalized directory trees.
- **Client Auto-Configuration:** `fossil mcp install <client>` automatically and safely modifies configuration files for `claude-desktop`, `claude-code`, `gemini`, `antigravity`, `codex`, `cursor`, and `vscode`, with backup generation before writing.

---

## 7. Developer Tooling, Pre-Commit Hooks & CI/CD

### Local Tooling

- **Rust:** `rust-toolchain.toml` (stable + `rustfmt` + `clippy`), workspace `[lints.clippy]` (`all`, `perf`), `deny.toml` (`cargo-deny`).
- **Frontend:** `pnpm`, ESLint 9 flat config + `eslint-plugin-svelte`, Prettier + `prettier-plugin-svelte`, `svelte-check`, `Vitest`.
- **Git Hooks (`lefthook.yml`):**
  - `pre-commit`: Staged `cargo fmt --check`, ESLint, Prettier.
  - `commit-msg`: Conventional Commits regex validation.
  - `pre-push`: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm run check`, `pnpm run test`.
- **Task Runner (`justfile`):** `just setup`, `just dev`, `just check`, `just fmt`, `just test`, `just build`, `just build-cli`.

### GitHub Actions Pipelines

- **[`.github/workflows/ci.yml`](./.github/workflows/ci.yml):** Runs on every push/PR across `ubuntu-22.04`, `macos-latest`, and `windows-latest` - frontend lint/typecheck/test/build, Rust `fmt`/`clippy`/`test --workspace`, and `cargo-deny` CVE/license audit.
- **[`.github/workflows/release.yml`](./.github/workflows/release.yml):** Triggered on SemVer tags (`v*.*.*`) - builds and uploads both Tauri desktop installers (`tauri-action`) and standalone `fossil-<target>.tar.gz`/`.zip` CLI/MCP archives across macOS (`aarch64` & `x86_64`), Windows (`x86_64`), and Linux (`x86_64`) to a Draft GitHub Release.
- **[`.github/dependabot.yml`](./.github/dependabot.yml):** Weekly grouped updates for Cargo, npm/pnpm, and GitHub Actions.
