# MCP Setup — let an AI agent drive FenceyMD

A 60-second guide to wiring FenceyMD's local MCP server into an AI coding
agent (Claude Code, Gemini, Codex, OpenCode, Antigravity, …).

> **Everything stays on your machine.** FenceyMD binds a `127.0.0.1` port
> only, the server has no network egress, and the tools are **read-only** —
> an agent can read and navigate the folder you opened, but can't write or
> edit through the server. No tokens, no telemetry.

---

## 0. What you need

- FenceyMD **1.0 or newer** installed and runnable. (Open the app once so
  it can install the `fenceymd` CLI on your PATH — see step 3.)
- An AI agent that speaks **MCP over stdio** (Claude Code, Gemini CLI,
  OpenCode, Codex) **or** over **Streamable HTTP** (Antigravity,
  anything HTTP-native).
- A folder of Markdown open in FenceyMD. The agent can only see what's
  in the open folder.

---

## 1. Open a folder in FenceyMD

Launch the app (**`open /Applications/FenceyMD.app`** on macOS, double-click
on Windows/Linux) and open a folder. The MCP server starts at the same
moment — no setting to flip, no button to press. The server writes a
**port file** with the random local port it's listening on:

| OS      | Port file location                                                |
|---------|-------------------------------------------------------------------|
| macOS   | `~/Library/Application Support/com.fenceymd.app/port`           |
| Linux   | `$XDG_DATA_HOME/com.fenceymd.app/port` (default `~/.local/share/...`) |
| Windows | `%APPDATA%\com.fenceymd.app\port`                                |

You don't need to read or edit the port file. The bridge does it for you.

---

## 2. Wire your agent to FenceyMD

The fastest way is the **Settings → AI agent control** toggle in FenceyMD.
Open Settings, find your agent in the list, flip its toggle. FenceyMD
writes the right MCP entry into that agent's own config file, idempotently
and without touching anything else.

| Agent                       | Config file                              | Restart |
|-----------------------------|------------------------------------------|---------|
| **Claude Code**             | `~/.claude.json`                          | fresh `claude` session |
| **Gemini CLI**              | `~/.gemini/settings.json`                 | fresh `gemini` session |
| **Antigravity**             | `~/.gemini/settings.json`                 | restart the IDE        |
| **OpenCode**                | `~/.config/opencode/opencode.json`        | fresh `opencode` session |
| **Codex**                   | `~/.codex/config.toml`                    | fresh `codex` session   |

> **Restart the agent.** Agents read their MCP config only at session
> start, so an already-running `claude` / `codex` / `gemini` / `opencode`
> session won't see FenceyMD until you start a fresh one. **This is
> the #1 cause of "it doesn't work."**

If your agent isn't in the toggle list (or you want to edit configs by
hand), see **[`AGENT_REGISTRATION.md`](AGENT_REGISTRATION.md)** for the
exact per-agent schema.

---

## 3. Install the `fenceymd` CLI (if you want to test from a terminal)

The Settings toggle writes an entry that points at the bare name `fenceymd`
on your PATH. FenceyMD installs a symlink named `fenceymd` into the first
writable directory of:

