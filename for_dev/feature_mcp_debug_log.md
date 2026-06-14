# MCP tool: get_debug_log

## Vision & DoD (5W1H)

**What.** The MCP `get_debug_log` tool returns the recent entries from the file-based activity log (`<app_data_dir>/debug.log` on macOS, `%APPDATA%\com.fenceymd.app\debug.log` on Windows, `$XDG_DATA_HOME/com.fenceymd.app/debug.log` on Linux). Optional arguments: `tail` (number of trailing lines, default 100, max 1000), `contains` (substring filter), `since_ts` (epoch seconds, only return entries at-or-after this time).

**Why.** When the user reports a bug, the agent often needs the same context the user can read in the log. Without `get_debug_log`, the agent has to ask "what did the app do?" and the user has to copy-paste from a file. With it, the agent can self-diagnose.

**Who.** Any agent debugging user-reported issues. Also useful for verification scripts: an agent can assert that a specific log line was emitted in response to a tool call.

**When.** On agent call. The log read is fast (~5 ms for a 10k-line log).

**Where.** `src-tauri/src/mcp.rs#tool_get_debug_log`. Reads the same file the user can see in their app data dir.

**How (acceptance / DoD).**
- Returns `{ path, lines, total_matched, returned, truncated }`.
- `lines` is an array of strings, most-recent last.
- `truncated` is true if more lines matched than `tail` (the result is the trailing N).
- `contains` filter is case-sensitive substring match.
- `since_ts` filter is inclusive (entries logged at-or-after this time).
- A missing log file (no activity yet) returns an empty list, not an error.
- The total number of matching lines (before tail truncation) is in `total_matched`.

---

## How we implemented it

**What.** A Rust function that:
1. Reads the debug log file as a single string.
2. Splits on lines.
3. Filters by `contains` (substring) and `since_ts` (leading epoch in `[<ts>]` format).
4. Takes the trailing N lines per `tail`.
5. Returns the result.

**Why this shape.** The log file is small (typically < 1 MB) and the filtering is cheap. Reading the whole file each call is fine; we don't need an index.

**When.** On agent call. The log file is read synchronously in the handler.

**Where.**
- `src-tauri/src/mcp.rs#tool_get_debug_log` — the tool.
- `src-tauri/src/mcp.rs#parse_log_ts` — the helper to extract the leading epoch.
- `src-tauri/src/mcp.rs#GetDebugLogArgs` — the input struct.
- `<app_data_dir>/debug.log` — the log file.
- `src-tauri/src/main.rs#debug_log_path` — the file path helper (already public).
- `src-tauri/src/main.rs#log_from_rust` — every Rust call writes here.

**How (tech).**
- **Read**: `std::fs::read_to_string(&log_path)`. A missing file is treated as empty (the agent doesn't fail just because nothing's been logged yet).
- **Line format**: each line is `[<epoch_secs>] <rest>`. `parse_log_ts` extracts the leading epoch from the `[N]` prefix.
- **Filters**: applied per-line. `contains` is a substring match; `since_ts` parses the leading epoch and compares (or keeps the line if the parse fails — fail-open).
- **Tail truncation**: keep the last `tail` lines. `truncated = true` if more matched than `tail`.
- **Response shape**: `{ path, lines, total_matched, returned, truncated }`. The path is included so the agent knows what it just read (and can correlate with the user's localStorage paths).

**Gotchas.**
- A very long log file (> 100 MB) would be slow to read each call. We don't cap the file size, but a typical session produces < 1 MB. If this becomes a problem, we'd add a ring buffer or a "since offset" parameter.
- The log file format is `[<epoch>] <text>`. If a line doesn't start with `[`, it's kept (fail-open on parse). Malformed lines never block the agent.
- The `since_ts` filter is inclusive (>=). The agent should pass the current epoch + 1 to skip lines logged before the call.
- The `path` field in the response is the OS-native path (not the in-app form), so the agent can hand it to the user as-is.
