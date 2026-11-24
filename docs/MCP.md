# Fossil MCP Server - AI Assistant Integration Guide

Fossil includes a built-in **Model Context Protocol (MCP)** server that allows AI assistants like **Claude Desktop**, **Claude Code**, **Gemini CLI**, **Google Antigravity**, **OpenAI Codex CLI**, and **ChatGPT (Web & Desktop)** to inspect storage, search across files, identify build bloat, and safely manage files on your machine.

---

## Quick Setup by Assistant

### 1. Claude Desktop & Claude Code

#### 1-Click CLI Setup

```bash
# Configure Claude Desktop
fossil mcp install claude-desktop

# Configure Claude Code
fossil mcp install claude-code
```

#### Manual Claude Desktop Configuration

Add Fossil to your `claude_desktop_config.json`:

- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
- **Linux:** `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "fossil": {
      "command": "/absolute/path/to/fossil",
      "args": ["mcp"]
    }
  }
}
```

---

### 2. Gemini CLI & Google Antigravity

#### 1-Click CLI Setup

```bash
# Configure Gemini CLI
fossil mcp install gemini

# Configure Google Antigravity
fossil mcp install antigravity
```

#### Manual Gemini Configuration

Add to `~/.gemini/settings.json` (or `~/.gemini/antigravity/settings.json`):

```json
{
  "mcpServers": {
    "fossil": {
      "command": "/absolute/path/to/fossil",
      "args": ["mcp"]
    }
  }
}
```

---

### 3. ChatGPT (Web & Desktop)

ChatGPT connects to MCP servers over **Streamable HTTP** (`POST /mcp` with Bearer Token authentication).

#### Step 1: Start the Fossil HTTP MCP Server

```bash
fossil mcp serve --http --addr 127.0.0.1:7742
```

Fossil will print a secure random authentication token, for example:

```text
Fossil MCP HTTP server listening on http://127.0.0.1:7742
Authentication token: a1b2c3d4e5f6...
MCP Endpoint: http://127.0.0.1:7742/mcp
```

#### Step 2: Expose Secure Tunnel (for ChatGPT Web)

Because ChatGPT Web runs in the cloud, expose your local port securely using Cloudflare Tunnel or Tailscale Funnel:

```bash
# Using Cloudflare Tunnel
cloudflared tunnel --url http://127.0.0.1:7742

# Or using Tailscale Funnel
tailscale funnel 7742
```

#### Step 3: Configure in ChatGPT

1. Go to **ChatGPT Settings** &rarr; **Connected apps** / **MCP Connectors**.
2. Add a new MCP server:
   - **URL:** `https://<your-tunnel-domain>/mcp` (or `http://127.0.0.1:7742/mcp` for local desktop app)
   - **Authentication:** Bearer Token
   - **Token:** Paste the token printed in Step 1.

Fossil also provides RFC 9728 OAuth 2.1 metadata discovery at `/.well-known/oauth-protected-resource`.

---

### 4. OpenAI Codex CLI

```bash
fossil mcp install codex
```

Or manually add to `~/.codex/config.toml`:

```toml
[mcp_servers.fossil]
command = "/absolute/path/to/fossil"
args = ["mcp"]
```

---

### 5. Cursor & VS Code

```bash
fossil mcp install cursor
# or
fossil mcp install vscode
```

Or manually configure `~/.cursor/mcp.json` or `.vscode/mcp.json`:

```json
{
  "mcpServers": {
    "fossil": {
      "command": "fossil",
      "args": ["mcp"]
    }
  }
}
```

---

## Verifying Setup

Check which AI clients are configured:

```bash
fossil mcp status
```

To remove Fossil from any assistant:

```bash
fossil mcp uninstall <client>
```

---

## Available MCP Tools

All Fossil MCP tools provide **Tool Annotations** (`readOnlyHint`, `destructiveHint`, `openWorldHint`) for safe execution and client confirmation prompts:

| Tool                         | Annotations             | Description                                                                                                   |
| :--------------------------- | :---------------------- | :------------------------------------------------------------------------------------------------------------ |
| **`scan_directory`**         | `readOnlyHint: true`    | Scans a directory across all CPU cores and caches the arena in memory for instant queries.                    |
| **`list_children`**          | `readOnlyHint: true`    | Lists child entries of a folder directly from memory with sorting and pagination, with no rescan.             |
| **`search_files`**           | `readOnlyHint: true`    | Case-insensitive multi-core search by name, path, extension, category, or file size.                          |
| **`get_largest_files`**      | `readOnlyHint: true`    | Returns the largest files sorted descending, with category and minimum age filters.                           |
| **`get_group_summary`**      | `readOnlyHint: true`    | Groups storage by `category`, `extension`, `size_tier`, or `age`.                                             |
| **`get_bloat_dirs`**         | `readOnlyHint: true`    | Finds regeneratable build/cache directories (`node_modules`, `target`, `.venv`, `DerivedData`).               |
| **`list_volumes`**           | `readOnlyHint: true`    | Lists mounted disks, filesystems, and common user target folders with usage stats.                            |
| **`get_cache_status`**       | `readOnlyHint: true`    | Returns cached root path, total files/nodes, scan duration, and memory metrics.                               |
| **`trash_paths`**            | `destructiveHint: true` | Safely moves files or directories to the OS Trash (protected system paths blocked). Supports `dry_run: true`. |
| **`reveal_in_file_manager`** | `readOnlyHint: true`    | Highlights a file or folder in Finder, File Explorer, or Linux file manager.                                  |

---

## MCP Prompts

Fossil exports ready-to-use guided workflows to compatible assistants:

- **`audit-disk-space`**: Complete disk audit discovering top space hogs, category distribution, and reclaimable storage.
- **`clean-dev-bloat`**: Discovers regeneratable development caches (`node_modules`, `target`, `.venv`, `DerivedData`) with safe dry-run preview.
- **`find-old-large-files`**: Spotlight large untouched files (videos, archives, disk images) not modified for N days.

---

## MCP Resources

- **`fossil://volumes`**: Live JSON representation of mounted storage drives and user quick targets.
- **`fossil://cache/summary`**: Current in-memory cached scan summary.

---

## Security & Sandboxing Options

You can enforce strict safety policies on the MCP server:

```bash
# Run in read-only mode (disables trash_paths and reveal)
fossil mcp --read-only

# Restrict file operations to specific directory trees
fossil mcp --allow-dir ~/Projects --allow-dir ~/Downloads
```
