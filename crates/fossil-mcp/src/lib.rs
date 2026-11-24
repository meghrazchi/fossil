pub mod config;
pub mod protocol;
pub mod server;
pub mod transport;

pub use config::{ClientTarget, check_status, install_client, uninstall_client};
pub use protocol::{
    DEFAULT_PROTOCOL_VERSION, JsonRpcError, JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSIONS,
    negotiate_protocol_version, prompt_definitions, resource_definitions, tool_definitions,
};
pub use server::{McpServer, ServerSecurityPolicy, expand_path, format_bytes};
pub use transport::stdio::run_stdio_server;

#[cfg(feature = "http")]
pub use transport::http::{HttpOptions, generate_auth_token, run_http_server};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_protocol_negotiation() {
        assert_eq!(negotiate_protocol_version(Some("2025-06-18")), "2025-06-18");
        assert_eq!(negotiate_protocol_version(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
        assert_eq!(
            negotiate_protocol_version(Some("unknown-version")),
            DEFAULT_PROTOCOL_VERSION
        );
        assert_eq!(negotiate_protocol_version(None), DEFAULT_PROTOCOL_VERSION);
    }

    #[test]
    fn test_initialize_and_capabilities() {
        let server = McpServer::default();

        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "clientInfo": { "name": "claude", "version": "1.0.0" }
            }
        });

        let resp_raw = server.handle_line(&req.to_string()).expect("response");
        let resp: Value = serde_json::from_str(&resp_raw).expect("valid json");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], "2025-03-26");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert!(resp["result"]["capabilities"]["resources"].is_object());
        assert!(resp["result"]["capabilities"]["prompts"].is_object());
        assert_eq!(resp["result"]["serverInfo"]["name"], "fossil");

        // Notification must not produce a response
        let notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        assert!(server.handle_line(&notif.to_string()).is_none());

        // Ping
        let ping_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        });
        let ping_resp_raw = server
            .handle_line(&ping_req.to_string())
            .expect("ping response");
        let ping_resp: Value = serde_json::from_str(&ping_resp_raw).expect("valid json");
        assert_eq!(ping_resp["id"], 2);
    }

    #[test]
    fn test_tools_resources_and_prompts_list() {
        let server = McpServer::default();

        // Tools list
        let tools_req = json!({ "jsonrpc": "2.0", "id": 10, "method": "tools/list" });
        let tools_resp: Value =
            serde_json::from_str(&server.handle_line(&tools_req.to_string()).unwrap()).unwrap();
        let tools = tools_resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);
        assert!(tools.iter().any(|t| t["name"] == "list_children"));
        assert!(tools.iter().any(|t| t["name"] == "get_cache_status"));

        // Resources list & read
        let res_req = json!({ "jsonrpc": "2.0", "id": 11, "method": "resources/list" });
        let res_resp: Value =
            serde_json::from_str(&server.handle_line(&res_req.to_string()).unwrap()).unwrap();
        let resources = res_resp["result"]["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 2);

        let read_req = json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "resources/read",
            "params": { "uri": "fossil://volumes" }
        });
        let read_resp: Value =
            serde_json::from_str(&server.handle_line(&read_req.to_string()).unwrap()).unwrap();
        assert!(
            read_resp["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("volumes")
        );

        // Prompts list & get
        let prompts_req = json!({ "jsonrpc": "2.0", "id": 13, "method": "prompts/list" });
        let prompts_resp: Value =
            serde_json::from_str(&server.handle_line(&prompts_req.to_string()).unwrap()).unwrap();
        let prompts = prompts_resp["result"]["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 3);

        let get_prompt_req = json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "prompts/get",
            "params": {
                "name": "audit-disk-space",
                "arguments": { "path": "~/Projects" }
            }
        });
        let get_prompt_resp: Value =
            serde_json::from_str(&server.handle_line(&get_prompt_req.to_string()).unwrap())
                .unwrap();
        let messages = get_prompt_resp["result"]["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains("~/Projects")
        );
    }

    #[test]
    fn test_scan_list_children_search_and_dry_run_trash() {
        let dir = tempdir().expect("tempdir");
        let sub1 = dir.path().join("subfolder");
        let sub_nm = dir.path().join("node_modules");
        fs::create_dir_all(&sub1).unwrap();
        fs::create_dir_all(&sub_nm).unwrap();

        let file_a = dir.path().join("video.mp4");
        let file_b = sub1.join("notes.txt");
        let file_c = sub_nm.join("index.js");

        fs::write(&file_a, vec![b'a'; 4096]).unwrap();
        fs::write(&file_b, vec![b'b'; 1024]).unwrap();
        fs::write(&file_c, vec![b'c'; 2048]).unwrap();

        let server = McpServer::default();

        // 1. Scan directory
        let scan_req = json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/call",
            "params": {
                "name": "scan_directory",
                "arguments": {
                    "path": dir.path().to_string_lossy()
                }
            }
        });
        let scan_resp: Value =
            serde_json::from_str(&server.handle_line(&scan_req.to_string()).unwrap()).unwrap();
        assert_eq!(scan_resp["result"]["isError"], false);
        let structured = &scan_resp["result"]["structuredContent"];
        assert_eq!(structured["total_files"], 3);

        // 2. Cache status
        let cache_req = json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "tools/call",
            "params": { "name": "get_cache_status", "arguments": {} }
        });
        let cache_resp: Value =
            serde_json::from_str(&server.handle_line(&cache_req.to_string()).unwrap()).unwrap();
        assert_eq!(cache_resp["result"]["structuredContent"]["cached"], true);

        // 3. List children (drill down)
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 22,
            "method": "tools/call",
            "params": {
                "name": "list_children",
                "arguments": { "limit": 10 }
            }
        });
        let list_resp: Value =
            serde_json::from_str(&server.handle_line(&list_req.to_string()).unwrap()).unwrap();
        assert_eq!(
            list_resp["result"]["structuredContent"]["total_children"],
            3
        );

        // 4. Bloat dirs
        let bloat_req = json!({
            "jsonrpc": "2.0",
            "id": 23,
            "method": "tools/call",
            "params": { "name": "get_bloat_dirs", "arguments": {} }
        });
        let bloat_resp: Value =
            serde_json::from_str(&server.handle_line(&bloat_req.to_string()).unwrap()).unwrap();
        assert_eq!(
            bloat_resp["result"]["structuredContent"]["dirs"][0]["name"],
            "node_modules"
        );

        // 5. Dry-run trash paths
        let dry_req = json!({
            "jsonrpc": "2.0",
            "id": 24,
            "method": "tools/call",
            "params": {
                "name": "trash_paths",
                "arguments": {
                    "paths": [file_a.to_string_lossy()],
                    "dry_run": true
                }
            }
        });
        let dry_resp: Value =
            serde_json::from_str(&server.handle_line(&dry_req.to_string()).unwrap()).unwrap();
        assert_eq!(dry_resp["result"]["isError"], false);
        assert_eq!(dry_resp["result"]["structuredContent"]["dry_run"], true);
        assert!(file_a.exists()); // Dry run did not delete!
    }

    #[test]
    fn test_security_policy_read_only_and_allowed_dirs() {
        let dir = tempdir().expect("tempdir");
        let allowed = dir.path().join("safe_zone");
        let forbidden = dir.path().join("secret_zone");
        fs::create_dir_all(&allowed).unwrap();
        fs::create_dir_all(&forbidden).unwrap();

        let allowed_file = allowed.join("test.txt");
        let forbidden_file = forbidden.join("private.txt");
        fs::write(&allowed_file, "hello").unwrap();
        fs::write(&forbidden_file, "secret").unwrap();

        // 1. Read-only policy
        let ro_policy = ServerSecurityPolicy::new(true, vec![]);
        let ro_server = McpServer::new(ro_policy);

        let trash_req = json!({
            "jsonrpc": "2.0",
            "id": 30,
            "method": "tools/call",
            "params": {
                "name": "trash_paths",
                "arguments": { "paths": [allowed_file.to_string_lossy()] }
            }
        });
        let ro_resp: Value =
            serde_json::from_str(&ro_server.handle_line(&trash_req.to_string()).unwrap()).unwrap();
        assert_eq!(ro_resp["result"]["isError"], true);
        assert!(
            ro_resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("read-only")
        );

        // 2. Allowed directories sandbox policy
        let sandbox_policy = ServerSecurityPolicy::new(false, vec![allowed.clone()]);
        let sandbox_server = McpServer::new(sandbox_policy);

        // Accessing forbidden path must fail
        let forbidden_scan = json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "tools/call",
            "params": {
                "name": "scan_directory",
                "arguments": { "path": forbidden.to_string_lossy() }
            }
        });
        let fb_resp: Value = serde_json::from_str(
            &sandbox_server
                .handle_line(&forbidden_scan.to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fb_resp["result"]["isError"], true);
        assert!(
            fb_resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Access denied")
        );

        // Accessing allowed path must succeed
        let allowed_scan = json!({
            "jsonrpc": "2.0",
            "id": 32,
            "method": "tools/call",
            "params": {
                "name": "scan_directory",
                "arguments": { "path": allowed.to_string_lossy() }
            }
        });
        let ok_resp: Value = serde_json::from_str(
            &sandbox_server
                .handle_line(&allowed_scan.to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(ok_resp["result"]["isError"], false);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_transport_auth_and_dispatch() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let server = Arc::new(McpServer::default());
        let token = "test-secret-token-12345".to_owned();

        // Pick an available local port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let options = HttpOptions {
            bind_addr: format!("127.0.0.1:{port}").parse().unwrap(),
            token: Some(token.clone()),
            allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
            allowed_origins: vec!["*".to_owned()],
        };

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let srv_clone = Arc::clone(&server);

        let handle = std::thread::spawn(move || {
            let _ = run_http_server(srv_clone, options, stop_clone);
        });

        // Give the server a moment to start listening
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 1. Unauthenticated request -> expect 401
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            let req = "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{}";
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            res.push_str(&String::from_utf8_lossy(&buf[..n]));
            assert!(res.contains("401 Unauthorized"));
            assert!(res.contains("WWW-Authenticate: Bearer"));
        }

        // 2. OAuth 2.1 metadata endpoint -> expect 200
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            let req =
                "GET /.well-known/oauth-protected-resource HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            res.push_str(&String::from_utf8_lossy(&buf[..n]));
            assert!(res.contains("200 OK"));
            assert!(res.contains("oauth-protected-resource") || res.contains("resource"));
        }

        // 3. Authenticated JSON-RPC ping request -> expect 200 OK
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            let json_body = r#"{"jsonrpc":"2.0","id":42,"method":"ping"}"#;
            let req = format!(
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            res.push_str(&String::from_utf8_lossy(&buf[..n]));
            assert!(res.contains("200 OK"));
            assert!(res.contains("\"id\":42"));
        }

        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
    }
}
