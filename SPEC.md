# Fossil - High-Performance Cross-Platform Disk Usage Analyzer & Storage Visualizer

> **Specification & Technical Architecture Document**
> **Target Platforms:** macOS (Apple Silicon & Intel), Windows (x64/ARM64), Linux (x64/ARM64)
> **Core Stack:** Rust + Tauri v2 + TypeScript + Svelte 5 (or React 19) + HTML5 Canvas / WebGL2

---

## 1. Executive Summary & Core Goals

**Fossil** is a native, ultra-fast desktop disk usage analyzer designed to scan entire drives (millions of files) in seconds, map every byte visually by size, intelligently group files for cleanup, and link every visual element 1:1 to real files on disk with safe OS-native actions.

### Core Performance & UX Targets
| Metric | Target |
| :--- | :--- |
| **Scan Speed (NVMe SSD, 1M files)** | < 2–4 seconds (OS-native bulk APIs / MFT) |
| **Memory Footprint (1M files)** | < 80 MB RAM (flat arena + string pool) |
| **Memory Footprint (10M files)** | < 650 MB RAM |
| **Installer Size** | ~6–12 MB (Tauri v2 native webview) |
| **UI Frame Rate (Treemap Pan/Zoom/Hover)** | 60–120 FPS (Rust layout + Canvas/WebGL2) |
| **Delete & Tree Update Latency** | < 10 ms (incremental in-memory delta, no rescan) |

---

## 2. Technology Stack & Speed Accelerators

To make Fossil noticeably faster than traditional analyzers (and orders of magnitude faster than Electron apps), we combine **Rust + Tauri v2** with **OS-level filesystem fast paths**, **cache-friendly memory layouts**, **binary IPC**, and **backend-computed layouts**.

### 2.1 Core Stack
- **Backend / Filesystem Engine:** **Rust** (2024 edition) - zero GC pauses, native SIMD/threading, deterministic memory layout.
- **Desktop Shell:** **Tauri v2** - native OS webview (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux), ~5–10 MB bundle, direct Rust IPC.
- **Frontend UI:** **TypeScript + Svelte 5** (recommended for fine-grained reactivity, zero VDOM overhead, minimal bundle; **React 19 + Vite** is a viable alternative if preferred) + **Tailwind CSS v4**.
- **Virtualized Lists & Tables:** **TanStack Virtual** + **TanStack Table** (windowed rendering backed by Rust pagination/slicing).
- **Treemap Renderer:** **Custom HTML5 Canvas 2D** (with optional **WebGL2** shader pass for 3D cushion shading) driven by a **Rust-side squarified treemap engine** (faster and lighter than ECharts/visx for 10,000+ visible rectangles; ECharts/visx can still be used for secondary summary charts like donuts/bars if desired).

### 2.2 Speed Accelerators (What Makes Fossil Blazingly Fast)

| Layer | Standard Approach | Fossil Accelerated Approach | Why It's Faster |
| :--- | :--- | :--- | :--- |
| **macOS Scanning** | `walkdir` / `jwalk` (`readdir` + `lstat`) | **`getattrlistbulk()`** syscall + `rayon` work-stealing | Fetches directory entries *and* metadata (`size`, `blocks`, `inode`, `nlink`, `mtime`, `type`) in a single batched kernel call - **3–5× faster** on APFS. |
| **Windows Scanning** | `FindFirstFile` + `stat` | **NTFS MFT Direct Read** (`FSCTL_ENUM_USN_DATA` / `usn-journal-rs`) when elevated; fallback to parallel **`FindFirstFileExW`** (`FindExInfoBasic` + `FIND_FIRST_EX_LARGE_FETCH`) | MFT read scans a 1 TB NVMe in **1–3 seconds**. Non-elevated fallback still avoids secondary `stat` calls because `WIN32_FIND_DATAW` includes file size. |
| **Linux Scanning** | `walkdir` | **`jwalk` / `rayon`** + `getdents64` + **`statx`** (requesting only needed fields with `AT_SYMLINK_NOFOLLOW`) | Minimal kernel copy, parallel directory traversal, automatic pseudo-FS skipping via `/proc/self/mountinfo`. |
| **Memory Representation** | `Vec<Box<Node>>` with `PathBuf` per node (~200+ B/node) | **Flat Arena (`Vec<FileNode>`)** with `u32` `NodeId` indices + **Contiguous Name Pool (`Vec<u8>`)** (~40–48 B/node) | **75–85% less RAM**, zero pointer chasing, CPU cache-line friendly. Full `PathBuf` is reconstructed on-demand via `parent` links in nanoseconds. |
| **Allocator** | System `malloc` | **`mimalloc`** (or `tikv-jemallocator` on Linux/macOS) | Significantly lower lock contention across multi-threaded directory walkers. |
| **Hashing / Dedup** | `std::collections::HashSet` (SipHash) | **`rustc-hash` (`FxHashSet`) / `ahash`** for inode & grouping maps; **`phf`** compile-time perfect hash map for file extension → category lookup | O(1) zero-overhead extension classification; 3–5× faster integer hashing for hardlink deduplication. |
| **IPC Transfer** | JSON serialization over `invoke` | **Tauri v2 `Channel<T>`** for streaming progress + **Raw Binary `tauri::ipc::Response` (`Vec<u8>`)** for treemap rects | Eliminates JSON stringify/parse bottlenecks; frontend reads rects directly via `DataView` / `Float32Array` / `Uint32Array`. |
| **Treemap Layout** | JS layout calculation on full tree | **Rust-computed Squarified Treemap** with sub-pixel culling (< 1.5 px) and sibling aggregation | Only visible rectangles (500–5,000 rects) ever cross IPC; recalculating on window resize takes < 2 ms in Rust. |
| **Optional: Fast Cache** | Rescan from scratch every launch | **Optional Snapshot Cache** (serialized with **`rkyv`** zero-copy deserialization) | Instant load of previous scan snapshots; incremental refresh for changed folders. |

