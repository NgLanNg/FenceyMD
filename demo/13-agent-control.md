---
title: Agent Control
---

# Agent Control

This is the chapter the rest of the book was written for.

FenceyMD is **AI-native**. The same Rust commands the UI uses —
open a chapter, scroll, edit, save — are exposed to an AI agent
over a local MCP server. Hand the agent the folder, tell it what
you want, and it works. The same guarantees the UI gives you
(writes stay inside the folder, scroll position preserved, no
telemetry) carry over to the agent.

You don't need this chapter to *use* FenceyMD. The app still
reads folders like a regular reader. This is for when you want
an agent — Claude Code, Antigravity, OpenCode, anything that
speaks MCP — to drive the app for you.

## What ships today

A Rust HTTP server lives inside the `.app` and starts when the
app starts. It binds a random `127.0.0.1` port (49152–65535),
writes the port to a `port` file in the app data dir
(`~/Library/Application Support/com.fenceymd.app/port` on macOS),
and exposes seven MCP tools. That's the whole surface.

| Tool | What it does |
| --- | --- |
| `open_file` | Navigate the reader to a chapter by path. |
| `get_current_chapter` | What the reader is showing right now — path, scroll, a 500-char preview. |
| `get_chapter_content` | The full markdown of any chapter, up to 1 MB. |
| `get_selected_text` | The text the user has highlighted, with the anchor it's anchored to. |
| `get_book_toc` | The flat list of every chapter in the open folder. |
| `capture_screenshot` | The current window as a base64 PNG (downscaled to ≤1600px) — for a vision-capable LLM. |
| `get_debug_log` | Recent activity-log lines (`tail` / `contains` / `since_ts` filters). |

Everything is local. No token leaves your machine. The server
is plain JSON-RPC 2.0 over HTTP — exactly what the MCP spec
asks for.

## A 30-second tour

Assume FenceyMD is running with the `demo/` folder open, and
port `60872` is what's in the port file.

```bash
$ curl -s -X POST -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
    http://127.0.0.1:60872/mcp
```

The response is the tool list. Pick one:

```bash
$ curl -s -X POST -H "Content-Type: application/json" \
    -d '{
      "jsonrpc":"2.0","id":2,"method":"tools/call",
      "params":{"name":"get_current_chapter","arguments":{}}
    }' \
    http://127.0.0.1:60872/mcp
```

You get back something like:

```json
{
  "result": {
    "path": "08-excalidraw.md",
    "scroll_position": 0.42,
    "preview": "Inline Excalidraw becomes a real drawing canvas…"
  }
}
```

That's it. The agent now knows what you're reading.

To navigate:

```bash
$ curl -s -X POST -H "Content-Type: application/json" \
    -d '{
      "jsonrpc":"2.0","id":3,"method":"tools/call",
      "params":{
        "name":"open_file",
        "arguments":{"path":"02-navigation.md"}
      }
    }' \
    http://127.0.0.1:60872/mcp
```

The reader jumps to chapter 2.

## The bridge

Some agents (Claude Code and most CLI agents) only speak stdio,
not HTTP. For those, FenceyMD's own binary doubles as a bridge:
run it with `--mcp-bridge` and it translates a stdio JSON-RPC
stream to the local HTTP server, rediscovering the port on each
connection. No Node, no extra binary, no dependencies.

The easiest way to wire it up is **Settings → AI agent control**:
flip the toggle for your agent and FenceyMD writes the right
entry into that agent's own config. If you'd rather do it by
hand, a stdio agent's entry looks like:

```json
{
  "mcpServers": {
    "fenceymd": {
      "type": "stdio",
      "command": "/Applications/FenceyMD.app/Contents/MacOS/fenceymd",
      "args": ["--mcp-bridge"]
    }
  }
}
```

Because the bridge looks up the port file every time, a single
static config keeps working across app restarts.

## What the agent can and can't do

**Can.**

- Open any chapter in the currently-open folder.
- Read the full content of any chapter.
- See what's on screen and what the user highlighted.
- Get the table of contents.
- Pass a `session_context` along with `open_file` so a future
  sidebar chat knows which agent (and which session of that
  agent) the request came from.

**Can't.**

- Read or write anything outside the open folder. Path-traversal
  (`../etc/passwd`, absolute paths) is rejected with a
  `-32001` JSON-RPC error before the disk is touched.
- Bypass the editor's save flow. The same `write_file` Rust
  command the UI uses is what the agent would call in Phase 2.
- Spawn a second instance silently. Each FenceyMD window gets
  its own `port-<pid>` file so a second `open` doesn't clobber
  the first.

## How to point an agent at FenceyMD

The fastest path is the **Settings → AI agent control** toggle —
it writes the config for you. If you prefer to edit by hand,
`docs/AGENT_REGISTRATION.md` has the exact per-agent shapes; they
differ in subtle ways, so match them exactly:

- **Claude Code** — `~/.claude.json`, `mcpServers`, `type:"stdio"`.
- **Gemini CLI / Antigravity** — `~/.gemini/settings.json`, no `type` field.
- **OpenCode** — `~/.config/opencode/opencode.json`, `mcp` key, `type:"local"`.
- **Codex** — `~/.codex/config.toml`, `[mcp_servers.fenceymd]`.

All four point at the same native `--mcp-bridge` subcommand, so
the entry survives app restarts.

## What's not here yet

This is **Phase 1**. It gets the agent on the same level as the
user: read, navigate, observe.

What it doesn't have yet — and what's planned:

- **Phase 2** — an in-app sidebar chat. The agent gets its own
  pane, with scrollback, tool-call traces, and the
  `session_context` plumbing already wired. Five agents (Claude,
  Gemini, Codex, OpenCode, Antigravity) are being designed for
  parallel support.
- **v2 (anchor-based edit)** — the user points at a block, the
  agent returns a surgical diff for that block, the editor
  applies it without touching the rest. This is the round-trip
  the *book* was written for: read with the agent, edit with
  the agent, never leave the chapter.

Both phases are scoped in `vault/plan/20260613_mcp_phase2_design.md`
and `vault/plan/20260613_anchor_edit_design.md`. The anchor
infrastructure that v2 needs is shipping in v1.1 (#23 on the
roadmap) — every block will have a stable `data-md-anchor`.

## A note on trust

The whole point of an agent reading your book is that the agent
shouldn't be able to do things you wouldn't do. The MCP server
enforces that: it runs as a child of the desktop app, it's bound
to `127.0.0.1`, it has no network egress, and the path
traversal guard is in Rust (not JavaScript, where it could be
bypassed by a hostile page).

If the agent is malicious, the worst it can do is read and edit
files in the folder you opened. That's the threat model. The
server doesn't try to be more clever than that — but it doesn't
try to be less, either.

## See it work

With FenceyMD running, pipe a couple of frames through the
native bridge — `fenceymd --mcp-bridge` reads the port file and
forwards them to the live server:

```bash
$ "/Applications/FenceyMD.app/Contents/MacOS/fenceymd" --mcp-bridge <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
{"jsonrpc":"2.0","id":2,"method":"tools/call",
 "params":{"name":"get_book_toc","arguments":{}}}
EOF
```

You should see two JSON-RPC responses: the tool list, then the
14-chapter TOC. That's the whole handshake. Everything else
builds on top of it.

If you're running from source (not the bundled .app), use the
built binary: `src-tauri/target/release/fenceymd --mcp-bridge`.
