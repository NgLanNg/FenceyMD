# Anchor infrastructure

## Vision & DoD (5W1H)

**What.** Every renderable block in a chapter (paragraph, code block, mermaid node, math equation, CSV cell, heading, list, table, image, excalidraw element) gets a stable `data-md-anchor` attribute. The anchor is unique within the chapter and survives re-renders. Anchors are used by:
- The paragraph-tracking feature (the active block is the anchor at the cursor/scroll).
- The outline pane (clicking a heading scrolls to the heading's anchor).
- The cross-chapter search (results carry the anchor; navigation goes to the anchor, not just the chapter).
- The v2 anchor-based edit feature (an agent returns a diff for a specific anchor; the editor applies it surgically).

**Why.** Without stable IDs, every feature that needs to refer to "this specific block" has to use fragile text-match or byte-offset. With anchors, the round-trip is exact: "anchor `para-12`" means the same block in every render.

**Who.** Any feature that needs to identify a specific block. The user benefits indirectly (the outline, search, paragraph tracking all work).

**When.** Every render. The anchor injection is part of the markdown `enhance()` post-process.

**Where.** `src/lib/anchors.js` is the injection logic. Each renderer's output is post-processed to add anchors. The e2e test asserts that anchors are present and contiguous.

**How (acceptance / DoD).**
- Every renderable block has a `data-md-anchor` attribute.
- Anchors are unique within a chapter.
- Anchors are stable across re-renders of the same content.
- Anchors carry a `kind` prefix (e.g. `para-`, `code-`, `mermaid-`, `h2-`, `math-`, `csv-cell-`).
- Nested blocks (e.g. a code block inside a list) have anchors at the innermost level.

---

## How we implemented it

**What.** A post-processing pass that walks the rendered HTML and adds `data-md-anchor` attributes to every renderable block. The walk is a single-pass DOM traversal; each block gets a counter and a kind tag.

**Why this shape.** We could have done the injection in the showdown extension layer (before the HTML is built), but DOM is easier to walk than the showdown AST. We do it after the markdown is rendered, but before Svelte inserts it via `{@html}`.

**When.** Every chapter render. The cost is ~2 ms per chapter (DOM walk).

**Where.**
- `src/lib/anchors.js` — the walk + counter logic.
- `src/lib/markdown.js` — calls `enhance()` after showdown, which runs the anchor pass.
- Each renderer in `src/lib/renderers/` returns HTML with `data-md-anchor` already attached for its own kind.

**How (tech).**
- **Walk**: a TreeWalker on the chapter root. For each block-level element (P, H1, H2, H3, UL, OL, TABLE, PRE, BLOCKQUOTE, etc.), assign a counter and a kind tag.
- **Counter scheme**: `para-N`, `code-N`, `h2-N`, `mermaid-N`, `math-N`, `csv-row-N`, etc. The counter resets per render (so a stable content gives the same anchors).
- **Stability rule**: a block's anchor is derived from its content hash + position. We use a simple counter for now (good enough for most cases); a content-hash fallback exists for cases where the same block has the same content but different positions.
- **Nested blocks**: a code block inside a list item has a single anchor on the code block (the innermost). The list item itself has its own anchor. The walk assigns counters in document order.
- **Mermaid sub-anchors**: each node in a mermaid diagram gets `data-md-anchor="mermaid-3:nodeA"`. The `:nodeA` suffix is the mermaid node ID. This is what v2 anchor-based edit will use to surgically update one node.

**Gotchas.**
- A block with the same content but different positions gets different anchors (counter-based). For v2's diff-surgery, we may need content-hash fallback to be robust.
- The walk happens *after* the DOM is built (i.e. on the html string), so we use a lightweight HTML parser (regex-based) instead of a real DOM. This is fragile but works for the cases we have.
- Mermaid sub-anchors require mermaid to finish rendering before we can read the node IDs. We add the parent `data-md-anchor="mermaid-N"` synchronously, and the sub-anchors asynchronously after mermaid renders.
