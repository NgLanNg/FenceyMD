---
title: Agent Control
---

# Agent Control

FenceyMD runs a local MCP server while it's open, so an AI agent that
speaks MCP can **read and navigate** what you're reading — open a chapter,
pull its content, see your selection, grab a screenshot of the view. It is
**read-only**: the agent can observe and navigate, but cannot edit your files
through the server. Everything stays on `127.0.0.1` — no network egress, no
telemetry.

You don't need this chapter to *use* FenceyMD. This is for when you want an
agent — Claude Code, Gemini, OpenCode, Codex, or anything that speaks MCP — to
follow along in the reader while it works.

## How it works

FenceyMD runs a small **local HTTP server** while it's open. The
server binds a random `127.0.0.1` port (somewhere in 49152–65535),
writes that port to a JSON file in the app data dir, and exposes
seven MCP tools over plain JSON-RPC 2.0. No network egress. No
telemetry. Binds to localhost only.

```
┌─────────────────────────────────────┐
│  FenceyMD (open, folder loaded)     │
│   • Rust HTTP server on 127.0.0.1   │
│   • writes port to ~/Library/.../port│
└──────────────┬──────────────────────┘
               │
       fenceymd --mcp-bridge
       (reads port file, POSTs JSON-RPC
        over stdin/stdout, no Node)
               │
       ┌──────┴──────┐
       │             │
   your agent    your agent
   (stdio)       (HTTP)
```

Most agents (Claude Code, Gemini, OpenCode, Codex) speak stdio.
FenceyMD's own binary doubles as a **bridge** that translates
stdio JSON-RPC to the local HTTP server — no Node, no extra
binary, no dependencies. Agents that speak HTTP directly
(Antigravity, anything Streamable-HTTP native) can skip the
bridge.

## The seven tools

| Tool                  | What the agent gets back                                             |
|-----------------------|----------------------------------------------------------------------|
| `open_file`           | `{ ok, active_folder, resolved_path }`. Path can be relative to the open folder, or absolute (auto-resolves to the right book). |
| `get_current_chapter` | `{ open, path, scroll_position, word_count, reading_time_min, content_preview }`. What the reader is showing right now. |
| `get_chapter_content` | The full markdown source of a chapter, up to 1 MB. Capped to keep the agent from accidentally pulling in a giant file. |
| `get_selected_text`   | The text the user has highlighted, with the `data-md-anchor` of the enclosing block. Empty when nothing is selected. |
| `get_book_toc`        | The flat list of every chapter in the open folder: `{ path, title, group, word_count }`. |
| `capture_screenshot`  | The current FenceyMD window as a base64 PNG (downscaled to ≤1600px longest edge). Decodes into an image for a vision LLM. |
| `get_debug_log`       | Recent activity-log lines. Optional args: `tail` (default 100, max 1000), `contains` (substring filter), `since_ts` (epoch-seconds floor). |

Writes are bounded to the open folder. Path traversal
(`../etc/passwd`, absolute escapes out of the folder) is rejected
with a `-32001` JSON-RPC error before any disk access.

## A 60-second tour

FenceyMD is running. The demo folder is open. The port file says
`60872`. The bridge handles the port lookup for you:

```bash
$ echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
    | fenceymd --mcp-bridge
```

You get back a one-line JSON list of the seven tools above. That
single handshake is the whole protocol — every other frame looks
the same.

To peek at what's currently open:

```bash
$ echo '{
    "jsonrpc":"2.0","id":2,"method":"tools/call",
    "params":{"name":"get_current_chapter","arguments":{}}
  }' | fenceymd --mcp-bridge
```

```json
{
  "result": {
    "path": "08-excalidraw.md",
    "scroll_position": 0.42,
    "word_count": 1180,
    "reading_time_min": 6,
    "content_preview": "Inline Excalidraw becomes a real drawing canvas…"
  }
}
```

The agent now knows what you're reading — the chapter path, the
scroll position, the word count, and a 500-char preview.

To navigate, send `open_file` with the relative path:

```bash
$ echo '{
    "jsonrpc":"2.0","id":3,"method":"tools/call",
    "params":{"name":"open_file","arguments":{"path":"02-navigation.md"}}
  }' | fenceymd --mcp-bridge
```

The reader jumps to chapter 2.

To read any chapter's full source:

```bash
$ echo '{
    "jsonrpc":"2.0","id":4,"method":"tools/call",
    "params":{"name":"get_chapter_content","arguments":{"path":"02-navigation.md"}}
  }' | fenceymd --mcp-bridge
```

