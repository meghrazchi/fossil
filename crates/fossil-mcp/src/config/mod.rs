use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTarget {
    ClaudeDesktop,
    ClaudeCode,
    Gemini,
    Antigravity,
    Codex,
    Cursor,
    VSCode,
    ChatGPT,
}

impl ClientTarget {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace('_', "-").as_str() {
            "claude-desktop" | "claude" => Some(Self::ClaudeDesktop),
            "claude-code" => Some(Self::ClaudeCode),
            "gemini" => Some(Self::Gemini),
            "antigravity" | "agy" => Some(Self::Antigravity),
            "codex" | "openai-codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "vscode" => Some(Self::VSCode),
            "chatgpt" => Some(Self::ChatGPT),
            _ => None,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::Gemini => "Gemini CLI",
            Self::Antigravity => "Google Antigravity",
            Self::Codex => "OpenAI Codex CLI",
            Self::Cursor => "Cursor",
            Self::VSCode => "VS Code",
            Self::ChatGPT => "ChatGPT (Web / Desktop)",
        }
    }

    #[must_use]
    pub fn config_path(&self) -> Option<PathBuf> {
        let home = home_dir()?;
        match self {
            Self::ClaudeDesktop => {
                #[cfg(target_os = "macos")]
                {
                    Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
                }
                #[cfg(target_os = "windows")]
                {
                    std::env::var("APPDATA")
                        .ok()
                        .map(PathBuf::from)
                        .map(|p| p.join("Claude/claude_desktop_config.json"))
                        .or_else(|| {
                            Some(home.join("AppData/Roaming/Claude/claude_desktop_config.json"))
                        })
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    Some(home.join(".config/Claude/claude_desktop_config.json"))
                }
            }
            Self::ClaudeCode => Some(home.join(".claude.json")),
            Self::Gemini => Some(home.join(".gemini/settings.json")),
            Self::Antigravity => Some(home.join(".gemini/antigravity/settings.json")),
            Self::Codex => Some(home.join(".codex/config.toml")),
            Self::Cursor => Some(home.join(".cursor/mcp.json")),
            Self::VSCode => Some(PathBuf::from(".vscode/mcp.json")),
            Self::ChatGPT => None,
        }
    }
}

pub fn get_fossil_binary_path() -> PathBuf {
    if let Some(exe) = std::env::current_exe().ok().filter(|p| p.exists()) {
        return exe;
    }
    PathBuf::from("fossil")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .map(PathBuf::from)
}

/// Installs Fossil MCP configuration into the specified AI assistant's configuration file.
pub fn install_client(target: ClientTarget) -> Result<String, String> {
    if target == ClientTarget::ChatGPT {
        return Ok(chatgpt_instructions());
    }

    let Some(path) = target.config_path() else {
        return Err(format!(
            "Cannot determine configuration path for {}",
            target.display_name()
        ));
    };

    let bin_path = get_fossil_binary_path();
    let bin_str = bin_path.to_string_lossy();

    if target == ClientTarget::Codex {
        install_toml_config(&path, &bin_str)?;
    } else {
        install_json_config(&path, &bin_str)?;
    }

    Ok(format!(
        "Successfully configured {} with Fossil MCP server at '{}'\nConfig file: {}",
        target.display_name(),
        bin_str,
        path.display()
    ))
}

/// Uninstalls Fossil MCP configuration from the specified AI assistant's config file.
pub fn uninstall_client(target: ClientTarget) -> Result<String, String> {
    let Some(path) = target.config_path() else {
        return Err(format!(
            "Cannot determine configuration path for {}",
            target.display_name()
        ));
    };

    if !path.exists() {
        return Ok(format!(
            "Configuration file does not exist: {}",
            path.display()
        ));
    }

    if target == ClientTarget::Codex {
        uninstall_toml_config(&path)?;
    } else {
        uninstall_json_config(&path)?;
    }

    Ok(format!(
        "Successfully removed Fossil MCP server from {}\nConfig file: {}",
        target.display_name(),
        path.display()
    ))
}

/// Checks installation status of all supported client configurations.
#[must_use]
pub fn check_status() -> Vec<(ClientTarget, bool, Option<PathBuf>)> {
    let targets = [
        ClientTarget::ClaudeDesktop,
        ClientTarget::ClaudeCode,
        ClientTarget::Gemini,
        ClientTarget::Antigravity,
        ClientTarget::Codex,
        ClientTarget::Cursor,
        ClientTarget::VSCode,
    ];

    targets
        .into_iter()
        .map(|t| {
            let path = t.config_path();
            let is_installed = path
                .as_ref()
                .is_some_and(|p| is_configured(t, p).unwrap_or(false));
            (t, is_installed, path)
        })
        .collect()
}

