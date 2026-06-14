# Reader

## Vision & DoD (5W1H)

**What.** The Reader is the main content view: a chapter's markdown rendered as a calm, editorial-style page. It has a top toolbar (back-to-library, in-chapter find, font width controls, theme toggle, slides, links, PDF export, snapshot, bookmark, edit, settings) and a centered content column with an oversized H1 title and reading metadata (time, word count).

**Why.** Long-form reading demands a different shape than dashboards or apps. The Reader intentionally strips everything that doesn't help the eye move down the page: no chrome competing with the content, no permanent UI in the content column, a single accent color, generous margins, a serif default font that scales.

**Who.** Anyone reading. Designed for the default case (5–20 minute read of a single chapter) but tolerates very long chapters via the scroll progress indicator and the scroll-to-top gesture.

**When.** Mounted by `App.svelte` whenever `$route.name === 'chapter'`. Unmounted when the route changes to `home` (the Reader's HTML is fully removed; no leak across chapters).

**Where.** `src/components/Reader.svelte` is the host. It reads from `folderMeta` (for chapter content), `progress` (for scroll position), and `route` (for the active chapter path). The toolbar is inside the same component, with the responsive behavior driven by a **container query** on the content column (not the viewport).

**How (acceptance / DoD).**
- Title appears as an oversized H1 in the centered editorial style.
- Reading metadata (`X min · Y words`) appears below the title.
- Content is sanitized and rendered as HTML.
- Scroll position is saved to the progress map (debounced ~500 ms).
- The toolbar is keyboard-accessible (`?` opens the cheatsheet; `⌘F` opens in-chapter find; `e` opens the editor; `←/→` navigates siblings).
- At narrow viewports (< 480 px), font-size and width controls hide into the Settings panel; at < 768 px the sidebar becomes a drawer.
- The Reader is `prefers-reduced-motion` aware — no smooth scroll, no fade-in transitions.

---

## How we implemented it

**What.** A single Svelte 5 component that owns: the content render, the toolbar, the scroll-progress state machine, and the keyboard handlers.

**Why this shape.** Svelte 5's `$derived` makes the "current chapter text → render → html" chain trivial:
```
$route.path → folderMeta.find(path) → text → renderMarkdown → html → bodyHtml
```
Each step is a pure function of the previous. No effects, no manual subscriptions.

**When.** Mounted on `$route.name === 'chapter'`. The scroll-position save runs on every scroll event but is debounced (~500 ms per-file timer, replaced from a single shared timer that caused the data-loss bug we fixed in the v1.1 hardening pass).

**Where.**
- `src/components/Reader.svelte` — the host.
- `src/lib/markdown.js` — the `renderMarkdown` function (showdown + `enhance()`).
- `src/lib/anchors.js` — `enhance()` post-processes the html to inject stable block anchors.
- `src/lib/stores/state.js` — exports `mcpSessionContext`, `route`, `currentChapterPath`, etc.
- `src/lib/stores/progress.js` — `saveProgress(folder, path, scroll, bookmarked)`.

**How (tech).**
- **Render**: `text = $derived(item?.content ?? placeholder)`, `html = $derived(renderMarkdown(text))`, `bodyHtml = $derived(html.replace(leadingH1))`.
- **Sanitization**: `bodyHtml` is passed through DOMPurify (`sanitize.js`) before being inserted via `{@html}`. Mermaid runs with `securityLevel: 'strict'`.
- **Anchors**: `enhance()` walks the rendered HTML, adds `data-md-anchor="para-N"` / `code-N` / `mermaid-N:nodeA` etc. to every renderable block. This is the foundation for v2's anchor-based edit feature.
- **Toolbar responsiveness**: the toolbar uses CSS **container queries** (`@container` in `app.css`), not viewport media queries, so it responds to its own width. This is the only surface in the app that uses container queries.
- **Scroll position save**: a single `requestIdleCallback`-backed debounce. The previous (shared) timer was the cause of a data-loss bug — fixed.
- **Editor navigation close**: when the user clicks a sibling chapter while the editor is open, the editor closes (no half-save). Implemented as a `$effect` that watches `path` and resets `editing`.
- **MCP view state push**: a `mcp_update_view_state` Tauri command is called from a `$effect` that watches scroll + path. Merge semantics in Rust: `selected_text` only overwrites if non-empty, `scroll_position` always overwrites, `path` only if `is_some()`. This prevents the scroll handler from accidentally clearing the path the editor set.
- **Snapshot button + ⌘⇧S**: invokes `snapshot_app_to_clipboard` Rust command (xcap → arboard), shows a toast with dimensions.

**Gotchas.**
- The Reader used to have two `editing` flags (a local in `Reader.svelte` AND a store in `state.js`); they desynced and corrupted the autosave path. The fix: a single source of truth, the path-change `$effect` reconciles them.
- `data-md-anchor` injection has to happen AFTER the markdown is rendered to HTML but BEFORE the DOM is built, because once the DOM exists we can't re-parse it cheaply.
- The "no horizontal overflow" e2e test uses puppeteer's `page.evaluate` to read `scrollWidth`/`clientWidth` of the chapter content at multiple viewport widths; the container query was added in v1.1 to make this pass at 420 px.
