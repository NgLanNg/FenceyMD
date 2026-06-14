# Slide view (Marp)

## Vision & DoD (5W1H)

**What.** A chapter whose content is a Marp-formatted deck renders as a navigable slide presentation. The user steps through slides with arrow keys or by clicking; Esc exits back to the Reader. The slide view has the same theme (light/dark, font) as the rest of the app.

**Why.** Some content is naturally a deck — a talk, a tutorial, a product pitch. Authors shouldn't have to maintain a separate `.pptx` file when the rest of the book is markdown. Marp is the de-facto "markdown for slides" format.

**Who.** Authors giving talks, recording screencasts, or structuring content as a presentation. Readers who want to step through instead of scroll.

**When.** A chapter that starts with a Marp front-matter block (`---` followed by `marp: true`) opens. The Reader auto-detects this and adds a "slides" button to the toolbar.

**Where.** `src/lib/renderers/manifest.json` declares `marp` as a known fence. `src/lib/slides.js` handles the Marp detection. `src/components/SlideViewer.svelte` is the deck UI.

**How (acceptance / DoD).**
- A Marp-formatted chapter shows a slides icon in the toolbar.
- Clicking the icon enters deck mode; the chapter content becomes a navigable deck.
- Arrow keys step forward/backward; clicking a slide jumps to it; Esc exits.
- The deck has a slide counter and a progress bar.
- Background images and theme directives from the Marp front matter are respected.
- The deck fits the viewport with no overflow (1280×720 aspect).

---

## How we implemented it

**What.** A slide-detection pass on the rendered HTML, plus a fullscreen overlay that steps through the deck.

**Why this shape.** Marp's slide breaks are `---` separators. The markdown pipeline already splits on those for tables; we hook in here. The slide viewer is a separate component (not the Reader) because the layout and interaction model is different.

**When.** A chapter is detected as Marp-formatted → slides icon appears. User clicks → slide viewer mounts as an overlay.

**Where.**
- `src/lib/slides.js` — Marp detection + slide splitting.
- `src/components/SlideViewer.svelte` — the deck UI.
- `src/lib/slides.js` exports `isMarpChapter(text)` and `splitSlides(text)`.

**How (tech).**
- **Detection**: a chapter is Marp if the first `---`-fenced block after the front matter contains `marp: true` in the YAML.
- **Splitting**: split the body on `---` (with the constraint that the first occurrence is the front matter, the rest are slide breaks). Each segment is rendered as its own "page."
- **Theme application**: the front-matter `theme:` directive maps to a CSS class (gaia, default, etc.). We bundle the Marp CSS and apply it at the slide-viewer root.
- **Keyboard**: `←/→` and `PgUp/PgDn` step; `Esc` exits; `Home/End` jump to first/last. The fullscreen container traps focus so the underlying Reader doesn't scroll.
- **Marquee selection**: a small slide counter at the bottom-right (e.g. `3 / 7`) plus a progress bar.

**Gotchas.**
- Marp's `---` separator collides with the markdown horizontal-rule `---`. We disambiguate by treating the first `---` block as front matter and the rest as slide breaks.
- The Marp `theme: gaia` directive is a no-op without the `@marp-team/marpit-theme-gaia` package. The bundled theme is the only one we ship; custom themes would need to be CSS-only and not require JS.
- The slide view's foreignObject was the cause of a sizing bug in v1.1 (Marp's default layout didn't size correctly to our viewport); we explicitly set the slide container's width/height in CSS to fix it.
