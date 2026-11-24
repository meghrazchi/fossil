use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::server::McpServer;

#[derive(Debug, Clone)]
pub struct HttpOptions {
    pub bind_addr: SocketAddr,
    pub token: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:7742".parse().unwrap(),
            token: None,
            allowed_hosts: vec![
                "127.0.0.1".to_owned(),
                "localhost".to_owned(),
                "::1".to_owned(),
            ],
            allowed_origins: Vec::new(),
        }
    }
}

/// Generates a secure random 32-character hex token using `fastrand`.
#[must_use]
pub fn generate_auth_token() -> String {
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = fastrand::u8(..);
    }
    let mut hex = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Runs the Streamable HTTP MCP server on the configured address.
pub fn run_http_server(
    server: Arc<McpServer>,
    options: HttpOptions,
    stop_signal: Arc<AtomicBool>,
) -> Result<(), String> {
    let http_server = Server::http(options.bind_addr)
        .map_err(|e| format!("Failed to bind HTTP server to {}: {e}", options.bind_addr))?;

    let effective_token = options.token.clone().unwrap_or_else(generate_auth_token);

    eprintln!(
        "Fossil MCP HTTP server listening on http://{}",
        options.bind_addr
    );
    eprintln!("Authentication token: {effective_token}");
    eprintln!("MCP Endpoint: http://{}/mcp", options.bind_addr);

    while !stop_signal.load(Ordering::Relaxed) {
        let req = match http_server.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                }
                eprintln!("HTTP request receive error: {e}");
                continue;
            }
        };

        handle_request(req, &server, &options, &effective_token);
    }

    Ok(())
}

