# Agent Registration — wiring FenceyMD's MCP server into each agent

FenceyMD runs a local-only MCP server while it's open, so AI coding agents
(Claude Code, Gemini, Codex, OpenCode, …) can drive the reader — open chapters,
read content, see the user's selection. This doc covers **how an agent gets
connected**.

> **New here?** Start with the step-by-step **[MCP_SETUP.md](MCP_SETUP.md)**
> walkthrough. This page is the per-agent schema reference for manual setup.

## The easy way: the Settings toggle (recommended)

Open **Settings → AI agent control** and flip the toggle for each agent you
want to enable. FenceyMD writes the right entry into that agent's own config
file, idempotently and non-destructively (every other key in the file is
preserved). Toggling off removes only FenceyMD's entry.

> **Restart the agent after toggling.** Agents read their MCP config only at
> session start, so an already-running `claude` / `codex` / `gemini` /
> `opencode` session won't see the change until you start a fresh one.

The toggle points every agent at the **native bridge**: FenceyMD's own binary
run with `--mcp-bridge`. That subcommand bridges the agent's stdio JSON-RPC to
the running app's local HTTP MCP server, discovering the (random, per-launch)
port from the port file on each connection. Because the port is rediscovered
every time, a single static config entry keeps working across app restarts —
no Node, no manual port edits. (If the app binary later moves — e.g. an update —
FenceyMD self-heals registered configs on next launch.)

## How discovery works (under the hood)

- The MCP HTTP server listens on a random `127.0.0.1:<port>` while the app is
  open. The port is published to a JSON **port file** in the app-data dir:
  - macOS: `~/Library/Application Support/com.fenceymd.app/port`
  - Windows: `%APPDATA%\com.fenceymd.app\port`
  - Linux: `$XDG_DATA_HOME/com.fenceymd.app/port` (default `~/.local/share/...`)
- The `--mcp-bridge` subcommand reads that file (honoring `$FENCEYMD_PORT_DIR`
  for tests) and POSTs JSON-RPC frames to `http://127.0.0.1:<port>/mcp`.
- If FenceyMD isn't running, the bridge emits a JSON-RPC error (the agent sees
  a real error, not a hang). Launch FenceyMD and the next call succeeds.

## The manual way (fallback / other agents)

If you'd rather edit configs by hand, or your agent isn't in the toggle list,
use the shapes below. `<APP>` is the command that launches the bridge: once the
`fenceymd` CLI is installed (first launch, or Settings → AI agent control →
Install CLI) use just **`fenceymd`**; otherwise the absolute path to the binary
(macOS: `/Applications/FenceyMD.app/Contents/MacOS/fenceymd`). The Settings
toggle picks `fenceymd` automatically when the CLI is present. The schemas
differ per agent — match them exactly or the entry is silently ignored.

### Claude Code — `~/.claude.json` (root-level `mcpServers` = user scope)

```json
{
  "mcpServers": {
    "fenceymd": { "type": "stdio", "command": "<APP>", "args": ["--mcp-bridge"] }
  }
}
```

`~/.claude.json` also holds Claude Code's own state — preserve every other key
when editing by hand (the Settings toggle does a safe read-modify-write).

### Gemini CLI / Antigravity — `~/.gemini/settings.json`

Transport is **inferred** from `command`; do **not** add a `type` field.

```json
{
  "mcpServers": {
    "fenceymd": { "command": "<APP>", "args": ["--mcp-bridge"] }
  }
}
```

### OpenCode — `~/.config/opencode/opencode.json`

Top-level key is **`mcp`** (not `mcpServers`); type is `"local"`; the command
is a **single array**. Honors `$XDG_CONFIG_HOME` and `$OPENCODE_CONFIG`.

```json
{
  "mcp": {
    "fenceymd": { "type": "local", "command": ["<APP>", "--mcp-bridge"], "enabled": true }
  }
}
```

### Codex — `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`)

```toml
[mcp_servers.fenceymd]
command = "<APP>"
args = ["--mcp-bridge"]
```

## HTTP transport (alternative)

Agents that speak Streamable HTTP MCP can skip the bridge and point at the URL
directly — but the port is random per launch, so a hardcoded URL goes stale on
the next restart. Prefer the bridge for stability. If you do use HTTP, read the
current port from the port file above and use:

```json
{ "mcpServers": { "fenceymd": { "type": "http", "url": "http://127.0.0.1:<port>/mcp" } } }
```

## Multiple windows

Each FenceyMD window runs its own server and writes a per-pid `port-<pid>`
file alongside the bare `port` alias (which tracks the most-recent instance).
Agents that must target a specific window read `port-<pid>` directly.