---

## 3. Application Look & Feel (UI/UX Specification)

### 3.1 Aesthetic & Visual Direction
- **Theme:** Native-feeling **Dark Mode** (default) and **Light Mode**, following OS appearance with subtle translucent vibrancy on macOS/Windows where appropriate.
- **Typography:** Clean system UI sans-serif (`Inter` / `-apple-system` / `Segoe UI`) for navigation, paired with **tabular monospace** (`JetBrains Mono` / `SF Mono`, `font-variant-numeric: tabular-nums`) for all byte sizes, percentages, and file counts so columns align perfectly.
- **Color Coding by Category (Consistent Everywhere):** Every category has a distinct, accessible accent color shared across the Treemap, Category Bar, Table badges, and Tooltips:
  - **Videos:** Coral / Crimson (`#F43F5E`)
  - **Disk Images & VMs (`.iso`, `.dmg`, `.vmdk`, `.qcow2`):** Purple (`#A855F7`)
  - **Archives (`.zip`, `.tar`, `.7z`, `.zst`):** Amber (`#F59E0B`)
  - **Code & Build Artifacts (`node_modules`, `target`, `.venv`, `DerivedData`):** Cyan (`#06B6D4`)
  - **Images & RAW Photos:** Emerald (`#10B981`)
  - **Audio:** Pink (`#EC4899`)
  - **Applications & Binaries:** Indigo (`#6366F1`)
  - **Documents & PDFs:** Blue (`#3B82F6`)
  - **System, Logs & Caches:** Slate (`#64748B`)
  - **Other / Unknown:** Neutral Gray (`#94A3B8`)

---

### 3.2 Screen 1: Home / Drive & Target Picker
When Fossil launches (or when clicking **New Scan**), users see a clean, instant overview of available volumes and quick targets:
- **Mounted Drives Grid:** Cards for each physical drive / volume showing name, filesystem type (APFS, NTFS, ext4), visual capacity bar (Used / Free / Purgeable), and a one-click **Scan** button.
- **Quick Targets:** Home Directory (`~`), Downloads, Applications, Developer Caches, or **Select Custom Folder...** (drag-and-drop a folder onto the window to start scanning immediately).
- **Recent Scans (Optional Snapshot):** Instant reopen of cached snapshots with timestamp and "Rescan" button.

```text
+-----------------------------------------------------------------------------+
|  FOSSIL                                             [Settings]  [Dark/Light]|
+-----------------------------------------------------------------------------+
|                                                                             |
|   Select a Drive or Folder to Analyze                                       |
|                                                                             |
|   +-------------------------------+   +-------------------------------+     |
|   | [SSD] Macintosh HD (APFS)     |   | [EXT] Samsung T7 (exFAT)      |     |
|   | [################----] 78%    |   | [#####---------------] 24%    |     |
|   | 780 GB used / 1.0 TB          |   | 480 GB used / 2.0 TB          |     |
|   |                     [ Scan > ]|   |                     [ Scan > ]|     |
|   +-------------------------------+   +-------------------------------+     |
|                                                                             |
|   Quick Folders:  [ ~ Home ]  [ Downloads ]  [ Dev & Caches ]  [ + Browse ] |
|                                                                             |
|   Drop any folder here to scan immediately                                  |
+-----------------------------------------------------------------------------+
```

---

### 3.3 Screen 2: Main Explorer Workspace (The Core Experience)
The main workspace uses a **synchronized split layout** so hovering, selecting, or zooming in one view highlights and filters the others in real time.

```text
+-----------------------------------------------------------------------------+
| [<] [>] [^] | / Users / behzad / Projects / fossil       | [Rescan] [Stop]  |
+-----------------------------------------------------------------------------+
| [Search files, ext, path...] [Group: Category v] [Size: Disk Usage v] [?]   |
+-----------------------------------------------------------------------------+
| CATEGORY BREAKDOWN BAR (Interactive Filter - Click any segment to filter)   |
| [==== Videos 38% ====][=== VMs 22% ===][== Build 19% ==][= Arch =][Other]   |
+-----------------------------------------------------------------------------+
| FOLDER TREE (Left Pane)     | ZOOMABLE TREEMAP (Main Canvas - 60-120 FPS)   |
|                             | +-----------------------+-------------------+ |
| v fossil        14.2 GB 100%| | target/ (Build)       | videos/           | |
|   > target       6.8 GB  48%| | 6.8 GB                | 4.1 GB            | |
|   > videos       4.1 GB  29%| | +-----------+-------+ | +---------------+ | |
|   > node_modules 2.1 GB  15%| | | debug/    | deps/ | | | demo_4k.mov   | | |
|   > assets       1.2 GB   8%| | | 4.2 GB    | 2.6GB | | | 2.9 GB        | | |
|                             | | +-----------+-------+ | +---------------+ | |
|                             | +-----------------------+-------------------+ |
|                             | | node_modules/ 2.1 GB  | assets/ 1.2 GB    | |
|                             | +-----------------------+-------------------+ |
+-----------------------------+-----------------------------------------------+
| [ Largest Files ]  [ Grouped by Category ]  [ By Extension ]  [ Heavy Dirs ]|
|-----------------------------------------------------------------------------|
| Name                 | Category | Disk Size | Logical | Modified   | Action |
| demo_4k.mov          | Video    |  2.90 GB  | 2.90 GB | 2026-10-12 | [R][T] |
| ubuntu-24.04.iso     | Disk Img |  2.41 GB  | 2.41 GB | 2026-08-03 | [R][T] |
| libllvm.a            | Binary   |  1.12 GB  | 1.12 GB | 2026-11-20 | [R][T] |
+-----------------------------------------------------------------------------+
| Status: 412,890 files | 14.2 GB scanned in 1.14s | 12 permission warnings   |
+-----------------------------------------------------------------------------+
  [R] = Reveal in Finder/Explorer   [T] = Move to Trash
```

