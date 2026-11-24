use std::io::{BufRead, Write};
use std::sync::Arc;

use crate::server::McpServer;

/// Runs the line-delimited JSON-RPC 2.0 MCP server over stdio.
pub fn run_stdio_server(server: Arc<McpServer>) -> Result<(), String> {
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
