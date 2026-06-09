# MD Reader — Lean Core + Plugins Plan

**Decision:** Ship a **lean core** and move data/terminal to **optional plugins**.
**Lean core:** `md · mermaid · katex · shiki · svg · excalidraw · slides`
**Plugins (opt-in, out of core):** `csv/data`, `terminal`

> Rationale: keep the default app focused on "calm reading" (per `design.md`
> north star) and the DMG/permissions lean (per `AGENTS.md`), while still
> allowing power features for those who want them.

---

## Current state (verified against the code)

| Core feature | State | Notes |
|---|---|---|
| Markdown render | ✅ | `showdown` in `src/lib/markdown.js` |
| Mermaid | ✅ | lazy-loaded; light/dark theming; per-diagram toggle; Copy/PNG |
| Inline SVG | ✅ | namespace-correct (DOMParser → `createElementNS`) |
| Excalidraw | ✅ | `ExcalidrawViewer.svelte`; save-to-chapter + save-as-file; Rust `update_excalidraw_block` (+9 unit tests) |
| Slides (Marp) | ✅ | `SlideViewer.svelte` |
| **Math (katex)** | ❌ **missing** | not wired; **not even a direct dep** — the `katex` build chunk is pulled transitively by mermaid. `$…$` / `$$…$$` in prose renders as plain text today. |
| **Syntax highlight (shiki)** | ❌ **missing** | not a direct dep; code fences render as plain monospace. For a tech-book reader this is a **lean-core must**, not a plugin — see Phase 1. |
| Plugin boundary | ❌ **missing** | `enhance()` is a hardcoded `language-*` if-chain; nothing to plug into |

Backend (`src-tauri/src/main.rs`) is solid: `scan_folder`, `write_file`,
`rename_file`, `save_export`, `copy_image`, `print_pdf` / `build_print_html`,
`update_excalidraw_block`, `watch_folder`. `cargo test` 9/9, e2e 15/15.

---

## Gaps → roadmap

### Phase 1 — Math + Syntax highlight in the core *(small, self-contained, two quick wins)*
- **Math:** add `katex` as a **direct** dependency.
  - Render `$…$` (inline) and `$$…$$` (block) in `enhance()`, lazy-loaded like
    mermaid. Must run in **both** the reader and `SlideViewer`.
  - Theme-neutral (katex inherits text color; verify in light + dark).
  - **Done when:** the demo gets a math chapter; e2e asserts a `.katex` element renders.
- **Syntax highlight:** add `shiki` as a **direct** dependency.
  - Render ` ```lang ` fences in `enhance()`, lazy-loaded; reuse the same
    light/dark theme tokens already in use.
  - VS Code TextMate grammars → matches a reader's editor if they have one.
  - MIT licensed; broad language coverage out of the box (js/ts, py, rs, go,
    java, c/c++, sql, bash, json, yaml, …).
  - **Done when:** the demo gets a code chapter; e2e asserts highlighted tokens
    render in both light and dark.

### Phase 2 — Renderer registry *(the keystone)*
Replace the hardcoded fenced-block chain with a registry so every block type is
declared once and reused everywhere.
- `registry.register(lang, { load, render(block, ctx) })`.
- Core registers: `svg`, `html`, `mermaid`, `excalidraw`, `math`, `shiki` (for code fences).
- **One dispatch, three consumers:** reader (`markdown.js`), slides
  (`SlideViewer.svelte`), and PDF (`build_print_html`) all go through it.
  *(Today svg/html handling is duplicated in reader + slides — the recent SVG
  fix had to be written twice. The registry removes that class of bug.)*
- **Done when:** `enhance()` has no per-language `if` branches; reader + slides +
  PDF share the registry; e2e still 15/15 + new svg/mermaid/excalidraw cases pass.

### Phase 3 — Plugin model + enable/disable
- **Define "plugin"** = compile-time module + runtime toggle (NOT arbitrary
  third-party code loaded from disk — that's a security rabbit hole for a Tauri
  app). Decide and write this down before coding.
- Settings → **Extensions** section: list plugins, toggle on/off, persist
  (localStorage, like other prefs). Heavy/sensitive plugins **off by default**.
- A disabled plugin's deps must **not** load (lazy import gated by the toggle).
- **Done when:** toggling a plugin in Settings enables/disables its block type
  live, and its bundle chunk only loads when enabled + present.

### Phase 4 — Plugins (each opt-in, outside core)
- **CSV (data):** `papaparse` → a styled editorial `<table>`. **No AG Grid, no
  Tabulator** — interactive grids add weight and break the "calm reading" brand.
  Fence: ```` ```csv ````. If/when interactive grids become a real need, revisit
  as a separate plugin.
