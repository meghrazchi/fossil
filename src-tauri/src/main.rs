// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "mcp" {
        if args.len() > 2 {
            match args[2].as_str() {
                "status" => {
                    let statuses = fossil_mcp::check_status();
                    println!("Fossil MCP Client Configuration Status:\n");
                    for (target, is_configured, path) in statuses {
                        let status_str = if is_configured {
                            "✓ Configured"
                        } else {
                            "✗ Not Configured"
                        };
                        let path_str = path
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "N/A".to_owned());
                        println!(
                            "  {:<24} {:<18} ({path_str})",
                            target.display_name(),
                            status_str
                        );
                    }
                    println!("\nRun 'fossil mcp install <client>' to configure an assistant.");
                    return;
                }
                "install" => {
                    if let Some(client) = args.get(3) {
                        if let Some(target) = fossil_mcp::ClientTarget::parse(client) {
                            match fossil_mcp::install_client(target) {
                                Ok(msg) => println!("{msg}"),
                                Err(err) => eprintln!("Error: {err}"),
                            }
                        } else {
                            eprintln!("Unknown client '{client}'.");
                        }
                    } else {
                        eprintln!("Usage: fossil mcp install <client>");
                    }
                    return;
                }
                "uninstall" => {
                    if let Some(client) = args.get(3) {
                        if let Some(target) = fossil_mcp::ClientTarget::parse(client) {
                            match fossil_mcp::uninstall_client(target) {
                                Ok(msg) => println!("{msg}"),
                                Err(err) => eprintln!("Error: {err}"),
                            }
                        } else {
                            eprintln!("Unknown client '{client}'.");
                        }
                    } else {
                        eprintln!("Usage: fossil mcp uninstall <client>");
                    }
                    return;
                }
                _ => {}
            }
        }

        let read_only = args.iter().any(|a| a == "--read-only");
        let policy = fossil_mcp::ServerSecurityPolicy::new(read_only, vec![]);
        let server = std::sync::Arc::new(fossil_mcp::McpServer::new(policy));
        if let Err(e) = fossil_mcp::run_stdio_server(server) {
            eprintln!("Fossil MCP error: {e}");
            std::process::exit(1);
        }
        return;
    }

    fossil_lib::run();
}
