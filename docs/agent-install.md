# Use Code Explorer With Claude Code And Codex

This guide installs Code Explorer as a local MCP server for AI coding agents.

## 1. Build Or Install The Binary

From source:

```bash
git clone https://github.com/phuetz/code-explorer.git
cd code-explorer
cargo build --release -p code-explorer-cli
```

The binary is:

- Windows: `target\release\code-explorer.exe`
- macOS/Linux: `target/release/code-explorer`

Put it on your `PATH`, or call it by absolute path.

Optional Magika/ONNX build for extensionless scripts:

```bash
cargo build --release -p code-explorer-cli --features magika-detect
```

## 2. Index The Project

Run this in the repository you want your agent to understand:

```bash
cd /path/to/your/project
code-explorer analyze .
code-explorer status
```

This creates `.codeexplorer/` in the project with the persisted graph.

## 3. Claude Code

Project-local install:

```bash
cd /path/to/your/project
code-explorer mcp-install --client claude --scope project
```

This writes `.mcp.json`:

```json
{
  "mcpServers": {
    "code-explorer": {
      "command": "/absolute/path/to/code-explorer",
      "args": ["mcp"]
    }
  }
}
```

Global install:

```bash
code-explorer mcp-install --client claude --scope global
```

Restart Claude Code after installation.

Claude Code skill:

- Bundled in this repo: `.claude/skills/code-explorer/SKILL.md`
- Optional global copy:

```bash
mkdir -p ~/.claude/skills
cp -R .claude/skills/code-explorer ~/.claude/skills/code-explorer
```

On Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force $env:USERPROFILE\.claude\skills
Copy-Item -Recurse .claude\skills\code-explorer $env:USERPROFILE\.claude\skills\code-explorer -Force
```

## 4. Codex

Codex install:

```bash
code-explorer mcp-install --client codex --scope global
```

Codex CLI reads MCP servers from the Codex user config. This updates:

- Windows: `%USERPROFILE%\.codex\config.toml`
- macOS/Linux: `~/.codex/config.toml`
- Custom: `$CODEX_HOME/config.toml`

The generated entry looks like this:

```toml
[mcp_servers.code_explorer]
command = "/absolute/path/to/code-explorer"
args = ["mcp"]
enabled = true
startup_timeout_sec = 30
```

Restart Codex after installation.

Codex skill:

- Bundled in this repo: `.codex/skills/code-explorer/SKILL.md`
- Optional global copy:

```bash
mkdir -p ~/.codex/skills
cp -R .codex/skills/code-explorer ~/.codex/skills/code-explorer
```

On Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force $env:USERPROFILE\.codex\skills
Copy-Item -Recurse .codex\skills\code-explorer $env:USERPROFILE\.codex\skills\code-explorer -Force
```

## 5. Install Both Agents

```bash
cd /path/to/your/project
code-explorer analyze .
code-explorer mcp-install --client both --scope project
```

This creates:

- `.mcp.json` for Claude Code
- Codex user config entry in `%USERPROFILE%\.codex\config.toml`, `~/.codex/config.toml`, or `$CODEX_HOME/config.toml`

## 6. Cursor

Project-local install:

```bash
cd /path/to/your/project
code-explorer mcp-install --client cursor --scope project
```

This writes `.cursor/mcp.json` using the common MCP `mcpServers` JSON shape:

```json
{
  "mcpServers": {
    "code-explorer": {
      "command": "/absolute/path/to/code-explorer",
      "args": ["mcp"]
    }
  }
}
```

Global install:

```bash
code-explorer mcp-install --client cursor --scope global
```

This writes `~/.cursor/mcp.json` (or `%USERPROFILE%\.cursor\mcp.json` on Windows).

Restart Cursor after installation.

## 7. VS Code

Project-local install:

```bash
cd /path/to/your/project
code-explorer mcp-install --client vscode --scope project
```

This writes `.vscode/mcp.json` using VS Code's `servers` MCP shape:

```json
{
  "servers": {
    "code-explorer": {
      "type": "stdio",
      "command": "/absolute/path/to/code-explorer",
      "args": ["mcp"]
    }
  }
}
```

VS Code user/global MCP configuration is profile-dependent, so Code Explorer currently writes only workspace `.vscode/mcp.json`. For user scope, use the VS Code command `MCP: Add Server`.

Restart VS Code or run `MCP: List Servers` after installation.

## 8. Claude Desktop

Claude Desktop uses a global user config file:

- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Install:

```bash
code-explorer mcp-install --client claude-desktop --scope global
```

This writes:

```json
{
  "mcpServers": {
    "code-explorer": {
      "command": "/absolute/path/to/code-explorer",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Desktop after installation.

## 9. Install All Supported Clients

```bash
cd /path/to/your/project
code-explorer analyze .
code-explorer mcp-install --client all --scope project
```

This creates:

- `.mcp.json` for Claude Code
- Codex user config entry in `%USERPROFILE%\.codex\config.toml`, `~/.codex/config.toml`, or `$CODEX_HOME/config.toml`
- `.cursor/mcp.json` for Cursor
- `.vscode/mcp.json` for VS Code

## 10. Try It

Ask Claude Code or Codex:

```text
What calls PaymentService?
What breaks if I change handleLogin?
Show me the impact of UserRepository.
Where is authentication checked?
```

For long JSON, diff, log, search, or source outputs selected from the graph, install lm-resizer alongside Code Explorer so the agent can compress supported context and retrieve originals when CCR offload applies. See [lm-resizer-integration.md](lm-resizer-integration.md).

Or run the graph directly:

```bash
code-explorer query "authentication middleware"
code-explorer context PaymentService
code-explorer impact PaymentService --direction both
code-explorer report
```

## 11. Troubleshooting

- If the agent cannot see tools, restart the agent after installing MCP.
- If commands return no graph, run `code-explorer analyze .` in the project first.
- If your target folder is not a Git repository, run `code-explorer analyze . --skip-git`.
- For stale results after a big refactor, run `code-explorer analyze . --force`.
- MCP mode must keep stdout clean. Code Explorer writes MCP protocol messages to stdout and logs to stderr.
