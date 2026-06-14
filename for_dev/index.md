# FenceyMD — for developers

Every feature the app ships, in two parts per doc:

1. **Vision & DoD** — what the feature is, why it exists, the acceptance criteria. No tech stack, no specific implementation. Read this if you want to understand the *what*.
2. **How we implemented it** — the actual tech stack, the algorithm, the gotchas. Read this if you want to understand the *how*.

Each doc is structured around the **5W1H** questions (Who, What, When, Where, Why, How). The vision section answers them at the user level; the implementation section answers them at the engineer level.

## Naming

- **Display name**: `FenceyMD` (the user-facing brand — title bar, sidebar, Settings → About, About panel).
- **Binary name**: `fenceymd` (lowercase, no separator — what shows up in `ps`, the `.app/Contents/MacOS/fenceymd` path, the `package.json`/`Cargo.toml` crate name, and the MCP `serverInfo.name`).
- **Bundle id**: `com.fenceymd.app` (reverse-DNS; drives the per-OS app-data dir like `~/Library/Application Support/com.fenceymd.app/` on macOS).
- **URL scheme**: `fenceymd://` (planned for Track C — lets any agent open a chapter with `open fenceymd:///abs/path` on macOS, no port file needed).
- **localStorage key prefix**: `fenceymd-*` (theme, font size, content width, etc.). A one-time migration on first launch copies any pre-rebrand `md-reader-*` keys to the new prefix and deletes the old ones — see `feature_rebrand_state_migration.md`.

If you're grepping the codebase for the brand name, search for `FenceyMD` (display) or `fenceymd` (technical). The two cases do not overlap.

## Index

### Reading
- [Folder as a book](feature_folder_as_a_book.md) — pick a directory, get a book
- [Library / Home](feature_library.md) — recents + continue reading
- [Sidebar chapter tree](feature_sidebar.md) — nested file tree
- [Reader](feature_reader.md) — typography, scroll, the reading experience
- [In-chapter Find](feature_in_chapter_find.md) — ⌘F search
- [Cross-chapter search](feature_cross_chapter_search.md) — ⌘⇧F full-text search
- [Reading progress + bookmarks](feature_progress_and_bookmarks.md) — survives restarts
- [Outline pane](feature_outline_pane.md) — auto-TOC for the chapter

### Rendering
- [Markdown pipeline](feature_markdown_pipeline.md) — showdown + extension registry
- [Live HTML fences](feature_html_fence.md) — `html` → real DOM
- [Live SVG fences](feature_svg_fence.md) — `svg` → the graphic itself
- [Code highlight (Shiki)](feature_code_highlight.md) — ` ``` ` with copy button
- [Math (KaTeX)](feature_math.md) — inline + display
- [Mermaid diagrams](feature_mermaid.md) — flowcharts, sequence, ER
- [Excalidraw](feature_excalidraw.md) — interactive drawing canvas
- [CSV](feature_csv.md) — fenced blocks become tables
- [Slide view (Marp)](feature_slide_view.md) — when and how to use deck mode

### Editing
- [Inline editor](feature_editor.md) — Tiptap WYSIWYG
- [Autosave](feature_autosave.md) — ⌘S + "saved Ns ago" indicator
- [Clipboard image paste](feature_image_paste.md) — paste PNG → save to `images/`
- [Find / replace](feature_find_replace.md) — editor-level search
- [Paragraph tracking](feature_paragraph_tracking.md) — cursor anchor for v2 AI

### Output
- [PDF export](feature_pdf_export.md) — headless Chrome → vector PDF
- [Window snapshot to clipboard](feature_snapshot.md) — ⌘⇧S
- [Diagram export (PNG)](feature_diagram_export.md) — copy mermaid/excalidraw as image
- [Open in external editor](feature_external_editor.md) — hand off to user's editor

### Agent control (MCP)
- [MCP server](feature_mcp_server.md) — local HTTP for AI agents
- [MCP tool: open_file with auto-resolve](feature_mcp_open_file.md) — agent asks for any path, app finds the folder
- [MCP tool: capture_screenshot](feature_mcp_capture_screenshot.md) — agent grabs the live view
- [MCP tool: get_debug_log](feature_mcp_debug_log.md) — agent reads the activity log
- [Agent auto-registration](feature_agent_registration.md) — Settings toggle wires the agent config

### Settings
- [Settings panel](feature_settings.md) — theme, font, code theme, width, reset
- [Theme + OS auto-detect](feature_theme.md) — light/dark + system sync
- [Reopen last folder on launch](feature_reopen_last.md) — session continuity
- [File watching](feature_file_watcher.md) — external edits show up live

### Foundation
- [Sanitization boundary](feature_sanitization.md) — untrusted-content trust model
- [Anchor infrastructure](feature_anchors.md) — stable block IDs (foundation for v2 AI)
- [Rebrand + state migration](feature_rebrand_state_migration.md) — `MD Reader` → `FenceyMD` data carry-over (one-time, on first launch after update)
