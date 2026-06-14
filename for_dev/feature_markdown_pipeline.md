# Markdown pipeline

## Vision & DoD (5W1H)

**What.** A single rendering function that takes a markdown string and returns safe HTML. The function dispatches fenced blocks (```lang) through a *registry* of per-language renderers (code, html, svg, mermaid, math, excalidraw, csv, slides) and treats the rest as plain markdown.

**Why.** Markdown alone covers 80% of a book, but a real book needs more — embedded diagrams, math, code samples with syntax highlighting. Hardcoding each fence type in the renderer would couple every new feature to a single file. A **registry** decouples them: adding a new fence = one file in `renderers/`, one line in the manifest, no edit to the main renderer.

**Who.** A user writing a chapter. They get to use any fence the manifest declares. They don't have to know the registry exists.

**When.** The render function runs every time the chapter text changes (it's a `$derived`). The result is sanitized and inserted via `{@html}`. The same function is called by the PDF-export Rust path (via the renderer manifest, which the backend reads too).

**Where.** The single entry point is `renderMarkdown(text)` in `src/lib/markdown.js`. The registry is in `src/lib/registry.js` and `src/lib/renderers/manifest.json`. Each fence has its own renderer in `src/lib/renderers/<lang>.js`. The same manifest is read by Rust (`load_renderer_manifest` in `main.rs`) for the PDF path — so the JS and Rust paths are guaranteed to see the same set of fence types.

**How (acceptance / DoD).**
- Plain markdown is rendered as HTML (headers, lists, bold/italic, links, code spans, blockquotes).
- Fenced blocks of a registered language are dispatched to that language's renderer.
- Unknown fences are rendered as plain code blocks.
- The output is sanitized: no `<script>`, no `on*` handlers, no `javascript:` URLs.
- The result is cached: re-rendering the same text returns the same DOM.
- Adding a new fence type = one new file in `renderers/`, one new line in the manifest.

---

## How we implemented it

**What.** A 4-stage pipeline:
1. **Markdown → HTML**: showdown parses, then `enhance()` walks the AST and rewrites fences.
2. **Enhance**: a post-processing pass that turns ` ```html `, ` ```svg `, ` ```mermaid `, etc. into the actual renderer's output (or marks them for hydration).
3. **Registry dispatch**: each fence type has a `transform` function in `renderers/<lang>.js` and an entry in the manifest.
4. **Sanitize**: DOMPurify passes over the html.

**Why this shape.** Showdown's extensibility is via `setOption`/extension hooks but they're awkward for our use case (we want per-fence dispatch with a clean registration API). We could have written a single mega-renderer, but the registry pattern lets each fence type be tested in isolation, lets the manifest be data-driven, and matches how the PDF path thinks about renderers.

**When.** Every time the chapter text changes. The result is also cached at the React-component level (Svelte's reactivity) so a scroll event doesn't re-run the pipeline.

**Where.**
- `src/lib/markdown.js` — `renderMarkdown` + `enhance`.
- `src/lib/registry.js` — looks up the renderer for a fence type, hands it the fence content + meta.
- `src/lib/renderers/manifest.json` — single source of truth for fence types.
- `src/lib/renderers/<lang>.js` — per-fence `transform` functions.
- `src/lib/sanitize.js` — DOMPurify boundary.
- `src-tauri/src/main.rs` — `load_renderer_manifest` reads the same JSON for the PDF path.

**How (tech).**
- **Markdown lib**: `showdown` v2. We use the converter API directly + extensions for fenced code, tables, autolinks.
- **Enhance**: the `enhance()` post-processor walks the html string, finds `<pre><code class="lang-XXX">` blocks, and substitutes them with the renderer's output. We use a single-pass regex walk to avoid re-parsing.
- **Registry**: a simple JS object keyed by fence language. The manifest is loaded once at module init.
- **Sanitization**: DOMPurify with profiles per surface (chapter body, html fence, svg fence). Mermaid is initialized with `securityLevel: 'strict'` separately.
- **PDF path**: Rust reads the manifest and calls each renderer's transform function (in the JS bundle, via `eval` at PDF-build time). This is the only place JS is run inside Rust — see `transform_for_pdf` in `main.rs`.

**Gotchas.**
- Showdown's html output is **not** XSS-safe out of the box. The `body` sanitizer runs on the chapter body, but we ALSO sanitize the `html` fence and the `svg` fence separately (different DOMPurify profiles). Mermaid runs in `strict` mode by default.
- The "embedded widgets" must be registered as fenced blocks, not as inline markdown. The renderer `enhance()` only handles top-level fenced blocks.
- Renderer initialization is lazy: mermaid, shiki, katex, excalidraw are heavy — they load on first use, not at app start. The `~5 MB DMG` budget depends on this.