#### Key Interactive Behaviors
1. **Live Progressive Scanning:**
   - As soon as a scan starts, the top bar shows live telemetry (`Files scanned`, `Bytes discovered`, `Elapsed time`, `Files/sec`) and a **Cancel** button.
   - The Treemap and Folder Tree begin populating progressively every ~150 ms so the user sees large folders immediately, never staring at a blank spinner.
2. **Zoomable Canvas Treemap:**
   - **Color Modes:** Switchable between *By Category* (default), *By Depth/Folder*, and *By Age (Heatmap)* (highlights stale multi-year-old giant files).
   - **Visual Style:** Crisp nested headers for top-level folders + optional subtle **3D Cushion Shading** toggle so file boundaries are immediately readable without harsh borders.
   - **Navigation:**
     - **Hover:** Floating tooltip showing exact path, **Disk Usage vs. Logical Size**, % of current folder, % of total drive, category, and last modified date.
     - **Click:** Selects node and highlights it in the Folder Tree & Inspector.
     - **Double-Click (or Enter):** Smoothly zooms into that directory and updates the breadcrumb bar.
     - **Right-Click:** Native-style context menu (*Reveal in Finder/Explorer*, *Copy Full Path*, *Move to Trash*, *Delete Permanently...*, *Exclude from View*, *Properties*).
     - **Backspace / Up Button / Breadcrumb Click:** Zooms back out to any ancestor directory.
3. **Interactive Category Breakdown Bar:**
   - A sleek horizontal stacked bar across the top showing exact proportions by category.
   - Clicking a category (e.g., *Code & Build Artifacts* or *Videos*) **highlights/filters** the Treemap and Bottom Table to show only items in that group.
4. **Bottom Multi-Tab Virtualized Panel (TanStack Virtual + TanStack Table):**
   - **Largest Files:** Flat, sortable list of the biggest individual files within the currently zoomed folder (instant sort by Disk Size, Logical Size, Modified Date, Name, Extension).
   - **Grouped by Category:** Expandable accordion/table grouping all files by category with total size, file count, and top contributors.
   - **By Extension:** Groups by `.mp4`, `.dmg`, `.zip`, `.log`, etc., making it trivial to spot unexpected space hogs.
   - **Heavy Folders / Cleanup Targets:** Highlights known regeneratable bloat (`node_modules`, `target`, `.venv`, `DerivedData`, `Docker.raw`, `.gradle`, `Caches`) with a badge explaining what it is and how much space removing it reclaims.
   - **Stale Large Files:** Files > 100 MB untouched for > 6 months.
5. **Safe Deletion & Cleanup Staging Queue (Optional Toggle):**
   - Users can either **Move to Trash** immediately (single click with undoable OS Trash toast) OR **Mark for Cleanup** (adds files to a staging drawer showing *"Total to reclaim: 18.4 GB (14 items)"*) and confirm all deletions in one batch.
   - **Permanent Delete** is hidden behind a secondary action + explicit confirmation modal displaying the exact full path(s) and total size, with a warning that the action cannot be undone.
6. **Keyboard Shortcuts:**
   - `Cmd/Ctrl + F`: Focus search/filter bar
   - `Backspace` / `Alt + Up`: Go to parent directory
   - `Enter`: Zoom into selected directory
   - `Cmd/Ctrl + R`: Reveal selected item in Finder / Explorer / File Manager
   - `Delete` / `Cmd + Backspace`: Move selected item to Trash
   - `Shift + Delete`: Permanent delete (with confirmation dialog)
   - `Cmd/Ctrl + C`: Copy absolute path

---

## 4. Data Architecture & Grouping Engine (Rust Core)

### 4.1 Compact Arena Memory Model
Instead of heap-allocating a `Node` struct with a `PathBuf` for every file, Fossil stores the entire filesystem tree in a **flat arena**:

```rust
pub type NodeId = u32; // Supports up to 4.29 billion nodes

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    File = 0,
    Directory = 1,
    Symlink = 2,
    Other = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Compact ~48-byte node stored contiguously in Vec<FileNode>
#[repr(C)]
pub struct FileNode {
    pub parent: NodeId,          // u32::MAX for root
    pub first_child: NodeId,     // u32::MAX if leaf/empty
    pub next_sibling: NodeId,    // Linked-list or range index into sorted children
    pub name_offset: u32,        // Byte offset into StringPool
    pub name_len: u16,           // Byte length of basename (UTF-8 / WTF-8)
    pub ext_id: u16,             // Interned extension ID (0 = none)
    pub kind: NodeKind,          // 1 byte
    pub category: FileCategory,  // 1 byte
    pub flags: u16,              // bitflags: IS_HARDLINK_DUP, HAS_PERM_ERR, IS_BLOAT_DIR, IS_HIDDEN
    pub logical_size: u64,       // Aggregated for directories, metadata.len() for files
    pub disk_size: u64,          // Aggregated allocated blocks (st_blocks * 512 / compressed size)
    pub file_count: u32,         // 1 for files, total descendant file count for directories
    pub modified_ts: i64,        // Unix timestamp (seconds)
}
```

