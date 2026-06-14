# Math (KaTeX)

## Vision & DoD (5W1H)

**What.** Inline math using `$...$` and display math using `$$...$$` render via KaTeX. Common LaTeX syntax — fractions, sums, integrals, Greek letters, matrices, cases.

**Why.** Scientific and technical books need real math notation. Unicode-only ("x² + y² = z²") is a workaround that breaks for anything beyond trivial expressions.

**Who.** Authors writing scientific, mathematical, or engineering content.

**When.** A chapter with `$...$` (inline) or `$$...$$` (display) math opens.

**Where.** `src/lib/renderers/math.js` is the renderer. KaTeX is loaded lazily.

**How (acceptance / DoD).**
- Inline math renders inline with the surrounding text.
- Display math renders on its own centered line.
- Fractions, sums, integrals, matrices, Greek letters all render correctly.
- An unknown LaTeX command falls back to a readable error message (KaTeX default).
- KaTeX CSS is auto-injected on first use; no manual setup.

---

## How we implemented it

**What.** A renderer that calls `katex.renderToString(latex, { displayMode, throwOnError: false })`.

**Why this shape.** KaTeX is the fastest math renderer for the web. It's synchronous (no font loading dance like MathJax), produces real HTML+CSS (not canvas/SVG), and is around 250 KB compressed.

**When.** Lazy-loaded. The first math fence triggers the import.

**Where.**
- `src/lib/renderers/math.js` — the renderer.
- `src/lib/renderers/manifest.json` — declares `math` as a known fence (with the auto-render mode that picks up `$...$` and `$$...$$` from the markdown body).

**How (tech).**
- **Detection**: showdown doesn't natively handle math. The `enhance()` pass post-processes the html: finds `$...$` and `$$...$$` patterns (not inside code blocks) and calls `katex.renderToString` for each.
- **Inline vs display**: a `$...$` that fits on one line is inline; `$$...$$` is always display. We use a simple parser to detect this.
- **CSS injection**: the `katex/dist/katex.min.css` is loaded once and injected into the document `<head>`. The same CSS is bundled into the PDF output by Rust (`read_katex_css`).
- **Error handling**: `throwOnError: false` — KaTeX returns the LaTeX source as-is with a small "color: red" error marker. Better than crashing the chapter.

**Gotchas.**
- `$` in code blocks should NOT be treated as math. We pre-scan the markdown for fenced code blocks and skip them in the math pass.
- KaTeX fonts are loaded async; the first render might briefly show "raw" LaTeX. We pre-warm by injecting the CSS on app start.
- The PDF path used to embed KaTeX CSS by reading the file from `node_modules/katex/dist/katex.min.css` and inlining it. This was a packaging change in v1.1 — the CSS is now bundled into the binary.
