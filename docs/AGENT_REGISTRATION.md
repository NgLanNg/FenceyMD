# Agent Registration — wiring FenceyMD's MCP server into each agent

Per-agent config snippets for connecting FenceyMD's local MCP server to an
AI coding agent. Use this when the **Settings → AI agent control** toggle
isn't enough (an agent not in the toggle list, or you want to edit by hand).

> **New here?** Start with the step-by-step **[`MCP_SETUP.md`](MCP_SETUP.md)**
> walkthrough. This page is the schema reference.

> **Verified scope.** What's been tested directly: the MCP server and its 7
> tools (driven over HTTP and via the `fenceymd --mcp-bridge` subprocess), the
> path-traversal guard, and that the Settings toggle writes the entry shapes
> below correctly. Whether a given agent then *connects* depends on that agent
> and its current MCP support — the per-agent shapes here follow each agent's
> own docs and may drift as those tools change. Claude Code is the path
> exercised most. After registering, **restart the agent**.

---

## The recommended way: Settings toggle

Open **Settings → AI agent control** in FenceyMD and flip the toggle for
each agent you want to enable. FenceyMD writes the correct entry into
that agent's own config file, idempotently and non-destructively (every
other key is preserved). Toggling off removes only FenceyMD's entry.

> **Restart the agent after toggling.** Agents read their MCP config only
> at session start. An already-running `claude` / `codex` / `gemini` /
> `opencode` session won't see FenceyMD until you start a fresh one.

The toggle points every agent at the same binary:

```
fenceymd --mcp-bridge
```

`fenceymd` is FenceyMD's own binary, run with the bridge subcommand. It
reads the app's port file on every connection, so a single static config
entry keeps working across app restarts. If the app binary later moves
(update, drag to a new path), FenceyMD self-heals registered configs on
next launch.

---

## The shared shape

Every stdio agent config below uses the same command. The `command`
field points at the bare name `fenceymd` (resolved from your PATH) — the
Settings toggle picks this automatically when the CLI is installed. If
the CLI isn't on PATH, fall back to the absolute path:

- macOS:   `/Applications/FenceyMD.app/Contents/MacOS/fenceymd`
- Linux:   `$(which fenceymd)` or wherever the install symlink points
- Windows: `C:\Program Files\FenceyMD\fenceymd.exe` (or wherever you
  installed it)

The rest of the config differs per agent — match it exactly or the entry
is silently ignored.

---

## Claude Code — `~/.claude.json`

Root-level `mcpServers` object. `type` is **`"stdio"`** (required for
Claude Code; it's the default transport, but the field must be present).

```json
{
  "mcpServers": {
    "fenceymd": {
      "type": "stdio",
      "command": "fenceymd",
      "args": ["--mcp-bridge"]
    }
  }
}
```

`~/.claude.json` also holds Claude Code's own state — every other top-
level key (`oauthAccount`, `numStartups`, etc.) is preserved by the
Settings toggle's read-modify-write.

---

## Gemini CLI — `~/.gemini/settings.json`

Root-level `mcpServers` object. **Do not add a `type` field** — Gemini
infers the transport from the presence of `command` + `args` (stdio).

```json
{
  "mcpServers": {
    "fenceymd": {
      "command": "fenceymd",
      "args": ["--mcp-bridge"]
    }
  }
}
```

---

## Antigravity — same file as Gemini CLI

Antigravity reads the same `~/.gemini/settings.json` as Gemini CLI and
uses the same schema:

```json
{
  "mcpServers": {
    "fenceymd": {
      "command": "fenceymd",
      "args": ["--mcp-bridge"]
    }
  }
}
```

**Restart the IDE**, not just the chat, when you change the config.

---

## OpenCode — `~/.config/opencode/opencode.json`

Top-level key is **`mcp`** (not `mcpServers`). The transport type is
**`"local"`** (OpenCode's name for stdio). The `command` field is a
**single array** — `["binary", "arg1", "arg2"]`, not two separate fields.

```json
{
  "mcp": {
    "fenceymd": {
      "type": "local",
      "command": ["fenceymd", "--mcp-bridge"],
      "enabled": true
    }
  }
}
```

Honors `$XDG_CONFIG_HOME` and `$OPENCODE_CONFIG` if you need to relocate
the config file.

---

## Codex — `~/.codex/config.toml`

TOML, not JSON. Section name is **`[mcp_servers.fenceymd]`** (not
`[[mcp_servers]]` — that's for arrays; `fenceymd` is a single server,
not a list). `command` and `args` are separate top-level keys under the
section.

```toml
[mcp_servers.fenceymd]
command = "fenceymd"
args = ["--mcp-bridge"]
```

The Settings toggle uses `toml_edit` to do a safe read-modify-write, so
your existing comments and sibling servers in `config.toml` are
preserved. (If you edit by hand, just keep this section.)

Honors `$CODEX_HOME` if you need to relocate the config.

---

## HTTP transport (no bridge)

If your agent speaks **Streamable HTTP MCP**, you can skip the bridge and
point it at the URL directly. Read the port from the file (locations in
[`MCP_SETUP.md`](MCP_SETUP.md#0-what-you-need)) and substitute:

```json
{ "mcpServers": { "fenceymd": { "type": "http", "url": "http://127.0.0.1:PORT/mcp" } } }
```

**The port changes on every app launch**, so a hardcoded URL goes stale
within minutes. The bridge is the durable choice — it reads the port
file on every connection. Use HTTP only if your agent genuinely cannot
speak stdio.

---

## Multiple windows

Each FenceyMD window runs its own MCP server and writes a per-pid
`port-<pid>` file alongside the bare `port` alias. The bare `port` file
tracks the most-recent instance (the one the bridge will dial by
default). To target a specific window, read `port-<pid>` directly and
point your HTTP config at that port.

---

## Verifying the manual config

After writing the file, the verification is the same as the toggle path:

1. Make sure FenceyMD is open (it must be running for the bridge to
   find the port file).
2. Start a **fresh** session of the agent you just configured (the
   #1 cause of "it doesn't work" is editing config then running the
   already-running session).
3. From the agent, ask it to call `get_book_toc` — you should get the
   chapters of the open folder.
4. From a terminal: `echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | fenceymd --mcp-bridge`
   should print the tool list as a single JSON line.

If any of those fail, see the **Troubleshooting** table in
[`MCP_SETUP.md`](MCP_SETUP.md#troubleshooting).