#### Why This Layout Excels
- **Exact Real Path Preservation:** Reconstructing the absolute `PathBuf` for any `NodeId` walks `parent` pointers to the root (typically 5–15 hops) and joins slices from `StringPool` onto `root_path: PathBuf`. Zero path duplication in RAM, 100% exact 1:1 mapping to disk.
- **Instant Incremental Deletion:** When a user deletes `NodeId(x)`:
  1. Mark `x` (and its subtree if a directory) as deleted (tombstone flag / unlink from parent).
  2. Walk `parent` chain up to root, subtracting `logical_size`, `disk_size`, and `file_count` from every ancestor.
  3. Update category totals and remove `x` from the Top-N Largest Files index.
  4. Push a lightweight delta event to the UI. Total time: **< 1 ms**, no rescan required.

### 4.2 Grouping & Classification Engine
Classification happens **inline during the scan** (zero extra pass):
1. **Extension Classification (`phf::Map<&'static str, FileCategory>`):**
   - Compile-time perfect hash map maps lowercase extensions in O(1) without heap allocation:
     - `Video`: `mp4, mkv, mov, avi, webm, m4v, wmv, flv, ts, prores`
     - `DiskImage`: `iso, dmg, img, vmdk, vdi, qcow2, vhd, vhdx, ova, raw, sparsebundle`
     - `Archive`: `zip, tar, gz, tgz, bz2, xz, zst, 7z, rar, cab, deb, rpm, pkg`
     - `Image`: `jpg, jpeg, png, webp, heic, gif, tiff, bmp, svg, psd, raw, cr2, cr3, nef, arw, dng`
     - `Audio`: `mp3, flac, wav, aac, m4a, ogg, opus, aiff, alac`
     - `Application`: `app, exe, dll, dylib, so, msi, appimage, apk, ipa, wasm`
     - `Document`: `pdf, doc, docx, xls, xlsx, ppt, pptx, txt, md, csv, json, xml, epub, key, pages`
     - `SystemCache`: `log, tmp, cache, bak, dmp, crash, ds_store, sqlite-wal`
2. **Smart Folder Pattern Detection (Build Artifacts & Bloat):**
   - Known directory names (`node_modules`, `target`, `.venv`, `venv`, `__pycache__`, `DerivedData`, `.gradle`, `.next`, `dist`, `build`, `.cargo`, `Pods`, `vendor`) are tagged with `IS_BLOAT_DIR`.
   - Descendants inside a `IS_BLOAT_DIR` can be rolled up under **Build Artifacts** for cleanup insights while still preserving their true extension when browsing normally.
3. **Multi-Dimensional Grouping Queries (Computed in Rust):**
   - **By Category:** Pre-aggregated per directory (or computed on-the-fly for any zoomed subtree via parallel fold in < 5 ms).
   - **By Extension:** Grouped via `FxHashMap<u16, GroupStats>` (total disk size, logical size, file count).
   - **By Size Tier:** `>= 10 GB`, `1–10 GB`, `100 MB–1 GB`, `10–100 MB`, `< 10 MB`.
   - **By Age:** `> 2 years`, `1–2 years`, `6–12 months`, `1–6 months`, `< 30 days`.
   - **Phase 2 - Exact Duplicate Finder:** Group files by exact `logical_size` (ignoring unique sizes), hash first+last 4 KB with **BLAKE3** (SIMD-accelerated, > 3 GB/s), then full BLAKE3 hash only for collisions.

---

## 5. High-Speed Scanner Pipeline & Edge Cases

### 5.1 Platform-Specific Scanning Backends
```text
                    +-----------------------------+
                    |    Scanner Trait / Engine   |
                    +--------------+--------------+
                                   |
         +-------------------------+-------------------------+
         |                         |                         |
+--------v--------+       +--------v--------+       +--------v--------+
|     macOS       |       |     Windows     |       |      Linux      |
| getattrlistbulk |       | NTFS MFT Direct |       |  jwalk / rayon  |
|  + rayon pool   |       |  (if elevated)  |       | + statx / mount |
| + APFS firmlink |       | Fallback:       |       |   exclusion     |
|   dedup         |       | FindFirstFileExW|       |                 |
+-----------------+       +-----------------+       +-----------------+
```

- **Thread-Local Builders (Zero Lock Contention):** Each worker thread accumulates discovered entries into a thread-local chunk (`Vec<RawEntry>` + local string buffer) and flushes batches to the arena aggregator via `crossbeam-channel`, eliminating mutex contention on the main arena.
- **Cooperative Cancellation:** An `Arc<AtomicBool>` cancel token is checked per directory so clicking **Stop** aborts immediately and cleanly frees partial state.

### 5.2 Critical Edge Cases Handled From Day One
1. **Permission Errors (Access Denied):**
   - Protected OS folders (e.g., macOS TCC-protected `/Library/Application Support`, SIP paths, Windows `System Volume Information`, Linux `/root`) never crash or abort a scan.
   - Increments a `permission_warnings` counter, logs the skipped path to a dedicated Warnings drawer, and flags the parent node so the UI can show a subtle amber indicator (and on macOS, offer a button to open *System Settings → Privacy & Security → Full Disk Access*).
2. **Symlinks & Loops:**
   - Never follow directory symlinks by default (`AT_SYMLINK_NOFOLLOW`). Display symlinks as tiny leaf entries with a symlink badge.
   - Guard against bind-mount cycles by tracking visited `(device_id, inode)` pairs for directories.
