mod mcp;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use clap::{Parser, Subcommand, ValueEnum};
use fossil_core::arena::SizeMetric;
use fossil_core::classify::FileCategory;
use fossil_core::grouping::{self, GroupBy, NodeItem, SortField};
use fossil_core::ops;
use fossil_core::scanner::{self, ScanOptions};
use fossil_core::volumes;
use mimalloc::MiMalloc;
use serde_json::json;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser)]
#[command(
    name = "fossil",
    version,
    about = "High-performance cross-platform disk usage analyzer CLI & MCP server"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliMetric {
    Disk,
    Logical,
}

impl From<CliMetric> for SizeMetric {
    fn from(m: CliMetric) -> Self {
        match m {
            CliMetric::Disk => Self::Disk,
            CliMetric::Logical => Self::Logical,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliGroupBy {
    Category,
    Extension,
    SizeTier,
    Age,
}

impl From<CliGroupBy> for GroupBy {
    fn from(g: CliGroupBy) -> Self {
        match g {
            CliGroupBy::Category => Self::Category,
            CliGroupBy::Extension => Self::Extension,
            CliGroupBy::SizeTier => Self::SizeTier,
            CliGroupBy::Age => Self::Age,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory and show storage summary, breakdown, and top entries
    Scan {
        path: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long, value_enum, default_value_t = CliGroupBy::Category)]
        group: CliGroupBy,
        #[arg(long, value_enum, default_value_t = CliMetric::Disk)]
        metric: CliMetric,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        same_fs: bool,
        #[arg(long)]
        skip_hidden: bool,
    },
    /// Multi-core search for files matching a filename or path query
    Search {
        path: PathBuf,
        query: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        ext: Option<String>,
        #[arg(long)]
        min_size: Option<u64>,
        #[arg(long, value_enum, default_value_t = CliMetric::Disk)]
        metric: CliMetric,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Find the largest individual files under a path
    Largest {
        path: PathBuf,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        ext: Option<String>,
        #[arg(long)]
        min_age_days: Option<u64>,
        #[arg(long, value_enum, default_value_t = CliMetric::Disk)]
        metric: CliMetric,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Find regeneratable build, dependency, and cache bloat directories
    Bloat {
        path: PathBuf,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = CliMetric::Disk)]
        metric: CliMetric,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// List mounted volumes and quick targets
    Volumes {
        #[arg(long)]
        json: bool,
    },
    /// Move files or directories safely to the OS Trash / Recycle Bin
    Trash {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Reveal a file or folder in the native OS file manager
    Reveal { path: PathBuf },
    /// Start the Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio
    Mcp,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Scan {
            path,
            json: as_json,
            top,
            group,
            metric,
            threads,
            same_fs,
            skip_hidden,
        } => {
            let m: SizeMetric = metric.into();
            let g: GroupBy = group.into();
            let arena = scan_path(path, same_fs, skip_hidden, threads)?;
            let (_, top_items) = grouping::get_children(
                &arena,
                0,
                m,
                match m {
                    SizeMetric::Disk => SortField::DiskSize,
                    SizeMetric::Logical => SortField::LogicalSize,
                },
                true,
                None,
                None,
                0,
                top,
            );
            let groups = grouping::get_group_summary(&arena, 0, g, m);

            if as_json {
                let payload = json!({
                    "root_path": arena.root_path.to_string_lossy(),
                    "total_disk_bytes": arena.nodes[0].disk_size,
                    "total_logical_bytes": arena.nodes[0].logical_size,
                    "total_files": arena.nodes[0].file_count,
                    "total_nodes": arena.nodes.len(),
                    "warnings_count": arena.warnings.len(),
                    "scan_duration_ms": arena.scan_duration_ms,
                    "backend_used": arena.backend_used,
                    "groups": groups,
                    "top_children": top_items
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "Scanned: {} ({} files, {} nodes) in {} ms [{}]",
                    arena.root_path.display(),
                    arena.nodes[0].file_count,
                    arena.nodes.len(),
                    arena.scan_duration_ms,
                    arena.backend_used
                );
                println!(
                    "Total:   {} disk / {} logical",
                    format_bytes(arena.nodes[0].disk_size),
                    format_bytes(arena.nodes[0].logical_size)
                );
                if !arena.warnings.is_empty() {
                    println!("Warnings: {}", arena.warnings.len());
                }
                println!("\nBreakdown:");
                for entry in &groups {
                    let sz = match m {
                        SizeMetric::Disk => entry.disk_size,
                        SizeMetric::Logical => entry.logical_size,
                    };
                    println!(
                        "  {:<24} {:>10}  ({:>5.1}%, {} files)",
                        entry.label,
                        format_bytes(sz),
                        entry.percent,
                        entry.file_count
                    );
                }
                if !top_items.is_empty() {
                    println!("\nTop Entries:");
                    print_items(&top_items, m);
                }
            }
            Ok(())
        }
        Commands::Search {
            path,
            query,
            limit,
            category,
            ext,
            min_size,
            metric,
            threads,
            json: as_json,
        } => {
            let m: SizeMetric = metric.into();
            let cat = parse_category_flag(category.as_deref())?;
            let arena = scan_path(path, true, false, threads)?;
            let (total, items) = grouping::search_files(
                &arena,
                0,
                &query,
                cat,
                ext.as_deref(),
                min_size,
                None,
                m,
                limit,
            );
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "total": total, "items": items }))
                        .map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "Found {total} matching files (showing {}, backend: {}):",
                    items.len(),
                    arena.backend_used
                );
                print_items(&items, m);
            }
            Ok(())
        }
        Commands::Largest {
            path,
            limit,
            category,
            ext,
            min_age_days,
            metric,
            threads,
            json: as_json,
        } => {
            let m: SizeMetric = metric.into();
            let cat = parse_category_flag(category.as_deref())?;
            let arena = scan_path(path, true, false, threads)?;
            let (total, items) = grouping::get_largest_files(
                &arena,
                0,
                m,
                cat,
                ext.as_deref(),
                None,
                min_age_days,
                0,
                limit,
            );
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "total": total, "items": items }))
                        .map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "Largest files ({} of {total} total, backend: {}):",
                    items.len(),
                    arena.backend_used
                );
                print_items(&items, m);
            }
            Ok(())
        }
        Commands::Bloat {
            path,
            limit,
            metric,
            threads,
            json: as_json,
        } => {
            let m: SizeMetric = metric.into();
            let arena = scan_path(path, true, false, threads)?;
            let dirs = grouping::get_bloat_dirs(&arena, 0, m, limit);
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "dirs": dirs }))
                        .map_err(|e| e.to_string())?
                );
            } else if dirs.is_empty() {
                println!("No bloat directories detected.");
            } else {
                println!("Bloat directories ({}):", dirs.len());
                for d in &dirs {
                    let sz = match m {
                        SizeMetric::Disk => d.disk_size,
                        SizeMetric::Logical => d.logical_size,
                    };
                    let reason = d.bloat_reason.as_deref().unwrap_or("");
                    println!("  {:>10}  {}  ({reason})", format_bytes(sz), d.path);
                }
            }
            Ok(())
        }
        Commands::Volumes { json: as_json } => {
            let vols = volumes::list_volumes();
            let targets = volumes::list_quick_targets();
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "volumes": vols,
                        "quick_targets": targets
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else {
                println!("Volumes:");
                for v in &vols {
                    println!(
                        "  {:<18} {:<24} {:>10} used / {:>10} total ({})",
                        v.name,
                        v.mount_point,
                        format_bytes(v.used_bytes),
                        format_bytes(v.total_bytes),
                        v.fs_type
                    );
                }
            }
            Ok(())
        }
        Commands::Trash { paths } => {
            ops::trash_paths(&paths)?;
            println!("Moved {} item(s) to Trash.", paths.len());
            Ok(())
        }
        Commands::Reveal { path } => ops::reveal_in_file_manager(&path),
        Commands::Mcp => mcp::run_stdio_server(),
    }
}

fn scan_path(
    path: PathBuf,
    same_filesystem: bool,
    skip_hidden: bool,
    threads: Option<usize>,
) -> Result<fossil_core::arena::FileArena, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let options = ScanOptions {
        same_filesystem,
        skip_hidden,
        threads,
    };
    scanner::scan_directory(path, &options, &cancel, |_| {})
}

fn parse_category_flag(raw: Option<&str>) -> Result<Option<FileCategory>, String> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    FileCategory::from_key(s)
        .map(Some)
        .ok_or_else(|| format!("Unknown category '{s}'"))
}

fn print_items(items: &[NodeItem], metric: SizeMetric) {
    for item in items {
        let sz = match metric {
            SizeMetric::Disk => item.disk_size,
            SizeMetric::Logical => item.logical_size,
        };
        println!("  {:>10}  {}", format_bytes(sz), item.path);
    }
}

fn format_bytes(bytes: u64) -> String {
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
