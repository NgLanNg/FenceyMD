# MCP Setup — let an AI agent drive Fenceymd

Fenceymd runs a small **local MCP server** whenever it's open, so an AI coding
agent (Claude Code, Gemini, Codex, OpenCode, …) can read and navigate your docs
*with* you — open a chapter, pull its content, see what you've selected, even
grab a screenshot. This guide gets you connected in about a minute.

> **Everything stays on your machine.** The server binds to `127.0.0.1` only,
> has no network egress, and an agent can only touch files inside the folder you
> opened. No tokens, no telemetry.

---

## How it works (10-second version)

```
Fenceymd (open) ──► starts a local MCP server on a random 127.0.0.1 port
                     and writes that port to a file in its app-data dir
        │
   your agent ──► talks to it via the native bridge:  fenceymd --mcp-bridge
                  (the bridge rediscovers the port each time, so your config
                   keeps working across restarts — no Node, no manual ports)
```

You don't manage the port or the bridge by hand — the **Settings toggle** wires
it all up for you.

---

## Setup in 3 steps

### 1. Run Fenceymd with a folder open
Launch the app and open a folder of Markdown (**Open another folder…** in
Settings, or the picker). The MCP server starts automatically; an agent can only
see the folder that's currently open.

### 2. Enable your agent
Open **Settings → AI agent control** and flip the toggle for your agent:

| Agent | Toggle writes to |
|-------|------------------|
| **Claude Code** | `~/.claude.json` |
| **Gemini CLI / Antigravity** | `~/.gemini/settings.json` |
| **OpenCode** | `~/.config/opencode/opencode.json` |
| **Codex** | `~/.codex/config.toml` |

The toggle writes the `fenceymd` MCP entry into that agent's own config —
idempotently, and without touching anything else in the file. Toggle off to
remove it.

### 3. Restart the agent
> ⚠️ **Agents read their MCP config only at session start.** A `claude` /
> `codex` / `gemini` / `opencode` session that's already running won't see
> Fenceymd until you start a **fresh** session. This is the #1 "it's not
> working" cause.

That's it.

---

## Verify it's connected

**From inside the agent** (Claude Code shown): run `/mcp` — `fenceymd` should be
listed. Then ask it to call `get_book_toc`; you should get the chapters of the
open folder.

**From a terminal** (smoke test — the native bridge reads the port for you).
FenceyMD installs a `fenceymd` command on your PATH on first launch (or via
**Settings → AI agent control → Install CLI**), so you can just run:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
  | fenceymd --mcp-bridge
```

You should get back the tool list as a single JSON line. Swap in a `tools/call`
to exercise one:

```bash
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_book_toc","arguments":{}}}' \
  | fenceymd --mcp-bridge
```

(If `fenceymd` isn't found, the CLI didn't install into an on-PATH dir — open
Settings → AI agent control and click **Install CLI**, or run the app binary at
`/Applications/FenceyMD.app/Contents/MacOS/fenceymd --install-cli` once.)

---

## What the agent can do — the 7 tools

| Tool | What it does |
|------|--------------|
| `open_file` | Navigate the reader to a chapter by path |
| `get_current_chapter` | What's on screen now — path, scroll, a 500-char preview |
| `get_chapter_content` | Full Markdown of a chapter (≤ 1 MB) |
| `get_selected_text` | The text you've highlighted, with its block anchor |
| `get_book_toc` | Flat list of every chapter in the open folder |
| `capture_screenshot` | The window as a base64 PNG (downscaled ≤1600px) for a vision LLM |
| `get_debug_log` | Recent activity-log lines (`tail` / `contains` / `since_ts` filters) |

Writes/paths are bounded to the open folder; path traversal (`../…`, absolute
escapes) is rejected in Rust before any disk access.

---

## Advanced / manual setup

- **Edit configs by hand** (or use an agent not in the toggle list): see
  [`AGENT_REGISTRATION.md`](AGENT_REGISTRATION.md) for the exact per-agent entry
  shapes — they differ in subtle ways (Claude needs `"type":"stdio"`, Gemini
  needs *no* `type`, OpenCode uses the `mcp` key with an array `command`, Codex
  uses `[mcp_servers.fenceymd]`).
- **HTTP-native agents** can skip the bridge and point straight at
  `http://127.0.0.1:<port>/mcp` — but the port is random per launch, so a
  hardcoded URL goes stale on restart. The bridge is the durable choice.
- **Port file location** (if you need it):
  `~/Library/Application Support/com.fenceymd.app/port` on macOS
  (`%APPDATA%\com.fenceymd.app\port` on Windows; `$XDG_DATA_HOME/com.fenceymd.app/port` on Linux).

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Agent doesn't list `fenceymd` | You didn't restart the agent after toggling — start a fresh session. |
| Tools error with "no book is currently open" | Open a folder in Fenceymd first. |
| Bridge errors / "is Fenceymd running?" | The app isn't open (no port file). Launch it and retry. |
| Worked yesterday, fails after a restart | If you used a hardcoded HTTP URL, the port changed — switch to the bridge (the Settings toggle does this). |
| `capture_screenshot` says "window not found" | The window is minimized or hidden — restore it, then retry. |
| Multiple Fenceymd windows | Each writes its own `port-<pid>` file; the bare `port` file tracks the most recent one. |

For a guided tour, the bundled demo book's **chapter 13 — Agent Control**
walks through all of this hands-on.
