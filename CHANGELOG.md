# Changelog

All notable changes to FenceyMD are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] — 2026-06-15

### Added
- **`fenceymd` CLI on PATH.** The app makes its binary runnable as `fenceymd`
  from a terminal by symlinking it into the first writable well-known bin dir
  (Homebrew's `bin`, `/usr/local/bin`, then `~/.local/bin`/`~/bin`). Installed
  automatically on **first launch** (release builds; a `.dmg` drag can't run
  code, so first launch is the hook), plus a **Settings → AI agent control →
  Install CLI** button and an `--install-cli` flag on the app binary for manual
  repair — run it by full path the first time
  (`/Applications/FenceyMD.app/Contents/MacOS/fenceymd --install-cli`), since
  `fenceymd` isn't on PATH until the install runs. Once the CLI is present, agent
  registrations use a clean `command: "fenceymd"` instead of the deep
  `…/FenceyMD.app/Contents/MacOS/fenceymd` path, and `refresh_registrations`
  upgrades older absolute-path entries on launch. New `cli.rs` module
  (6 unit tests covering symlink install, idempotency, stale-symlink replace,
  never-clobber-a-real-file, candidate fall-through). Verified live: `fenceymd
  --mcp-bridge` round-trips `tools/list` against the running app.
- **MCP setup guide** (`docs/MCP_SETUP.md`) — a step-by-step "start here" walkthrough
  (run with a folder open → toggle the agent in Settings → restart it → verify),
  the 7-tool table, a terminal smoke test, and troubleshooting. Cross-linked from
  the README and `AGENT_REGISTRATION.md` (which remains the per-agent schema
  reference). Doc corrections in the same pass: `feature_mcp_server.md`
  (five → seven tools), `feature_mcp_capture_screenshot.md` (window match
  `md reader` → `fenceymd`, ≤1600px downscale, occluded-window blank-capture
  limitation), `feature_sanitization.md` (actual exported fn names).
- **One-click agent registration (Settings → AI agent control).** A per-agent
  toggle writes FenceyMD's `fenceymd` MCP entry into each agent's own config
  — Claude Code (`~/.claude.json`, `type:"stdio"`), Gemini CLI / Antigravity
  (`~/.gemini/settings.json`, no `type`), OpenCode
  (`~/.config/opencode/opencode.json`, `mcp` key, `type:"local"`, array
  command), Codex (`~/.codex/config.toml`, edited via `toml_edit` to preserve
  comments/siblings). Writes are idempotent and non-destructive (every other
  key preserved), keyed by the server name. Agents are pointed at the new
  **native bridge** — the app's own binary run with `--mcp-bridge` (no Node) —
  which rediscovers the random port each connection, so one static config
  survives every restart. `agents.rs` (new module) holds the verified
  per-agent schemas and a filesystem-free merge/remove core (18 unit tests);
  `main.rs` self-heals registered configs on launch if the binary moved. New
  `src-tauri/tests/bridge.rs` drives the real `--mcp-bridge` subprocess
  (round-trip + order under burst, stdout hygiene, `-32000` on connection
  refused). Restart the agent after toggling — agents read MCP config only at
  session start.
- **Native `--mcp-bridge` subcommand.** The app binary now doubles as a
  zero-dependency stdio↔HTTP MCP bridge: spawned as `fenceymd --mcp-bridge`,
  it branches before any GUI/Tauri init, resolves the endpoint
  (`--endpoint` > `MCP_BRIDGE_ENDPOINT` > port file, 3 retries), and forwards
  one JSON-RPC frame per line to the running server over a raw `TcpStream`
  (`Connection: close`, read-to-EOF). stdout carries only JSON-RPC frames; all
  logs go to stderr. Replaces the previous Node `mcp-bridge.mjs` (and its
  bundling + user-level shim), which has been removed.
- **MCP server for AI agent control (Phase 1 of MCP integration).**
  FenceyMD now runs a local MCP-over-HTTP server (axum + tokio) on a
  random localhost port. Seven tools are exposed: `open_file`
  (navigates the reader, stashes optional `session_context` for
  Phase 2), `get_current_chapter` (path + scroll + 500-char
  preview), `get_chapter_content` (full markdown, 1 MB cap),
  `get_selected_text` (`{text, anchor}`), `get_book_toc`
  (flat chapter list), `capture_screenshot` (window PNG,
  base64-encoded, downscaled to ≤1600px), and `get_debug_log`
  (recent activity-log lines). The port file in the app-data dir
  (macOS `~/Library/Application Support/com.fenceymd.app/port`;
  atomic write, multi-instance per-pid file) is the discovery
  mechanism. Stdio-only agents bridge stdio ↔ HTTP via the native
  `--mcp-bridge` subcommand. End-to-end verified against the live
  `.app`: tools return correct shapes, path-traversal blocked
  with `-32001`, navigation event round-trips JS `goChapter`.
  See `docs/AGENT_REGISTRATION.md` for per-agent config snippets
  and `vault/plan/20260611_mcp_integration.md` for the contract.
- **MCP port-file lifecycle (cleanup + stale detection).** On
  graceful app close (user clicks the red X) the `RunEvent::ExitRequested`
  handler removes both the bare `port` file (in the app-data dir) and the
  per-pid `port-<pid>` file. On startup, the new `is_pid_alive` check
  reads the existing port file and logs `stale port file found
  (pid <n> dead); will overwrite` if the previous process is
  gone, vs `port file owned by live pid <n> (likely another
  instance)` if it's a sibling FenceyMD window. New
  unit tests: `is_pid_alive_returns_false_for_zero`,
  `is_pid_alive_returns_true_for_self`,
  `is_pid_alive_returns_false_for_obviously_dead_pid` (14/14
  mcp unit tests pass).
- **Demo chapter 13 — Agent Control.** The bundled `demo/`
  tour book grew from 13 to 14 chapters. The new chapter walks
  through the MCP server, the tools, the bridge, the
  threat model, and a 30-second hands-on curl tour.
- **OS dark-mode auto-detect.** The `theme` store now seeds from
  `prefers-color-scheme` on first launch. A live `change` listener
  follows the system theme as long as the user hasn't set an explicit
  override (clicking the theme toggle writes to localStorage and locks
  the choice — Reset all prefs opts back into OS-following). E2E
  coverage: prefers-color-scheme dark/light emulation + an override-beats-OS
  probe.
- **PDF mermaid relight.** When the reader is in dark mode, every
  ` ```mermaid ` block is re-rendered in the light theme onto a CLONE of
  the chapter (the live DOM is untouched) before the PDF payload is
  shipped to Rust. Previously a dark-mode export printed diagrams as
  dark-on-white ink. The clone avoids a visible flash; the next in-app
  render re-initializes mermaid with the current theme, so the global
  `mermaid.initialize()` mutation during relight doesn't leak.

### E2E coverage (P1.2)
- **HTML fence** — navigates to the Live HTML demo chapter, asserts
  `.html-block` exists with real `<button>`/`<strong>` preserved and
  no `<pre>` leaks, and probes the sanitizer with a planted payload
  (must strip `<script>`/`onclick`/`onerror>`/`javascript:`).
- **Outline pane** — hovers the toolbar trigger, asserts
  `.outline-pane` mounts with one entry per chapter heading.
- **Narrow-width responsive** — viewport-walks Library and Settings at
  1280/1024/800/640/480px and asserts no horizontal page overflow.
  Picker is explicitly untested in `?test=1` mode (App.svelte
  auto-loads sample data; the surface is a static centered card
  visually verified during the v1 review).

### Security
- **Untrusted-content sanitization.** Chapter markdown can be shared,
  downloaded, or LLM-generated, and renders in a WebView with full IPC
  authority. A DOMPurify boundary (`src/lib/sanitize.js`) now sanitizes the
  rendered chapter body (showdown output → `{@html}`), the ` ```html ` fence,
  and the ` ```svg ` fence — stripping `<script>`, `on*` handlers, and
  `javascript:` URLs while preserving presentational markup. Mermaid is
  initialized with `securityLevel: 'strict'` (was `'loose'`).
- **PDF export sandbox.** `print_pdf` now runs headless Chrome **with** the
  sandbox by default, falling back to `--no-sandbox` only if Chrome refuses
  to start (some Linux/container hosts). Previously `--no-sandbox` was always
  passed over file:// content.
- **`write_file`** rejects a symlinked leaf that resolves outside the folder
  and bounds-checks via the nearest existing ancestor (so saves into new
  subfolders work while traversal stays blocked).
- **`open_in_external_editor`** rejects control characters in the user's
  editor-command setting (defense-in-depth; spawned without a shell).

### Fixed
- **Restored the "AI agent control" Settings section.** The per-agent toggle UI
  (and now the CLI install row) had been dropped from `Settings.svelte` during
  the rebrand, leaving the `agents_*` Rust commands + JS wrappers orphaned with
  no way to reach them. Re-added, wired to `agents_detect`/`agents_register`/
  `agents_unregister` + `cli_status`/`cli_install`.
- **Settings dialog now scrolls.** It was `overflow: hidden` with no
  `max-height` while vertically centered, so a tall dialog (many sections / a
  short screen) pushed its header — and the close button — above the viewport,
  unreachable. Capped to the viewport with internal scroll.
- **Autosave no longer closes the editor.** The `selfSaveSeq` signal that tells
  the reader "this content change is my own save, stay open" was defined but
  never incremented, so every autosave (and ⌘S) closed the editor mid-edit. The
  editor now bumps `selfSaveSeq` before each save and the reader reads it
  *untracked* (so the bump can't fire the close-effect early and consume the
  signal). e2e #29 was hardened to fail if the editor closes on autosave.
- **MCP `open_file` to a nested chapter** showed "Content not available." The
  `mcp-navigate` handler routed the raw disk-relative path, but the renderer
  keys chapters by the group-stripped `item.path`; for a nested file (e.g.
  `docs/setup.md`) the route set the title while the body lookup missed. It now
  translates `diskPath → item.path` via `folderMeta` (the same mapping the
  `mcp-folder-changed` handler already used). Top-level files are unaffected.
- **MCP `get_current_chapter` returned `No such file or directory`** after
  `open_file` to a nested chapter. The Reader's view-state $effect was
  pushing `current_chapter_path: path` (the group-stripped key) to the Rust
  MCP server, which then joined it to the active folder root — for
  `desktop-app/docs/MCP_SETUP.md` it tried `<root>/docs/MCP_SETUP.md`
  (missing the `desktop-app/` prefix). It now pushes `item?.diskPath || path`
  (the full on-disk relative path) — the same pattern used everywhere else
  in `Reader.svelte` (progress keys, link resolution, content enhancement).
  Verified: `open_file` on a nested file → `get_current_chapter` returns
  the chapter's preview and word count; the screenshot of the running app
  shows the chapter rendered (not the home view).
- **MCP `capture_screenshot` returned "FenceyMD window not found"** on the
  first call after launch (about half the time even after a GUI launch).
  xcap's `Window::all()` uses `kCGWindowListOptionOnScreenOnly`, which
  excludes windows that haven't been activated on SkyLight's active list.
  The tool now calls `unminimize()` + `show()` + `set_focus()` on the Tauri
  `WebviewWindow`, then `tokio::time::sleep(300ms)` before enumerating. The
  settle is the async window-server activation delay; the first call after
  launch used to always error, now it works. Total tool latency is ~500ms
  (300ms settle + ~200ms capture/encode). No external `osascript activate`
  needed. Verified: 10 consecutive `capture_screenshot` calls right after
  a fresh `open /Applications/FenceyMD.app` all returned valid PNGs.
- **In-chapter Find** silently skipped matches: a `/g` regex's `lastIndex`
  advanced across text nodes. Reset before each membership test.
- **Reading-progress data loss**: a single shared debounce timer meant
  switching chapters within 400ms dropped the previous chapter's disk-write.
  Now debounced per file.
- **Reader toolbar overflow** at 769–1130px (narrow desktop window with the
  sidebar in-flow) pushed a horizontal page scrollbar. Replaced viewport
  media queries with a container query on the content pane + a flex-shrinking
  find input; the e2e suite now asserts no page overflow at narrow widths.
- **PDF export** always renders light (white page, dark text) regardless of
  the app theme — fixes the dark "box in white margins" when exporting from
  dark mode. Wide CSV/markdown tables wrap instead of clipping; tall diagrams
  scale to fit one page instead of being cut off.
- **`watch_folder`** recovers from a poisoned mutex instead of panicking.
- **First-launch CLI install could create a self-referential symlink** if
  `current_exe()` returned the symlink path (e.g. the app was launched via
  `fenceymd` from a terminal, the dock stored a relative path, or some
  Apple-event pathway resolved through the existing symlink). The result:
  `which fenceymd` finds nothing and any call returns "too many levels of
  symbolic links". `install_into` now canonicalizes the exe and rejects
  self-references before creating a symlink, with a new test
  (`refuses_self_referential_symlink`) covering the case. The existing
  idempotent-reinstall path is preserved (the guard only fires on create,
  not on verify-already-correct). Recovery: `rm /opt/homebrew/bin/fenceymd
  && /Applications/FenceyMD.app/Contents/MacOS/fenceymd --install-cli`.
- **Failed fence renders** (offline lazy-load, bad diagram syntax) now show an
  inline notice and keep the source visible, instead of failing silently.

### Changed
- `scan_folder` caps per-file reads at 5 MB and logs read failures instead of
  silently substituting empty content.
- Build artifacts (`target/`, `dist/`, `node_modules/`) untracked from git
  (they were committed before `.gitignore` covered them).
- `dompurify` added as an explicit dependency (was only transitive).
- Stale PDF-pipeline comments (Reader.svelte, DEVLOOP.md) corrected to
  describe the actual headless-Chrome path.

### Added
- **CSV fence** rendered as a real `<table>` (lean core, not a plugin).
  `papaparse` lazy-loads from `src/lib/renderers/csv.js`; first row
  becomes the header, the rest the body, with a quiet "N rows" note.
  See `demo/12-csv.md`.
- **CI workflow** (`.github/workflows/ci.yml`):
  test matrix on macOS / Windows / Linux, desktop bundle on push to
  main via `tauri-apps/tauri-action`, and a license-drift job that
  fails PRs if `THIRD-PARTY-LICENSES.*` is out of date.
- **`THIRD-PARTY-LICENSES.md` + `.csv`**: 445 npm + 527 cargo deps,
  ~98% permissive. Generated by `scripts/gen-licenses.mjs`; rerun
  with `npm run licenses` after a dep change.
- **`SECURITY.md`**: vulnerability disclosure policy.
- **e2e coverage** for slide view (`#22`) and CSV fence (`#23`).
  Test selector for the slide view toolbar button tightened to
  `aria-label="Toggle slide view"` (was matching the sidebar
  chapter button by accident).

### Changed
- `src/lib/renderers/manifest.json` — adds the `csv` entry.
- `src-tauri/src/main.rs` — `renderer_manifest_parses_and_lists_langs`
  test now also asserts `csv` is in the manifest. PDF-side CSS
  for `.csv-block` mirrors the in-app card.
- README — features list mentions CSV, demo count 10 → 13, added
  the `THIRD-PARTY-LICENSES.md` attribution block and the
  `npm run licenses` hint.
- `.gitignore` — `Cargo.lock` is no longer ignored (binary app,
  reproducibility), `.mavis/` added.

## [1.0.0] — 2026-06-09

### Added
- **Renderer registry** (`src/lib/registry.js`). One dispatch for
  reader, slides, and PDF. The per-language `if` chain that used
  to live in `markdown.js` is gone; adding a new fence type means
  dropping a file in `src/lib/renderers/` and adding a row to
  `renderers/manifest.json`.
- **Katex math** (`src/lib/renderers/math.js`) — `$…$` and
  `$$…$$` in prose, theme-neutral.
- **Shiki syntax highlight** (`src/lib/renderers/shiki.js`) —
  dual-theme (github-light / github-dark), covers 25+ languages,
  falls back for any unknown fence lang.
- **Six core renderers registered**: `svg`, `html`, `mermaid`,
  `excalidraw`, `math`, `shiki`.
- **Live folder watcher** in Rust (`watch_folder` Tauri command).
  External edits refresh the library; scroll position preserved.
- **PDF export via headless Chrome** (`print_pdf`). Embeds katex
  CSS + shiki block CSS in the printable HTML.
- **13-chapter demo book** in `demo/` covering every feature.

### Fixed
- **TDZ from circular import** — the registry file used to
  side-effect-import the renderers, but ESM hoists imports so
  the renderers' top-level `register()` calls hit TDZ on the
  registry's `const`. Fix: renderers import the registry; the
  reader imports the renderer set from `renderers/index.js`.
- **CSS selector** — `pre code[class^="language-"]` failed on
  classes like `"js language-js"` (the attribute string doesn't
  start with `"language-"`). `collectBlocks` now scans all
  `pre code` and filters by class token.

[Unreleased]: https://github.com/NgLanNg/fenceymd/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/NgLanNg/fenceymd/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/NgLanNg/fenceymd/releases/tag/v1.0.0