fn handle_request(
    mut req: tiny_http::Request,
    server: &Arc<McpServer>,
    options: &HttpOptions,
    expected_token: &str,
) {
    let url_raw = req.url();
    let url = url_raw.split('?').next().unwrap_or(url_raw);

    // Host header validation (DNS rebinding guard)
    let host_header = req
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("host"))
        .map(|h| h.value.as_str())
        .unwrap_or("");

    let host_name = host_header.split(':').next().unwrap_or("");
    let host_ok = options
        .allowed_hosts
        .iter()
        .any(|allowed| host_name.eq_ignore_ascii_case(allowed) || allowed == "*");

    if !host_ok && !host_header.is_empty() {
        let resp = Response::from_string("Forbidden: Invalid Host header")
            .with_status_code(StatusCode(403));
        let _ = req.respond(resp);
        return;
    }

    // Origin header validation if present
    if let Some(origin_header) = req
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("origin"))
        .map(|h| h.value.as_str())
    {
        let origin_ok = origin_header == "null"
            || origin_header.starts_with("http://localhost")
            || origin_header.starts_with("http://127.0.0.1")
            || options.allowed_origins.iter().any(|allowed| {
                origin_header == allowed || origin_header.ends_with(allowed) || allowed == "*"
            });

        if !origin_ok && !options.allowed_origins.is_empty() {
            let resp = Response::from_string("Forbidden: Invalid Origin header")
                .with_status_code(StatusCode(403));
            let _ = req.respond(resp);
            return;
        }
    }

    // Handle CORS preflight OPTIONS
    if req.method() == &Method::Options {
        let resp = Response::empty(204)
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header(
                "Access-Control-Allow-Methods",
                "GET, POST, DELETE, OPTIONS",
            ))
            .with_header(header(
                "Access-Control-Allow-Headers",
                "Authorization, Content-Type, Mcp-Session-Id",
            ))
            .with_header(header("Access-Control-Expose-Headers", "Mcp-Session-Id"));
        let _ = req.respond(resp);
        return;
    }

    // RFC 9728 OAuth 2.1 protected resource metadata discovery
    if url == "/.well-known/oauth-protected-resource" && req.method() == &Method::Get {
        let metadata = json!({
            "resource": format!("http://{}/mcp", options.bind_addr),
            "authorization_servers": [],
            "bearer_methods_supported": ["header"],
            "resource_documentation": "https://github.com/meghrazchi/fossil"
        });
        let body = serde_json::to_string_pretty(&metadata).unwrap_or_default();
        let resp = Response::from_string(body)
            .with_status_code(StatusCode(200))
            .with_header(header("Content-Type", "application/json"))
            .with_header(header("Access-Control-Allow-Origin", "*"));
        let _ = req.respond(resp);
        return;
    }

    // MCP endpoint routing
    if url == "/mcp" {
        // Bearer Token Authentication
        let auth_header = req
            .headers()
            .iter()
            .find(|h| {
                h.field
                    .as_str()
                    .as_str()
                    .eq_ignore_ascii_case("authorization")
            })
            .map(|h| h.value.as_str())
            .unwrap_or("");

        let token_valid = if let Some(stripped) = auth_header.strip_prefix("Bearer ") {
            stripped.trim() == expected_token
        } else {
            false
        };

        if !token_valid {
            let err_body = json!({
                "error": "Unauthorized",
                "message": "Valid Bearer token required in Authorization header"
            });
            let resp = Response::from_string(err_body.to_string())
                .with_status_code(StatusCode(401))
                .with_header(header("WWW-Authenticate", "Bearer"))
                .with_header(header("Content-Type", "application/json"))
                .with_header(header("Access-Control-Allow-Origin", "*"));
            let _ = req.respond(resp);
            return;
        }

        let session_id = req
            .headers()
            .iter()
            .find(|h| {
                h.field
                    .as_str()
                    .as_str()
                    .eq_ignore_ascii_case("mcp-session-id")
            })
            .map(|h| h.value.as_str().to_owned())
            .unwrap_or_else(generate_auth_token);

        match *req.method() {
            Method::Post => {
                let mut content = String::new();
                if let Err(e) = req.as_reader().read_to_string(&mut content) {
                    let resp = Response::from_string(format!("Failed to read request body: {e}"))
                        .with_status_code(StatusCode(400));
                    let _ = req.respond(resp);
                    return;
                }

                if let Some(reply_line) = server.handle_line(&content) {
                    let resp = Response::from_string(reply_line)
                        .with_status_code(StatusCode(200))
                        .with_header(header("Content-Type", "application/json"))
                        .with_header(header("Mcp-Session-Id", &session_id))
                        .with_header(header("Access-Control-Allow-Origin", "*"));
                    let _ = req.respond(resp);
                } else {
                    // Notification acknowledged without response payload
                    let resp = Response::empty(202)
                        .with_header(header("Mcp-Session-Id", &session_id))
                        .with_header(header("Access-Control-Allow-Origin", "*"));
                    let _ = req.respond(resp);
                }
            }
            Method::Get => {
                // SSE stream handshake
                let initial_event = "event: endpoint\ndata: /mcp\n\n";
                let resp = Response::from_string(initial_event)
                    .with_status_code(StatusCode(200))
                    .with_header(header("Content-Type", "text/event-stream"))
                    .with_header(header("Cache-Control", "no-cache"))
                    .with_header(header("Connection", "keep-alive"))
                    .with_header(header("Mcp-Session-Id", &session_id))
                    .with_header(header("Access-Control-Allow-Origin", "*"));
                let _ = req.respond(resp);
            }
            Method::Delete => {
                // Close session
                let resp =
                    Response::empty(204).with_header(header("Access-Control-Allow-Origin", "*"));
                let _ = req.respond(resp);
            }
            _ => {
                let resp = Response::from_string("Method Not Allowed")
                    .with_status_code(StatusCode(405))
                    .with_header(header("Allow", "GET, POST, DELETE, OPTIONS"));
                let _ = req.respond(resp);
            }
        }
        return;
    }

    // Default 404
    let resp = Response::from_string("Not Found").with_status_code(StatusCode(404));
    let _ = req.respond(resp);
}

fn header(name: &str, val: &str) -> Header {
    Header::from_bytes(name.as_bytes(), val.as_bytes()).unwrap()
}
