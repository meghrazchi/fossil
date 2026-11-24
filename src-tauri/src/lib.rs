pub mod commands;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_volumes,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_treemap_layout,
            commands::get_node_details,
            commands::get_node_names,
            commands::get_breadcrumbs,
            commands::get_children,
            commands::get_largest_files,
            commands::get_group_summary,
            commands::get_bloat_dirs,
            commands::get_scan_warnings,
            commands::reveal_in_file_manager,
            commands::trash_nodes,
            commands::delete_nodes_permanently,
            commands::get_mcp_client_statuses,
            commands::install_mcp_client,
            commands::uninstall_mcp_client,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fossil");
}