1. `/opt/homebrew/bin` (Apple Silicon Homebrew)
2. `/usr/local/bin` (Intel Homebrew / on macOS's default PATH)

Both are on your shell's PATH, so the command is immediately usable. (We do
**not** fall back to `~/.local/bin`/`~/bin` — they aren't on macOS's default
PATH, so installing there would leave `fenceymd` present-but-not-found. If
neither dir above is writable — a Mac with no Homebrew — Settings shows the CLI
as not installed rather than installing it somewhere you can't reach.)

The install happens automatically on first launch of a release build, and
you can re-run it any time from **Settings → AI agent control → Install
CLI**. The CLI itself exposes a repair subcommand too — once the
`fenceymd` symlink is on PATH (even from a stale install), running
`fenceymd --install-cli` re-creates it pointing at the current `.app`.

If the bare `fenceymd` command is missing entirely (a clean install
where the first-launch hook didn't fire), open the .app once and the
auto-install runs; otherwise right-click the .app in Finder, choose
**Open**, and the install will fire on the next launch.

After install, `which fenceymd` should resolve to the symlink. (The
`Settings` toggle also re-points the symlink when you update the app, so
it always points at the running `.app`.)

---

## 4. Verify it works

### 4a. From your agent

Open a fresh agent session and ask it something the MCP can answer:

> _"List the chapters in the folder I have open."_

The agent should call `get_book_toc` and return the chapter list. If it
says "I don't see an MCP server called fenceymd", the agent didn't read
the updated config — start a fresh session.

In Claude Code, you can also run **`/mcp`** — `fenceymd` should appear in
the list with the 7 tools.

### 4b. From a terminal (the smoke test)

The native bridge handles the port-file lookup for you, so you don't need
to know the port:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
  | fenceymd --mcp-bridge
```

A one-line JSON response listing the 7 tools. If you see
`"bridge: connect 127.0.0.1:60872: Connection refused"`, FenceyMD isn't
running — launch it and try again. The bridge surfaces a clean JSON-RPC
error; the agent sees a real error rather than a hang.

---

## The 7 tools at a glance

| Tool                     | What it does                                                         |
|--------------------------|----------------------------------------------------------------------|
| `open_file`              | Navigate the reader to a chapter by path. Accepts relative or absolute paths; absolute paths auto-resolve to the right book folder. |
| `get_current_chapter`    | What the reader is showing right now — path, scroll fraction, a 500-char preview, word count, reading time. |
| `get_chapter_content`    | The full Markdown of a chapter (≤ 1 MB). Capped to protect the agent from accidentally pulling in a giant file. |
| `get_selected_text`      | The text the user has highlighted, with the block anchor it's anchored to. Empty when nothing is selected. |
| `get_book_toc`           | The flat list of every chapter in the open folder (`path`, `title`, `group`, `word_count`). |
| `capture_screenshot`     | The current FenceyMD window as a base64 PNG (downscaled ≤1600px). Decodes into an image you can hand to a vision LLM. |
| `get_debug_log`          | Recent activity-log lines — `tail` (default 100), `contains` substring filter, `since_ts` epoch-seconds filter. Use it to see what the app is doing. |

Writes are bounded to the open folder; path traversal (`../…`, absolute
escapes) is rejected in Rust before any disk access.

---

## HTTP transport (alternative to the bridge)

Agents that speak **Streamable HTTP MCP** can skip the bridge and POST
directly to `http://127.0.0.1:<port>/mcp`. The catch: the port is random
per launch, so a hardcoded URL goes stale on every restart. The bridge
is the durable choice — it reads the port file on every connection.

If you do use HTTP, read the port from the file above and substitute it
into `http://127.0.0.1:<port>/mcp`.

---

## Troubleshooting

| Symptom                                                       | Fix                                                                                          |
|---------------------------------------------------------------|---------------------------------------------------------------------------------------------|
| Agent doesn't list `fenceymd` after you toggled it on          | You didn't restart the agent. Start a fresh `claude` / `codex` / `gemini` / `opencode` session. |
| Tool errors with `"no book is currently open"`                 | Open a folder in FenceyMD first.                                                             |
| `fenceymd: command not found` in a terminal                   | The CLI didn't auto-install. Open **Settings → AI agent control → Install CLI** in the running app, or run the .app's binary once with `--install-cli` — that's the repair hatch when PATH is broken. |
| Bridge error `"connect 127.0.0.1:60872: Connection refused"` | FenceyMD isn't open. Launch the app, wait ~1s, retry.                                       |
| Worked yesterday, fails after a restart                       | You used a hardcoded HTTP URL and the port changed. Switch to the bridge (the Settings toggle does this). |
| `capture_screenshot` says "window not found"                  | The window is hidden behind another app or minimized. The tool self-activates the window, so it usually resolves itself; if not, click FenceyMD to focus it and retry. (~500ms latency per call, due to the 300ms SkyLight activation settle.) |
| Multiple FenceyMD windows                                     | Each writes its own `port-<pid>` file; the bare `port` file tracks the most recent one. Target a specific window by reading `port-<pid>` directly. |
| Agent says the response is a hallucination / wrong            | Make sure you opened the *folder* you mean (not the demo), then re-run. Tool responses reflect the **currently open** folder, not the whole filesystem. |

For a guided hands-on walkthrough, open the bundled demo book's
**chapter 13 — Agent Control**.
