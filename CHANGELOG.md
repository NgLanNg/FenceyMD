# Changelog

All notable changes to FenceyMD are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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

[Unreleased]: https://github.com/NgLanNg/fenceymd/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/NgLanNg/fenceymd/releases/tag/v1.0.0