fn is_configured(target: ClientTarget, path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;

    if target == ClientTarget::Codex {
        Ok(content.contains("[mcp_servers.fossil]"))
    } else {
        let val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        Ok(val
            .get("mcpServers")
            .and_then(|s| s.get("fossil"))
            .is_some())
    }
}

fn install_json_config(path: &Path, bin_path: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let mut root: Value = if path.exists() {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if raw.trim().is_empty() {
            serde_json::json!({})
        } else {
            // Backup before editing
            let bak_path = path.with_extension("json.bak");
            let _ = fs::write(&bak_path, &raw);

            serde_json::from_str(&raw)
                .map_err(|e| format!("Failed to parse JSON in {}: {e}", path.display()))?
        }
    } else {
        serde_json::json!({})
    };

    if !root.is_object() {
        root = serde_json::json!({});
    }

    let mcp_servers = root
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    if !mcp_servers.is_object() {
        *mcp_servers = serde_json::json!({});
    }

    mcp_servers.as_object_mut().unwrap().insert(
        "fossil".to_owned(),
        serde_json::json!({
            "command": bin_path,
            "args": ["mcp"]
        }),
    );

    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    fs::write(path, formatted).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

fn uninstall_json_config(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut root: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse JSON in {}: {e}", path.display()))?;

    if let Some(mcp_servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        mcp_servers.remove("fossil");
    }

    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    fs::write(path, formatted).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

fn install_toml_config(path: &Path, bin_path: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let raw = if path.exists() {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        let bak = path.with_extension("toml.bak");
        let _ = fs::write(&bak, &content);
        content
    } else {
        String::new()
    };

    let mut doc: toml_edit::DocumentMut = raw
        .parse()
        .map_err(|e| format!("Failed to parse TOML in {}: {e}", path.display()))?;

    let mcp_servers = doc
        .entry("mcp_servers")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .ok_or("mcp_servers is not a table")?;

    let mut fossil_tbl = toml_edit::Table::new();
    fossil_tbl.insert("command", toml_edit::value(bin_path));
    let mut args_arr = toml_edit::Array::new();
    args_arr.push("mcp");
    fossil_tbl.insert("args", toml_edit::value(args_arr));

    mcp_servers.insert("fossil", toml_edit::Item::Table(fossil_tbl));

    fs::write(path, doc.to_string())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

fn uninstall_toml_config(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = raw
        .parse()
        .map_err(|e| format!("Failed to parse TOML in {}: {e}", path.display()))?;

    if let Some(mcp_servers) = doc
        .get_mut("mcp_servers")
        .and_then(|i| i.as_table_like_mut())
    {
        mcp_servers.remove("fossil");
    }

    fs::write(path, doc.to_string())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

fn chatgpt_instructions() -> String {
    "To connect Fossil MCP to ChatGPT (Web or Desktop):\n\n\
     1. Start the Fossil Streamable HTTP server:\n\
        fossil mcp serve --http --port 7742\n\n\
     2. For ChatGPT Web, expose your local port securely (e.g. via Cloudflare Tunnel or Tailscale):\n\
        cloudflared tunnel --url http://127.0.0.1:7742\n\n\
     3. In ChatGPT -> Settings -> Connected apps / Custom Actions / MCP Connectors:\n\
        - URL: https://<your-tunnel-domain>/mcp (or http://127.0.0.1:7742/mcp for local desktop)\n\
        - Authentication: Bearer Token\n\
        - Enter the token printed when starting `fossil mcp serve --http`."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_json_install_and_uninstall() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("config.json");

        // First install on fresh non-existent file
        install_json_config(&cfg_path, "/usr/local/bin/fossil").unwrap();
        let content = fs::read_to_string(&cfg_path).unwrap();
        let val: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            val["mcpServers"]["fossil"]["command"],
            "/usr/local/bin/fossil"
        );
        assert_eq!(val["mcpServers"]["fossil"]["args"][0], "mcp");

        // Uninstall
        uninstall_json_config(&cfg_path).unwrap();
        let content2 = fs::read_to_string(&cfg_path).unwrap();
        let val2: Value = serde_json::from_str(&content2).unwrap();
        assert!(val2.get("mcpServers").unwrap().get("fossil").is_none());
    }

    #[test]
    fn test_toml_install_and_uninstall() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("config.toml");

        // First install
        install_toml_config(&cfg_path, "/usr/local/bin/fossil").unwrap();
        let content = fs::read_to_string(&cfg_path).unwrap();
        assert!(content.contains("[mcp_servers.fossil]"));
        assert!(content.contains("command = \"/usr/local/bin/fossil\""));

        // Uninstall
        uninstall_toml_config(&cfg_path).unwrap();
        let content2 = fs::read_to_string(&cfg_path).unwrap();
        assert!(!content2.contains("[mcp_servers.fossil]"));
    }
}
