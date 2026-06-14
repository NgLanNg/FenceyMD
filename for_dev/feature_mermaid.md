# Mermaid diagrams

## Vision & DoD (5W1H)

**What.** A ` ```mermaid ` block renders as a live SVG diagram — flowcharts, sequence diagrams, ER, gantt, class, state. The reader sees the diagram; clicking on a node can show its label; the diagram scales to fit the column.

**Why.** Diagrams in a book are the difference between "explained" and "shown." Mermaid lets the author write the diagram in source (diffable, reviewable, regenerable) and renders it on the fly.

**Who.** Technical authors. Anyone writing architecture docs, sequence explanations, ER models.

**When.** A chapter with ` ```mermaid ` fences opens. Mermaid is loaded lazily on first use.

**Where.** `src/lib/renderers/mermaid.js` is the renderer. `mermaid.initialize()` is called once with the security-level and theme settings.

**How (acceptance / DoD).**
- Flowchart, sequence, class, state, gantt, ER all render correctly.
- The diagram respects the app theme (light/dark via Mermaid's `theme` setting).
- Diagrams scale to the chapter content width.
- A node click can be a hyperlink (if the source used `click NodeName url`).
- Mermaid runs in `securityLevel: 'strict'` — no inline scripts.

---

## How we implemented it

**What.** A renderer that:
1. Marks the fence as a mermaid-placeholder (a `<div class="mermaid-block" data-source="...">`).
2. After DOM insertion, calls `mermaid.render(uniqueId, source)` which returns the SVG.
3. Swaps the placeholder for the rendered SVG.

**Why this shape.** Mermaid needs a real DOM node to inject into. We do the swap post-DOM-insertion (in a `$effect` in the Reader, not in the markdown pipeline) because Mermaid's API is DOM-based, not string-based.

**When.** Lazy-loaded. First mermaid fence triggers a ~500 KB download.

**Where.**
- `src/lib/renderers/mermaid.js` — the renderer placeholder.
- `src/components/Reader.svelte` — the `$effect` that walks `.mermaid-block` elements and renders them.
- `src/lib/renderers/manifest.json` — declares `mermaid` as a known fence.

**How (tech).**
- **Mermaid v10+**: async API. `mermaid.render(id, source) → { svg }`.
- **Init**: `mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: currentTheme, fontFamily: getComputedStyle(root).getPropertyValue('--font-sans') })`.
- **Theme switching**: when the app theme flips, we re-render all `.mermaid-block` elements with the new theme. This is the only mermaid-specific theme-flip path.
- **PDF path**: the PDF renderer uses a clone of the chapter DOM, re-initializes mermaid on the clone with the light theme, and ships the result to Rust. Without the clone, the global `mermaid.initialize()` call would leak the light theme back into the live DOM.

**Gotchas.**
- Mermaid's `mermaid.render` returns a Promise; we have to await it. The post-DOM-insertion pattern is the simplest way to manage this.
- A common bug: mermaid blocks at the bottom of a chapter weren't rendered because the `$effect` ran before the user scrolled there. We use `IntersectionObserver` to render-on-scroll instead of render-on-mount. This is a future improvement; currently we render all blocks at mount (fast for small chapters, slow for huge ones).
- The PDF dark-mode bug (mermaid dark-on-white export) is fixed by the clone + re-init pattern.