That's the whole handshake. Everything else builds on top of it.

## Wiring an agent (the 30-second version)

The fastest path is **Settings → AI agent control** in FenceyMD.
Flip the toggle for your agent — FenceyMD writes the right entry
into that agent's own config file, idempotently and without
touching anything else. Restart the agent (start a fresh session)
and you're connected.

If you'd rather edit by hand, all four agents point at the same
native `--mcp-bridge` subcommand:

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

The per-agent files and the subtle schema differences are in
`docs/AGENT_REGISTRATION.md`:

- **Claude Code** — `~/.claude.json`, `mcpServers`, `type:"stdio"`.
- **Gemini CLI / Antigravity** — `~/.gemini/settings.json`, no `type` field.
- **OpenCode** — `~/.config/opencode/opencode.json`, `mcp` key (not `mcpServers`), `type:"local"`, command is a single array.
- **Codex** — `~/.codex/config.toml`, `[mcp_servers.fenceymd]`.

All four point at the same `fenceymd --mcp-bridge` entry, so the
config survives app restarts (the bridge rediscovers the port on
every connection).

## What the agent can and can't do

**Can.**

- Open any chapter in the currently-open folder. Relative or
  absolute paths; absolute paths auto-resolve to the right book.
- Read the full content of any chapter (capped at 1 MB).
- See what's on screen and what you've highlighted.
- Get the table of contents of the open book.
- Grab a PNG screenshot of the live view as a base64 blob (the
  window must be visible; an occluded window can capture blank).
- Read the recent activity log (`get_debug_log`).
- Pass an optional `session_context` object with `open_file`; it's
  stored and emitted to the UI, nothing more today.

**Can't.**

- Write or edit anything. Every tool is read/navigate/observe;
  there is no write tool over MCP.
- Reach outside the open folder. Path traversal (`../etc/passwd`,
  absolute escapes) is rejected with a `-32001` error before the
  disk is touched.
- Spawn a second instance silently. Each FenceyMD window gets
  its own `port-<pid>` file, so a second `open` doesn't clobber
  the first.

## The bridge, in detail

`fenceymd --mcp-bridge` is a small Rust binary that does three
things:

1. Reads the port file the running app wrote on startup.
2. Reads newline-delimited JSON from **stdin**.
3. POSTs each frame to `http://127.0.0.1:<port>/mcp` and
   writes the response (also newline-delimited JSON) to
   **stdout**.

EOF on stdin → clean exit. Connection refused on dial → a
structured JSON-RPC error on stdout (not a hang). The agent sees
a real error rather than a frozen prompt. This is the standard
MCP-over-stdio contract.

Because the bridge looks up the port file on every connection,
a single static config entry keeps working across app restarts.
When you update the app and the binary moves, FenceyMD
self-heals the registered configs on next launch.

## See it work

With FenceyMD running, pipe a few frames through the native
bridge in one go (newline-delimited JSON on stdin):

```bash
$ fenceymd --mcp-bridge <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
{"jsonrpc":"2.0","id":2,"method":"tools/call",
 "params":{"name":"get_book_toc","arguments":{}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call",
 "params":{"name":"get_current_chapter","arguments":{}}}
EOF
```

You should see three JSON-RPC responses: the tool list, the
14-chapter TOC, and a preview of whatever chapter is open. That's
the whole handshake. Everything else builds on top of it.

If you want the raw HTTP path (no bridge), the same frames work
as `curl -X POST` against `http://127.0.0.1:<port>/mcp` — read
the port from `~/Library/Application Support/com.fenceymd.app/port`
on macOS (or the equivalent on Linux/Windows; see
`docs/MCP_SETUP.md`).

## A note on trust

The whole point of an agent reading your book is that the
agent shouldn't be able to do things you wouldn't do. The MCP
server enforces that: it runs as a child of the desktop app,
it's bound to `127.0.0.1`, it has no network egress, and the
path traversal guard is in Rust (not JavaScript, where it could
be bypassed by a hostile page).

If the agent is malicious, the worst it can do over MCP is *read*
Markdown files in the folder you opened (and screenshot the
window) — the tools are read-only, so it can't write or escape
the folder. That's the threat model.

## Scope today

Today the tools are **read + navigate + observe** (plus a screenshot
and the activity log). There is no agent-driven *editing* over MCP
yet — an agent can't change your files through the server.

For the start-here setup guide and per-agent config, see
`docs/MCP_SETUP.md` and `docs/AGENT_REGISTRATION.md`.
