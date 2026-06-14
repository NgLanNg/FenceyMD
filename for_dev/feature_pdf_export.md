# PDF export

## Vision & DoD (5W1H)

**What.** The user clicks the PDF icon in the Reader toolbar (or presses ⌘P) and the current chapter is exported as a self-contained vector-text PDF. The export uses the chapter's rendered HTML (sanitized, with all fences resolved) and produces a print-ready document with the same fonts, layout, and embedded diagrams as the on-screen render.

**Why.** Books are for reading. Some reading happens offline (print, archive, send to a colleague). PDF is the universal format for that.

**Who.** Anyone who wants to share or print a chapter.

**When.** The Reader is showing a chapter. The user clicks the PDF icon or presses ⌘P. A native Save dialog appears; the user picks a location; the PDF is written.

**Where.** The button is in the Reader toolbar. The export is a Tauri command (`print_pdf`) that orchestrates the rendering + writing.

**How (acceptance / DoD).**
- The PDF contains the chapter's content as vector text (not raster screenshots).
- All diagrams (mermaid, excalidraw, svg) are inlined as SVG.
- Code blocks are syntax-highlighted.
- KaTeX-rendered math is preserved.
- The PDF is single-column, with the same fonts as the on-screen render.
- The PDF is light-themed (always), regardless of the app's current theme.
- The user's local fonts are used (no remote font requests).
- A "saving" indicator shows while the export runs.

---

## How we implemented it

**What.** A Rust Tauri command (`print_pdf`) that:
1. Builds a print-ready HTML string from the chapter's content (via the renderer manifest + JS in `build_print_html`).
2. Writes the HTML to a temp file.
3. Spawns headless Chrome (`chrome --headless --print-to-pdf`) to convert HTML → PDF.
4. Reads the PDF, returns the bytes; the JS side writes them to the user's chosen path.

**Why this shape.** Headless Chrome gives us pixel-perfect fidelity: the same rendering engine that powers the in-app render is the one that prints. We get vector text, all CSS, embedded SVG, etc. for free.

**When.** Triggered by the PDF button or ⌘P. Takes 1-3 seconds for a typical chapter.

**Where.**
- `src/components/Reader.svelte` — the toolbar button + ⌘P handler.
- `src/lib/tauri.js` — `printPdf` wrapper.
- `src-tauri/src/main.rs` — `print_pdf` Tauri command.
- `src-tauri/src/main.rs` — `build_print_html`, `transform_for_pdf`, `read_katex_css`.

**How (tech).**
- **Chrome**: `chromium` (Linux) or system Chrome (macOS/Windows). We find the binary with `find_chrome()` (checks well-known paths, env var, etc.).
- **Print args**: `--no-pdf-header-footer --print-to-pdf=path --no-sandbox` (the `--no-sandbox` is conditional — we try sandboxed first and fall back if Chrome refuses).
- **HTML build**: `build_print_html` reads the chapter's content, inlines KaTeX CSS, applies the renderer manifest (the same one JS uses), and forces a light theme palette. This is the "always-light" rule.
- **Dark-mode relight**: when the app is in dark mode, mermaid blocks are re-rendered on a *clone* of the chapter DOM with the light theme before being shipped to Rust. The clone avoids a visible flash in the live UI.
- **Diagram scale**: wide tables and large diagrams are scaled to fit the page width (`transform_for_pdf`).
- **Async**: the Rust command is async (uses tokio); the JS awaits the result.

**Gotchas.**
- The "dark-on-white" mermaid bug was caused by `mermaid.initialize()` being called globally with the current theme. Fixed by cloning the chapter DOM, initializing mermaid on the clone with the light theme, and replacing the live blocks with the clone's output.
- The "always-light" rule is intentional. Dark PDFs print as washed-out gray on white; users who want dark can use the app's dark mode in-app.
- We try Chrome with `--sandbox` first, fall back to `--no-sandbox` only if Chrome refuses. Some Linux containers can't run sandboxed Chrome; this fallback is logged.
- A native Save dialog is used (not a JS prompt) so the user can pick a folder, see file types, etc. Native dialogs can't be auto-clicked in e2e; the export is verified by code path and a manual test.
