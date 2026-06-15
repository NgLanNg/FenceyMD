# FenceyMD Codebase Structure & File Map

This document describes the directory layout, file organization, and architectural boundaries of the FenceyMD project.

---

## High-Level Layout

The project is structured as a standard Tauri v2 monorepo with Svelte 5:

```
fenceymd/
├── demo/                 # Bundled tour/tutorial book (used in ?test=1 mode)
├── docs/                 # General documentation (setup guides, registration snippets, screenshots)
├── for_dev/              # Developer guides: feature-by-feature design vision & implementation details
├── scripts/              # Build, license compilation, and deployment scripts
├── src/                  # Frontend: Svelte 5 application, stores, and custom renderers
├── src-tauri/            # Backend: Tauri privileged shell (Rust)
├── e2e-test.mjs          # Puppeteer integration test suite (40+ E2E scenarios)
├── vite.config.js        # Vite build & plugin configuration
└── svelte.config.js      # Svelte preprocessor & compiler options
```

---

## 1. Frontend Structure (`src/`)

All frontend logic, styles, and markup reside in [src/](file:///Users/alan/WORKSPACE/Books/desktop-app/src).

```
src/
├── App.svelte            # Root component / application shell & global keyboard listener
├── app.css               # Unified styling (Vanilla CSS with CSS variables for theming)
├── main.js               # Application entry point (mounts App.svelte)
├── components/           # Svelte UI components
└── lib/                  # Core JS modules, stores, and renderer registry
```

### Components (`src/components/`)
Components are visual building blocks. They do not hold persistent data (which belongs in stores):

* **[Picker.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Picker.svelte):** Landing screen shown when no folder is open. Hosts folder selection buttons and the recent files/folders list.
* **[Sidebar.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Sidebar.svelte):** Left navigation panel. Contains folder switcher, filter inputs, and footer options.
* **[SidebarTree.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/SidebarTree.svelte) & [TreeNode.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/TreeNode.svelte):** Grouped hierarchical navigation for directories and Markdown chapters.
* **[Library.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Library.svelte):** Main directory explorer/dashboard showing files and parts/groups.
* **[Reader.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Reader.svelte):** Reading surface layout, toolbar controls (font resize, bookmarking, export triggers, in-chapter find).
* **[Editor.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Editor.svelte):** WYSIWYG editor overlay powered by Tiptap. Supports inline markdown styling, keyboard shortcuts, and split-pane previews.
* **[SlideViewer.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/SlideViewer.svelte):** Full-screen presentation mode that transforms chapters (split by `---`) into slide decks via Marp.
* **[OutlinePane.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/OutlinePane.svelte):** Renders table of contents (headings hierarchy) for the active chapter.
* **[CrossSearchPanel.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/CrossSearchPanel.svelte):** Global multi-file search overlay (triggered by `⌘⇧F`).
* **[Settings.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/Settings.svelte):** Preferences modal (themes, fonts, width controls, AI Agent register toggles).
* **[ZoomOverlay.svelte](file:///Users/alan/WORKSPACE/Books/desktop-app/src/components/ZoomOverlay.svelte):** Modal overlay permitting full-screen zooming of visual blocks (diagrams, SVGs).

### Libraries & Business Logic (`src/lib/`)
Contains non-visual helpers, state management, and the renderer registry:

* **[registry.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/registry.js):** The Markdown code fence registry. Decouples code fence processors from Markdown rendering, dispersing them to PDF rendering, reader, and slides.
* **[markdown.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/markdown.js):** High-level markdown parsing (using showdown) and post-render enhancement wrapper.
* **[tauri.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/tauri.js):** Wrapper around Tauri APIs (invoke, listen, dialog). Gracefully mocks commands when running in a plain browser (`?test=1`) to avoid crashing.
* **[stores.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores.js):** Unified import barrel for all Svelte stores.
* **stores/** (Submodule Directory):
  * **[state.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores/state.js):** Core shared writeables (current active path, search flags).
  * **[prefs.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores/prefs.js):** User settings (themes, font sizes, page widths) synced to localStorage.
  * **[progress.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores/progress.js):** Reading progress (scroll percentage, bookmark states).
  * **[library.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores/library.js):** Folder indexing, navigation, and sibling chapter calculations.
  * **[files.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/stores/files.js):** Save-to-disk handlers, optimistic layout updates, and file renaming.

### Renderers (`src/lib/renderers/`)
Each Markdown plugin corresponds to a file registered in the barrel index:
* **[manifest.json](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/manifest.json):** Declarative catalog of renderers mapping file names to target styles (fence, inline, math).
* **[index.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/index.js):** Barrel import file that registers all renderers in `registry.js` on bootstrap.
* **[svg.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/svg.js):** Renders raw XML SVG fences safely into the actual DOM using correct namespaces.
* **[html.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/html.js):** Safely injects HTML layout blocks (sanitized using dompurify).
* **[mermaid.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/mermaid.js):** Renders flowcharts and diagrams using Mermaid.js.
* **[excalidraw.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/excalidraw.js):** Mounts interactive drawing canvases, saving layout states back into Markdown code blocks.
* **[math.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/math.js):** Renders LaTeX equations (`$` and `$$`) via KaTeX.
* **[shiki.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/shiki.js):** Highlights code syntax (fallback for unhandled code fences).
* **[csv.js](file:///Users/alan/WORKSPACE/Books/desktop-app/src/lib/renderers/csv.js):** Converts CSV code blocks into static Svelte tables using PapaParse.

---

## 2. Backend Structure (`src-tauri/`)

Tauri handles desktop integrations, filesystem safety, and agent APIs.

```
src-tauri/
├── Cargo.toml            # Rust dependencies & packaging config
├── tauri.conf.json       # App name, capabilities, and windows specs
├── src/                  # Rust source
│   ├── main.rs           # Tauri entry point, file/OS commands, PDF printing
│   ├── mcp.rs            # HTTP/JSON-RPC Local MCP Server for AI Agent control
│   ├── agents.rs         # Agent configuration injector (Claude, Gemini, Codex, etc.)
│   └── cli.rs            # Native terminal CLI installer
```

### Core Backend Modules
* **[main.rs](file:///Users/alan/WORKSPACE/Books/desktop-app/src-tauri/src/main.rs):** Orchestrates Tauri commands (scanning folders, writing/renaming files, clipboard operations). Contains the headless Chrome printer for vector PDF generation.
* **[mcp.rs](file:///Users/alan/WORKSPACE/Books/desktop-app/src-tauri/src/mcp.rs):** Launches the local Model Context Protocol (MCP) server. Exposes tools (`open_file`, `get_chapter_content`, `get_book_toc`, `capture_screenshot`) over a random localhost port, communicating with the UI via Tauri events.
* **[agents.rs](file:///Users/alan/WORKSPACE/Books/desktop-app/src-tauri/src/agents.rs):** Connects AI agents to the application. Modifies configuration files (`~/.claude.json`, `~/.gemini/settings.json`, Codex configs) to register FenceyMD's native bridge command.
* **[cli.rs](file:///Users/alan/WORKSPACE/Books/desktop-app/src-tauri/src/cli.rs):** Manages the `fenceymd` terminal CLI installation by symlinking the active binary onto the user's `PATH` (such as `/usr/local/bin` or brew bin directories).

---

## 3. Developer Documentation Structure (`for_dev/`)

The [for_dev/](file:///Users/alan/WORKSPACE/Books/desktop-app/for_dev) folder is dedicated to developer specifications and feature logs.

* **[index.md](file:///Users/alan/WORKSPACE/Books/desktop-app/for_dev/index.md):** Main entry point mapping technical naming, technical requirements, and index of features.
* **`feature_*.md` files:** One document per capability containing:
  1. **Vision & Definition of Done (DoD):** Answers the *Who, What, When, Where, Why* at a product level without referencing implementation details.
  2. **Implementation & Gotchas:** Details the engineering details (tech stack, algorithms, platform gotchas, and edge cases discovered during QA/testing).
* **[plan_mcp_screenshot_v2.md](file:///Users/alan/WORKSPACE/Books/desktop-app/for_dev/plan_mcp_screenshot_v2.md):** Long-term blueprint for headless Chrome re-rendering in the capture tool.