- **Terminal:** `@xterm/xterm` UI + **`portable-pty`** (Rust) backend behind a
  **separate, opt-in Tauri capability** — never in the core allowlist. Treat as a
  companion module; keep shell execution out of the default reader.

### Phase 5 — OSS release readiness *(separate track)*
- Choose + add a **LICENSE** (MIT recommended; Apache-2.0 also fine).
- `README` (install + screenshots), `THIRD-PARTY-LICENSES`
  (`npx license-checker` + `cargo about`), identify the one **EPL-2.0** transitive
  dep, `CONTRIBUTING.md`.
- CI (GitHub Actions) building macOS / Windows / Linux on native runners.
- e2e coverage for excalidraw, slides, math (currently uncovered).

---

## Sequencing & acceptance

1 → 2 → 3 → 4 (4 can fan out per plugin once 3 lands). 5 runs in parallel.

Every phase must hold the project's stop conditions (`AGENTS.md`):
`cargo test` green · `npm run build:desktop` succeeds · e2e (or smoke) passes ·
code-review sign-off · committed to a working branch (never `main`).

## Out of scope for v1 *(parking lot, revisit after v1 ships)*

These were considered and explicitly **deferred** — not in this roadmap, not
accidentally missed.

- **DBML / ERD** — earlier brainstorm had DBML → mermaid `erDiagram`, but
  `erDiagram` is lossy (no column aliases, no per-column notes, no schema refs).
  Anyone using DBML for serious schema docs hits the wall. **Out** — if/when
  real schema-doc demand shows up, revisit as a plugin with a proper DBML
  renderer (not a transform into mermaid).
- **D2 (diagrams-as-code)** — alternative to mermaid with cleaner syntax for
  nested/architecture diagrams. **Not needed** — mermaid is in lean core, broader
  diagram coverage, larger ecosystem. D2 is a different tool, not a "mermaid 2.0".
- **force-graph (file relationship visualizer)** — interesting "map of chapters"
  idea but non-trivial renderer, no clear must-have, needs a new lib
  (d3-force or react-force-graph) + interaction model. **v2 conversation.**
- **Other file formats (EPUB, AsciiDoc, …)** — outside the "lean core reading
  experience" scope. EPUB export is the only one that might come back as a
  reader-of-books feature, and even that is v2+.

### v2 quality-of-life *(also parking lot, not blockers)*

- **Cross-chapter search** — Cmd+Shift-F across the whole book (today Cmd+F is
  per-file only). Needs an index strategy (lunr/minisearch over chapter text).
- **Auto-TOC from folder** — reader builds a sidebar/index from chapter
  filenames. Currently probably manual ordering.
- **Code block copy button** — one-line, expected by every reader since
  GitBook. Low cost, ship when convenient.
- **Image paste from clipboard** — common writer workflow, possibly already
  half-built in the image-copy backend.

---

## Open decisions (need an owner call)
- Plugin = compile-time-module-with-toggle? (recommended) — confirm.
- CSV = lightweight styled table via PapaParse (no AG Grid, no Tabulator) — confirm.
- App license = MIT? — confirm.

---
*Highest-leverage start: Phase 1 (math + shiki — two quick wins); Phase 2
(registry) is the keystone that makes them "just renderers" and turns
csv/terminal into real opt-in plugins instead of more hardcoded branches.*