3. **Virtual / Pseudo Filesystems & Network Mounts:**
   - **Linux:** Parse `/proc/self/mountinfo` before scanning; exclude `proc`, `sysfs`, `devtmpfs`, `devpts`, `tmpfs`, `cgroup*`, `pstore`, `debugfs`, `tracefs`, `fuse.gvfsd-fuse`, and stay on the same filesystem (`st_dev` check) when scanning a single volume unless the user explicitly enables cross-mount scanning.
   - **macOS:** Handle APFS **firmlinks** (`/System/Volumes/Data` mounted alongside `/`) so scanning `/` does not double-count user files. Exclude `/dev`, `/Volumes/*` (when scanning root), and Time Machine local snapshot mounts.
   - **Windows:** Skip reparse points / junctions (`FILE_ATTRIBUTE_REPARSE_POINT`) by default to prevent infinite loops (e.g., `C:\Users\All Users` → `C:\ProgramData`).
   - **Cloud Stubs (iCloud Drive, OneDrive, Dropbox Smart Sync):** Detect sparse / dataless / cloud-only files (`SF_DATALESS` on macOS, `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `FILE_ATTRIBUTE_OFFLINE` on Windows) so Fossil **never triggers accidental cloud downloads** and accurately reports `disk_size = 0 B` vs. `logical_size = X MB`.
4. **Hardlinks:**
   - If `nlink > 1`, insert `(dev_id, inode)` into a deduplication `FxHashSet`. Count the first occurrence toward `disk_size` and mark subsequent hardlinks with `IS_HARDLINK_DUP` (shown in tooltips as *"Hardlink - already counted elsewhere"*).
5. **Logical Size vs. Allocated Disk Usage:**
   - Always store **both** `logical_size` (`st_size`) and `disk_size` (`st_blocks * 512` on Unix; `GetCompressedFileSizeW` / cluster-rounded size on Windows).
   - Default view uses **Disk Usage** (actual space consumed on drive, accurate for sparse files, APFS/NTFS compression, and cloud placeholders), with a 1-click toolbar toggle to **Logical Size**. Tooltips always display both clearly.

---

## 6. OS Integration & Safe File Operations

### 6.1 Tauri v2 Command & Event API

| Command / Channel | Direction | Description |
| :--- | :--- | :--- |
| `list_volumes()` | UI → Rust | Returns mounted drives, mount points, FS type, total/used/free bytes. |
| `start_scan(path, options, on_progress: Channel<ScanProgress>)` | UI → Rust | Starts background scan; streams progress stats + periodic root updates via `Channel`. |
| `cancel_scan(scan_id)` | UI → Rust | Sets atomic cancel flag to stop active scan immediately. |
| `get_treemap_layout(root_id, width, height, metric, filter)` | UI → Rust | **Returns Binary `Vec<u8>`** of packed squarified rects `(x, y, w, h, node_id, category, depth, flags)`. |
| `get_node_details(node_id)` | UI → Rust | Returns full reconstructed path, sizes, timestamps, category, hardlink/symlink metadata. |
| `get_children(parent_id, offset, limit, sort_by, filter)` | UI → Rust | Paginated/windowed children for the virtualized Folder Tree. |
| `get_largest_files(root_id, offset, limit, filter)` | UI → Rust | Windowed slice of largest individual files under `root_id`. |
| `get_group_summary(root_id, group_by)` | UI → Rust | Returns aggregated sizes & counts grouped by Category, Extension, Size Tier, Age, or Bloat Dirs. |
| `reveal_in_file_manager(node_id)` | UI → Rust | Opens OS file manager and selects the exact file (`open -R` on macOS, `explorer /select,` on Windows, DBus `org.freedesktop.FileManager1.ShowItems` with `xdg-open` parent fallback on Linux). |
| `trash_nodes(node_ids: Vec<u32>)` | UI → Rust | Moves real files to OS Trash / Recycle Bin via `trash` crate, then updates Rust arena incrementally and returns updated ancestor deltas. |
| `delete_nodes_permanently(node_ids: Vec<u32>)` | UI → Rust | Explicit secondary permanent delete (`fs::remove_file` / `fs::remove_dir_all`), with safety guard preventing root/system directory deletion, plus incremental arena update. |
| `get_scan_warnings(offset, limit)` | UI → Rust | Returns list of permission-denied / skipped paths from the last scan. |

### 6.2 Safety Guardrails for Deletion
- **Trash-First Default:** Every primary delete action calls `trash::delete_all(&paths)` so files are recoverable from macOS Trash, Windows Recycle Bin, or Linux Freedesktop Trash.
- **Protected Path Guard:** Hard-block deletion of mount roots (`/`, `C:\`, `/Users`, `/System`, `C:\Windows`, etc.) in the Rust backend regardless of UI state.
- **Atomic Verification:** Before deleting, verify the reconstructed `PathBuf` still exists and matches the expected kind (file vs. directory). If a partial batch fails (e.g., file in use), report per-item errors and only subtract successfully deleted items from the arena.

---

## 7. Recommended Crate & Package Dependencies

### Rust (`src-tauri/Cargo.toml`)
- **Framework:** `tauri = "2"`, `tauri-plugin-dialog`, `tauri-plugin-opener`, `tauri-plugin-os`
- **Concurrency & Scanning:** `rayon`, `crossbeam-channel`, `jwalk` (fallback / Linux), `libc` (macOS `getattrlistbulk` & Linux `statx`), `windows-sys` (Windows `FindFirstFileExW` / NTFS MFT / `GetCompressedFileSizeW`)
- **Performance & Memory:** `mimalloc`, `rustc-hash` (or `ahash`), `phf` (compile-time extension map), `bitflags`, `rkyv` (optional zero-copy snapshot cache), `blake3` (Phase 2 duplicate detection)
- **OS Actions & Volumes:** `trash`, `sysinfo` (for drive/volume listing), `serde` (only for small metadata structs; raw binary for treemap)

### Frontend (`package.json`)
- **Framework:** `svelte` (v5) + `@sveltejs/vite-plugin-svelte` + `vite` + `typescript` *(or `react` v19)*
- **Styling:** `tailwindcss` (v4), `lucide-svelte` (or `lucide-react`) for crisp icons
- **Virtualized Lists/Tables:** `@tanstack/svelte-virtual` + `@tanstack/svelte-table`
- **Tauri Bindings:** `@tauri-apps/api` (v2)

---

## 8. Developer Tooling, Linting, Pre-Commit Hooks & CI/CD

Fossil is set up from day one with strict automated quality gates, fast local developer workflows, and a full cross-platform CI/CD pipeline that builds native binaries and installers for macOS, Windows, and Linux.

### 8.1 Local Development & Code Quality Stack

| Area | Tooling | Purpose & Configuration |
| :--- | :--- | :--- |
| **Package Manager** | **pnpm** (Corepack pinned) | Fast, deterministic frontend dependency management with strict lockfiles (`pnpm-lock.yaml`). |
| **Rust Toolchain Pin** | `rust-toolchain.toml` | Pins stable Rust channel + `clippy`, `rustfmt`, and cross-compilation targets so local and CI builds match. |
| **Rust Linting & Format** | **`rustfmt`** + **`cargo clippy`** | Configured in `src-tauri/Cargo.toml` `[lints.clippy]` with `pedantic`, `perf`, `nursery` (selective), and `-D warnings` in CI. |
| **Rust Security & Licenses** | **`cargo-deny`** (`deny.toml`) | Audits dependencies for known CVEs (RustSec), banned crates, duplicate versions, and license compliance. |
| **Rust Testing & Benchmarks** | **`cargo-nextest`** + **`criterion`** | Parallel unit/integration test execution (`nextest`) + microbenchmarks for arena, scanner throughput, and treemap layout speed. |
| **Frontend Linting & Format** | **ESLint (flat config) + Prettier** (`eslint-plugin-svelte`, `prettier-plugin-svelte`, `typescript-eslint`) *(or **Biome** if using React)* | Enforces TypeScript strict mode, Svelte 5 rune rules, and consistent formatting. |
| **Frontend Typecheck & Unit Tests** | **`svelte-check`** + **`tsc --noEmit`** + **Vitest** | Catches template/type errors and tests binary IPC decoding, byte formatting, and UI state logic. |
| **Git Pre-Commit / Pre-Push Hooks** | **`lefthook`** (`lefthook.yml`) | Single-binary, parallel, zero-overhead Git hook runner for polyglot Rust + TS repos. |
| **Unified Task Runner** | **`just`** (`justfile`) | One-command developer shortcuts: `just dev`, `just check`, `just lint`, `just fmt`, `just test`, `just bench`, `just build`. |
| **Editor Consistency** | `.editorconfig` + `.vscode/extensions.json` | Standardizes indentation, line endings, and recommends `rust-analyzer`, `Svelte`, `Tailwind CSS`, and `Tauri` extensions. |

### 8.2 Git Hooks (`lefthook.yml`)
- **`pre-commit` (parallel, staged files only - finishes in < 2s):**
  - `rustfmt`: Runs `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` when `.rs` files are staged.
  - `frontend-lint`: Runs Prettier check + ESLint on staged `.ts`, `.svelte`, `.js`, `.css` files.
  - `toml-json-check`: Validates `Cargo.toml`, `tauri.conf.json`, and `package.json` syntax.
- **`commit-msg`:**
  - Enforces **Conventional Commits** (`feat:`, `fix:`, `perf:`, `refactor:`, `ci:`, `docs:`, `chore:`) so changelogs and semantic releases can be auto-generated.
- **`pre-push`:**
  - Runs `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `pnpm run check` (`svelte-check` / `tsc --noEmit`) before code reaches the remote.

### 8.3 CI/CD Pipeline (GitHub Actions)

Three focused workflows under `.github/workflows/`:

#### 1. Continuous Integration - `.github/workflows/ci.yml`
Triggered on every **Push to `main`** and every **Pull Request**:
- **Frontend Quality Job (`ubuntu-latest`):**
  - `pnpm install --frozen-lockfile` (with pnpm store cache)
  - `pnpm run lint` (ESLint + Prettier check)
  - `pnpm run check` (`svelte-check` + `tsc --noEmit`)
  - `pnpm run test` (Vitest unit tests)
  - `pnpm run build` (Vite production bundle check)
- **Rust Quality & Security Job (Matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`):**
  - Uses `Swatinem/rust-cache@v2` for fast incremental compilation across runs.
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run` (runs platform-specific scanner tests on all 3 OSes!)
  - `cargo deny check` (runs on Linux to verify RustSec advisories & licenses)
- **Tauri Smoke Build (PR verification):**
  - Verifies the full Tauri desktop binary compiles cleanly on all 3 platforms (debug or `--no-bundle` on PRs to save CI minutes).

#### 2. Cross-Platform Release & Binary Builder - `.github/workflows/release.yml`
Triggered on **SemVer Git tags (`v*.*.*`)** or manual `workflow_dispatch`:
- Uses **`tauri-apps/tauri-action@v0`** across a native multi-OS build matrix to produce ready-to-install binaries for every target platform:

| Matrix Runner | Target Triple | Output Artifacts |
| :--- | :--- | :--- |
| `macos-latest` (Apple Silicon) | `aarch64-apple-darwin` | `Fossil_<ver>_aarch64.dmg`, `.app.tar.gz` |
| `macos-latest` (Intel cross-compile) | `x86_64-apple-darwin` | `Fossil_<ver>_x64.dmg`, `.app.tar.gz` *(or `universal-apple-darwin` combined `.dmg`)* |
| `windows-latest` | `x86_64-pc-windows-msvc` | `Fossil_<ver>_x64-setup.exe` (NSIS), `Fossil_<ver>_x64_en-US.msi` |
| `windows-11-arm` *(optional)* | `aarch64-pc-windows-msvc` | `Fossil_<ver>_arm64-setup.exe` |
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `Fossil_<ver>_amd64.AppImage`, `Fossil_<ver>_amd64.deb`, `Fossil_<ver>-1.x86_64.rpm` |
| `ubuntu-22.04-arm` *(optional)* | `aarch64-unknown-linux-gnu` | `Fossil_<ver>_arm64.AppImage`, `Fossil_<ver>_arm64.deb` |

- **Post-Build Release Steps:**
  - Generates `SHA256SUMS.txt` checksums for every produced binary/installer.
  - Creates a **Draft GitHub Release** with auto-generated release notes (grouped by Conventional Commits) and attaches all OS installers + checksums.
  - **Code Signing & Notarization Ready:** Pre-wired environment secret hooks (gracefully skipped when secrets aren't set yet, enabled automatically once keys are added):
    - **macOS:** `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` (Developer ID signing + Apple Notarization).
    - **Windows:** `TAURI_SIGNING_PRIVATE_KEY` (for Tauri updater) and optional EV/Azure Trusted Signing cert.
    - **Tauri Updater:** Optional `latest.json` manifest generation for built-in in-app auto-updates.

#### 3. Dependency Updates - `.github/dependabot.yml`
- Weekly grouped PRs for **Cargo** (`src-tauri`), **npm/pnpm**, and **GitHub Actions** versions, automatically verified by `ci.yml`.

---

## 9. Proposed Project Structure

```text
fossil/
├── SPEC.md                          # This architecture & design specification
├── .github/
│   ├── dependabot.yml               # Automated Cargo, pnpm & Actions updates
│   └── workflows/
│       ├── ci.yml                   # Lint, typecheck, clippy, cargo-deny, multi-OS tests
│       └── release.yml              # Multi-OS binary/installer builds & GitHub Releases
├── .vscode/
│   ├── extensions.json              # Recommended IDE extensions
│   └── settings.json                # Format-on-save, rust-analyzer & Svelte settings
├── .editorconfig                    # Cross-editor formatting defaults
├── lefthook.yml                     # Fast pre-commit, commit-msg & pre-push Git hooks
├── justfile                         # Unified local dev/lint/test/build commands
├── rust-toolchain.toml              # Pinned Rust toolchain & clippy/rustfmt components
├── deny.toml                        # cargo-deny security & license policy
├── eslint.config.js                 # Frontend ESLint flat config
├── .prettierrc                      # Prettier formatting config
├── package.json
├── pnpm-lock.yaml
├── vite.config.ts
├── src/                             # Frontend (TypeScript + Svelte 5)
│   ├── main.ts
│   ├── App.svelte
│   ├── lib/
│   │   ├── ipc.ts                   # Typed Tauri v2 invoke, Channel, and binary decoder
│   │   ├── state/                   # Reactive app state (active scan, zoom stack, filters)
│   │   ├── colors.ts                # Category color palette & cushion shading helpers
│   │   └── format.ts                # Byte formatting (SI/IEC), relative dates, percentages
│   └── components/
│       ├── DrivePicker.svelte       # Home screen with volume cards & drag-drop target
│       ├── TopBar.svelte            # Navigation history, breadcrumbs, scan progress
│       ├── CategoryBar.svelte       # Interactive stacked horizontal breakdown bar
│       ├── TreemapCanvas.svelte     # Custom HiDPI Canvas 2D / WebGL2 treemap + hit-testing
│       ├── FolderTree.svelte        # Virtualized collapsible tree (TanStack Virtual)
│       ├── BottomInspector.svelte   # Tabbed Largest Files / Groups / Extensions / Bloat
│       ├── ContextMenu.svelte       # Right-click OS actions menu
│       └── DeleteConfirmModal.svelte# Safe confirmation modal for permanent delete / batch
└── src-tauri/                       # Rust Backend
    ├── Cargo.toml                   # Workspace deps, [lints.clippy], release LTO profile
    ├── tauri.conf.json              # App metadata, bundle targets (.dmg/.msi/.exe/.deb/.AppImage)
    ├── benches/                     # Criterion benchmarks (scanner, arena, treemap)
    └── src/
        ├── main.rs
        ├── lib.rs                   # Tauri v2 builder, state setup, mimalloc global allocator
        ├── commands.rs              # Tauri IPC commands & binary Response encoders
        ├── arena.rs                 # Flat FileNode arena, StringPool, PathBuf reconstruction
        ├── classify.rs              # phf extension map + bloat folder heuristics
        ├── treemap.rs               # Rust squarified treemap layout + sub-pixel culling
        ├── grouping.rs              # Category, extension, size-tier, and age aggregations
        ├── ops.rs                   # Reveal in file manager, trash, permanent delete + delta
        ├── volumes.rs               # Cross-platform volume & mount discovery
        └── scanner/
            ├── mod.rs               # Unified scanner pipeline, cancel token, progress channel
            ├── macos.rs             # getattrlistbulk + APFS firmlink/dataless handling
            ├── windows.rs           # NTFS MFT reader + FindFirstFileExW fallback
            └── linux.rs             # jwalk/statx + /proc/self/mountinfo exclusions
```

---

## 10. Phased Implementation Roadmap

### Phase 1 - Project Scaffold, Dev Tooling, CI/CD & Core Engine
- Initialize Git repo, scaffold Tauri v2 + TypeScript + Svelte 5 + Tailwind v4 with `pnpm` and `mimalloc`.
- Configure full dev tooling from scratch: `rust-toolchain.toml`, strict `clippy`/`rustfmt`, `deny.toml`, ESLint + Prettier + `svelte-check`, `lefthook.yml` pre-commit/pre-push hooks, `justfile`, and `.editorconfig`.
- Set up GitHub Actions from scratch: `ci.yml` (multi-OS lint/test/security checks) and `release.yml` (multi-OS binary builds for macOS `.dmg`, Windows `.exe`/`.msi`, Linux `.AppImage`/`.deb`/`.rpm` + `SHA256SUMS.txt`).
- Implement `FileNode` flat arena, `StringPool`, `PathBuf` reconstruction, and `classify.rs` (`phf` extension map) with unit tests and Criterion benchmarks.
- Build cross-platform parallel scanner (`jwalk` + `rayon` baseline first, then plug in macOS `getattrlistbulk` and Windows `FindFirstFileExW`/MFT fast paths).
- Implement volume listing (`list_volumes`), progress streaming via Tauri v2 `Channel`, and edge-case protections (symlinks, virtual FS, permission warnings, hardlinks).

### Phase 2 - Rust Treemap Engine & Interactive Explorer UI
- Implement squarified treemap layout in Rust with binary IPC (`Vec<u8>` → `Float32Array`/`Uint32Array`).
- Build `TreemapCanvas.svelte` with HiDPI Canvas 2D rendering, spatial hit-testing, hover tooltips, zoom/breadcrumb stack, and category color coding.
- Build synchronized `FolderTree.svelte` and `BottomInspector.svelte` (Largest Files, Grouped by Category, By Extension, Heavy Bloat Dirs) using TanStack Virtual + TanStack Table.
- Add `CategoryBar.svelte` interactive filtering and Disk Usage vs. Logical Size toggle.

### Phase 3 - OS Actions, Safe Deletion & Incremental Updates
- Implement `reveal_in_file_manager`, `trash_nodes` (`trash` crate), and `delete_nodes_permanently` with protected-path guards.
- Wire incremental arena subtraction so deletions update the Treemap, Tree, Category Bar, and Tables in < 10 ms without rescanning.
- Add Context Menu, keyboard shortcuts, and Cleanup Staging Queue.

### Phase 4 - Polish, Native Fast Paths & Signed Releases
- Finalize macOS `getattrlistbulk` + APFS firmlink/cloud-dataless handling and Windows NTFS MFT elevated fast path.
- Add optional 3D cushion shading shader toggle on the treemap.
- Add optional `rkyv` scan snapshot caching and Phase 2 BLAKE3 Duplicate File Finder.
## 11. CLI, MCP Server, Full-Core Parallel Search & Legacy OS Compatibility

To make Fossil usable from terminals, scripts, and AI agents (via Model Context Protocol) without requiring a GUI, the Rust backend is organized as a **Cargo Workspace**:

- **`crates/fossil-core`**: Pure Rust library (no GUI/WebKit dependencies) containing the flat `FileArena`, `phf` classifier, multi-core parallel scanner, multi-core `rayon` search/grouping engine, squarified treemap generator, volume discovery, and safe OS operations.
- **`crates/fossil-cli`** (`fossil` binary): Lightweight standalone CLI + stdio **MCP server** (`fossil mcp`) sharing `fossil-core`.
  - **CLI Commands:** `fossil scan`, `fossil largest`, `fossil bloat`, `fossil volumes`, `fossil trash`, `fossil reveal`, `fossil mcp`.
  - **MCP Tools (JSON-RPC 2.0 over stdio):** `scan_directory`, `get_largest_files`, `get_group_summary`, `get_bloat_dirs`, `list_volumes`, `trash_paths`, `reveal_in_file_manager`.
- **`src-tauri`**: Tauri v2 desktop app wrapping `fossil-core`.

### Multi-Core Parallel Search & Older OS Compatibility (Zero-Overhead Fast Paths)
- **All-Core Parallelism:** Both directory scanning and file search/grouping (`get_largest_files`, `get_group_summary`, `get_bloat_dirs`) use `rayon` across all logical CPU cores (`std::thread::available_parallelism()`), configurable via `--threads`.
- **Runtime Capability Probing & Graceful Fallbacks (No Slowdown on Modern OSes):**
  - **macOS:** Uses batched `getattrlistbulk()` syscalls with Rayon work-stealing on APFS/HFS+; automatically falls back per-directory to `readdir`/`lstat` when `ENOTSUP`/`EINVAL` occurs (older macOS versions, SMB, exFAT, FAT32, FUSE).
  - **Linux:** Runtime-probes `statx` syscall; if an older kernel (< 4.11) or container seccomp policy returns `ENOSYS`/`EPERM`, an atomic flag flips permanently to classic `lstat` so subsequent calls incur zero failed-syscall overhead. Mount discovery falls back from `/proc/self/mountinfo` to `/proc/mounts`.
  - **Windows:** Uses standard Win32 parallel traversal (`jwalk` + `rayon`) without Win11-only hard dependencies, with safe cluster-size estimation fallback when compressed-size APIs are unavailable.

