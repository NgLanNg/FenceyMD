# Outline pane

## Vision & DoD (5W1H)

**What.** A right-side drawer in the Reader that shows the active chapter's headings (H1 + H2 from the rendered HTML). Clicking a heading scrolls the chapter to that section. The drawer opens on hover over the toolbar trigger (rather than click) — a calm, opt-in pattern.

**Why.** For a chapter with 20+ headings, scrolling to find a specific section is friction. The outline lets the user jump to "the H2 about X" with one click.

**Who.** Anyone reading a long chapter with many sections. The drawer is only shown when the chapter has ≥ 2 headings (small chapters don't need it).

**When.** The Reader is showing a chapter. The outline is hidden by default; hover the toolbar's list icon to reveal it. Mouse-leave closes it after 300 ms.

**Where.** `src/components/OutlinePane.svelte`. Mounted by the Reader.

**How (acceptance / DoD).**
- The trigger icon is in the Reader toolbar (right side).
- Hovering the trigger opens the pane.
- The pane shows every H1 and H2 in the chapter, in document order.
- Clicking a heading scrolls the chapter to that section (smooth scroll, respects `prefers-reduced-motion`).
- The currently-visible heading is highlighted (driven by paragraph tracking).
- The pane's z-index is correct (above content, below the toolbar).

---

## How we implemented it

**What.** A Svelte 5 component that queries the chapter DOM for `[data-md-anchor]` elements whose kind is `h1` or `h2`, builds a list, and renders them as clickable buttons.

**Why this shape.** The anchor infrastructure was already in place (every heading has a `data-md-anchor="h2-N"`). The outline just consumes those anchors. No new parsing, no new state.

**When.** Mounted by the Reader. Subscribes to `paragraphFocus` to highlight the active heading.

**Where.**
- `src/components/OutlinePane.svelte` — the drawer.
- `src/lib/anchors.js` — the `data-md-anchor` injection.
- `src/lib/stores/state.js` — `paragraphFocus` store.

**How (tech).**
- **Anchor query**: `Array.from(document.querySelectorAll('[data-md-anchor^="h"]'))` returns all heading anchors. We filter to `h1` and `h2` only.
- **Click → scroll**: `document.getElementById(anchor)?.scrollIntoView({ behavior: 'smooth', block: 'start' })`. The reduced-motion media query is honored.
- **Active highlight**: `paragraphFocus` updates a `cursor` variable. The matching list item gets a `.active` class.
- **Hover-to-open**: a CSS-only trick. The trigger has `:hover` that flips a `data-open` attribute on the pane; CSS transitions the pane in. A 300 ms `mouseleave` delay prevents flicker.
- **Trigger placement**: lives inside `reader2-tools-right` (the right cluster of the Reader toolbar) so it's part of the responsive layout, not a fixed-position overlay.

**Gotchas.**
- The hover pattern breaks on touch devices. We add a click-to-toggle for touch via a `pointerdown` listener.
- Long chapter titles wrap; the outline's text-overflow handling uses a `title` attribute as a tooltip.
- The v1.1 disabled this feature for a few weeks because the CSS was missing; restored once the responsive sweep was redone.
