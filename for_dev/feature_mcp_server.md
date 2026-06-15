# MCP server

## Vision & DoD (5W1H)

**What.** A local MCP-over-HTTP server runs inside the FenceyMD app. While the app is open, an AI agent (Claude Code, Antigravity, OpenCode, Gemini CLI, Codex) can talk to the reader over a JSON-RPC endpoint exposed at a random `127.0.0.1` port. The agent gets tools to navigate, read content, see the user's selection, and (in v2) edit chapters.

**Why.** Long-form content workflow is half reading, half writing. The user can ask an agent "summarize chapter 3", "find every mention of the GoF pattern", "rewrite this section to be more concise", "add a Mermaid diagram for the auth flow". The agent needs *structured* access to the book — not screen scraping, not asking the user to copy-paste. MCP is the protocol that makes this clean.

**Who.** Developers using FenceyMD with an AI agent. The agent is configured (via its MCP config) to discover FenceyMD's port file and connect.

**When.** The app starts → the MCP server starts → a random port is bound → a port file is written to `<app_data_dir>/port`. The agent reads the port file on each connection (the port changes on each app launch).

**Where.** The server is in the Rust process (axum + tokio). It binds to `127.0.0.1` (loopback only, never exposed to the network). The port file is the discovery mechanism. The agent connects via either:
- **Streamable HTTP MCP** (Antigravity, OpenCode, Gemini CLI) — directly to `http://127.0.0.1:<port>/mcp`.
- **stdio bridge** (Claude Code, Codex) — via the `--mcp-bridge` subcommand of the .app binary, which reads the port file, opens one HTTP connection, and relays JSON-RPC frames between stdio and HTTP.

**How (acceptance / DoD).**
- The MCP server starts on app launch and binds a random 127.0.0.1 port.
- The port file is written atomically (no half-reads).
- A multi-instance window gets a per-pid port file (`port-<pid>`) so a second window doesn't clobber the first.
- On graceful close, the port file is removed.
- On startup, the existing port file is checked for staleness; if the previous pid is dead, the file is overwritten.
- The seven tools work: `open_file`, `get_current_chapter`, `get_chapter_content`, `get_selected_text`, `get_book_toc`, `capture_screenshot`, `get_debug_log`.
- Path traversal is rejected with error code -32001.
- All file operations are bounded to the active folder.
- The connection is local-only; no network egress.
- The agent can discover the port without the user configuring a number (it's in the port file).

---

## How we implemented it

**What.** A Rust axum HTTP server with a single endpoint (`/mcp`) accepting POSTs of JSON-RPC 2.0 frames. The server reads state from Tauri's managed state (`McpState { active_folder_root, active_folder_meta, view, session_context }`) and emits Tauri events back to the JS side (`mcp-navigate`, `mcp-folder-changed`, `mcp:session-context`).

**Why this shape.** axum gives us a battle-tested HTTP server in Rust. JSON-RPC 2.0 is the wire format the MCP spec mandates. Tauri events let the JS side react to MCP calls (e.g. when the agent calls `open_file`, the JS side navigates to the chapter).

**When.** The server starts inside the .app's `setup` hook (Tauri 2). The port file is written immediately after binding. The server runs for the lifetime of the app.

**Where.**
- `src-tauri/src/mcp.rs` — the whole module (~1800 lines).
- `src-tauri/src/main.rs` — `mod mcp;` and `mcp::start(app)` in the `setup` hook.
- `<app_data_dir>/port` — the port file (atomic write).
- `<app_data_dir>/port-<pid>` — per-pid instance file.

**How (tech).**
- **HTTP**: `axum 0.7` with `tower`. One route: `POST /mcp`. The handler parses JSON-RPC, dispatches to `handle_tools_call` or `handle_initialize` / `handle_tools_list`.
- **Async runtime**: `tokio 1` with `net`, `macros`, `rt-multi-thread`, `sync`, `time`. The server runs in `tauri::async_runtime::spawn`.
- **Port discovery**: `pick_port()` tries 3 random ephemeral ports (49152-65535), binds a `TcpListener` to test, then `axum::serve` runs on it. Port is written to `<app_data_dir>/port` via atomic rename.
- **State**: `tauri::Manager` is used to manage `McpState` (a `Mutex`-protected struct with the active folder, view, session context). Tauri commands (`mcp_update_view_state`, `mcp_set_active_folder`, etc.) are the bridge for the JS side to update this state.
- **Lifecycle**: `RunEvent::ExitRequested` (the graceful close) calls `cleanup_port_file` which removes both `port` and `port-<pid>`. On startup, `is_pid_alive` checks the existing port file's pid; if dead, log "stale port file found, will overwrite" and re-bind.
- **Multi-instance**: each FenceyMD window has its own process (Tauri is single-window per process). Each writes `port-<pid>`; the bare `port` is the most-recent.
- **Bridge** (the `--mcp-bridge` subcommand): a Rust function (`run_bridge`) that reads stdin, writes stdout, reads the port file, opens one persistent HTTP connection, and relays JSON-RPC frames. No Node.

**Tools (the surface an agent sees).**
- `open_file(path, session_context?)` — navigate. `path` can be absolute or relative-to-active-folder. Absolute paths auto-resolve to a recent folder (recents search → walk-up parents). The new `mcp-folder-changed` event tells the JS side to re-scan + populate folderMeta + navigate (the JS combines all three because they need to be ordered).
- `get_current_chapter()` — `{ open, path, scroll_position, word_count, reading_time_min, content_preview }`.
- `get_chapter_content(path)` — full markdown, capped at 1 MB. Same path resolution as `open_file`.
- `get_selected_text()` — `{ text, anchor }` (text is the user's current selection; anchor is the `data-md-anchor` of the enclosing block).
- `get_book_toc()` — flat list of `{ path, title, group, word_count }` mirroring the sidebar.
- `capture_screenshot()` — `{ format: 'png', width, height, bytes, data_b64 }`. The agent can decode base64 and pass the image to a vision LLM.
- `get_debug_log(tail?, contains?, since_ts?)` — tail/filter the file-based activity log.

**Error codes.**
- `-32000` ERR_TOOL — generic tool failure.
- `-32001` ERR_PATH_NOT_IN_BOOK — path traversal.
- `-32002` ERR_NO_BOOK_OPEN — no active folder.
- `-32003` ERR_CONTENT_TOO_LARGE — file > 1 MB.
- `-32600/-32601/-32602/-32603/-32700` — standard JSON-RPC.

**Gotchas.**
- The `platform` is a function in `node:os`, not a string — caught when the bridge's first implementation was comparing `platform === 'darwin'` and always falling through to the Linux branch. The integration test only catches this if it exercises the actual platform branch; ours used `FENCEYMD_PORT_DIR` and bypassed it. End-to-end testing with the bundled bridge caught it.
- Path-traversal is a hot attack surface. The `safe_resolve_in_folder` function does `canonicalize → starts_with check`; never trust the agent's path verbatim.
- The `mcp-folder-changed` event is the *combined* event for "agent asked for an absolute path; we switched folders." The JS handler does rescan + navigate in one go to avoid a race where the navigate lands on an empty folderMeta. This was a real bug in v1.0.
- The Rust bridge is preferred over a Node bridge because it removes a Node dependency from the install path. The bridge is the same binary as the .app; it just runs in a subcommand mode.
- The active folder in `McpState` is updated by the JS side via `mcp_set_active_folder` (called after every successful `openScanResult`).
