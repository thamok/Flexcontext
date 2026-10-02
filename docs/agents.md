# Agent integrations

Install `flexcontext` using the [build instructions](../README.md#build-and-install). Every integration starts the same local stdio server:

```sh
flexcontext serve /absolute/path/to/repository
```

Use an absolute binary path if your editor does not inherit your shell's `PATH`. Each server searches one configured repository. No API key is required. The following commands use POSIX shell syntax; JSON configurations also work in other environments with appropriate executable paths.

## Codex

From the repository you want to search:

```sh
codex mcp add flexcontext -- "$(command -v flexcontext)" serve "$PWD"
codex mcp list
```

The command writes user configuration, so the registered server keeps searching that repository even when you change directories. Use distinct server names for multiple repositories. For a project-specific setup, add the following to the trusted repository's `.codex/config.toml`, replacing both paths:

```toml
[mcp_servers.flexcontext]
command = "/absolute/path/to/flexcontext"
args = ["serve", "/absolute/path/to/repository"]
startup_timeout_sec = 30
tool_timeout_sec = 30
```

Start a new session or restart MCP servers after configuring or rebuilding the binary. Check `/mcp` in the terminal UI. See [official Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

## Claude Code

From the repository you want to search:

```sh
claude mcp add --transport stdio --scope project flexcontext -- "$(command -v flexcontext)" serve "$PWD"
claude mcp list
```

This registers the server in project `.mcp.json`. Review the generated absolute paths before sharing the configuration with collaborators. Start a new session, approve the project server when prompted, and check `/mcp`. See [official Claude Code MCP documentation](https://code.claude.com/docs/en/mcp).

## Cursor

Merge this entry into your project's `.cursor/mcp.json` under `mcpServers`; preserve other servers. Replace the executable path with your installed binary. `${workspaceFolder}` is resolved by Cursor.

```json
{
  "mcpServers": {
    "flexcontext": {
      "type": "stdio",
      "command": "/absolute/path/to/flexcontext",
      "args": ["serve", "${workspaceFolder}"]
    }
  }
}
```

Enable the server in Cursor's MCP settings and confirm its tools are listed. See [official Cursor MCP documentation](https://cursor.com/docs/mcp).

## VS Code / GitHub Copilot

Merge this entry into `.vscode/mcp.json` under `servers`, replacing the executable path. VS Code uses `servers`, whereas Cursor uses `mcpServers`.

```json
{
  "servers": {
    "flexcontext": {
      "type": "stdio",
      "command": "/absolute/path/to/flexcontext",
      "args": ["serve", "${workspaceFolder}"]
    }
  }
}
```

Use **MCP: List Servers** to start the server, accept the trust prompt, and enable its tools in agent chat. See [official VS Code MCP documentation](https://code.visualstudio.com/docs/agent-customization/mcp-servers).

## Optional discovery skill

The bundled [flexcontext skill](../integrations/skills/flexcontext/SKILL.md) teaches a discovery → targeted search → edit workflow. It works with either MCP or the CLI and does not install the binary or configure the server.

Run these commands from the cloned `flexcontext` repository. Pick the harness you use; if the destination already contains a skill, review it before replacing it.

```sh
# Codex: install for your user.
mkdir -p "$HOME/.agents/skills"
cp -R integrations/skills/flexcontext "$HOME/.agents/skills/"

# Claude Code: install for your user.
mkdir -p "$HOME/.claude/skills"
cp -R integrations/skills/flexcontext "$HOME/.claude/skills/"
```

Restart the session. Invoke `$flexcontext` in Codex or `/flexcontext` in Claude Code, or ask for repository discovery. See the official [Codex skill guide](https://learn.chatgpt.com/docs/build-skills) and [Claude Code skill guide](https://code.claude.com/docs/en/skills).

For harnesses without skills, add this instruction to their repository guidance:

> Use flexcontext to discover relevant implementations when the files or symbols are unknown. Follow its source locations with targeted searches and source inspection before editing. Refresh the resident MCP index after edits.

## First query and troubleshooting

Ask the agent to call `code_search` with `{"query":"session expiration","max_tokens":8000}`. Both tools are read-only with respect to repository source; indexing may write the local `.flexcontext` cache.

- **Server does not start:** run the configured binary with `--version`, check the absolute paths, then run `flexcontext index /absolute/path/to/repository` to expose scan errors. For large repositories, choose a smaller root or adjust the [scan limits](indexing.md).
- **Results are stale:** call `refresh_index` after source edits. Restart after rebuilding the server binary.
- **No visible source in tool results:** the client must consume MCP `structuredContent`. The text field contains only a result summary. Text-only clients are unsupported; use the CLI's `--json` output instead.
- **No relevant hits:** check [language support](languages.md), ignore rules, explicit filters and the repository root. Try terms describing the implementation, then follow known identifiers with a targeted text search.

These are documented configuration recipes. Automated tests cover the server's protocol, tools and result formats; client UI behavior depends on the installed harness version. See the [MCP reference](mcp.md) for compatibility details.
