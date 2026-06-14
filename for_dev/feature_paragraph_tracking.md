# Paragraph tracking

## Vision & DoD (5W1H)

**What.** When the user is editing a chapter, the Reader knows which paragraph (or block) the cursor is currently in. This is exposed as a "paragraph-focus" event with the block's anchor (e.g. `para-12`, `code-7`, `mermaid-3:nodeA`). The OutlinePane can use this to highlight the active block in the chapter's heading list.

**Why.** The user's mental model of "where am I in the document" should match the sidebar/outline's view. Without paragraph tracking, the outline can only show "the chapter is open," not "the user is reading the section on X."

**Who.** Anyone editing or reading. The outline pane consumes the event; future v2 features (anchor-based edit) also depend on it.

**When.** In edit mode, every cursor movement fires a `paragraph-focus` event. In read mode, every scroll-position change fires one. The events are debounced (the user shouldn't see the outline flickering on every pixel of scroll).

**Where.** `src/components/Editor.svelte` and `src/components/Reader.svelte` emit the events. The handler is in `App.svelte` which forwards to the `paragraphFocus` store.

**How (acceptance / DoD).**
- In edit mode, the paragraph anchor at the cursor position is exposed.
- In read mode, the paragraph anchor at the visible scroll position is exposed.
- The event payload is `{ anchor, kind }` (e.g. `para-12, "para"`).
- The event is debounced (no more than 5 per second).
- The outline pane reflects the active block.

---

## How we implemented it

**What.** A Tiptap `onUpdate` (edit mode) and a scroll observer (read mode) that:
1. Reads the cursor / visible block position.
2. Looks up the nearest ancestor with a `data-md-anchor` attribute.
3. Emits the event with the anchor and kind.

**Why this shape.** The anchor infrastructure (`data-md-anchor` on every renderable block) was already in place from the anchor feature. Paragraph tracking is just "tell me which anchor the cursor/scroll is at."

**When.** Every editor update (debounced) and every scroll change (debounced).

**Where.**
- `src/components/Editor.svelte` — the edit-mode emitter.
- `src/components/Reader.svelte` — the read-mode emitter.
- `src/lib/anchors.js` — the anchor-walking helper.
- `src/App.svelte` — the event listener (forwards to store).

**How (tech).**
- **Cursor → anchor**: `editor.view.domAtPos(state.selection.head).node` returns the DOM node; we walk up looking for `data-md-anchor`.
- **Scroll → anchor**: we use `IntersectionObserver` to find the first block whose top is above 50% viewport. The block's `data-md-anchor` is the active one.
- **Debounce**: a 200 ms debounce on both paths. The outline shouldn't flicker.
- **Event shape**: `CustomEvent('paragraph-focus', { detail: { anchor, kind } })`. Bubbles up to `App.svelte` which writes to the `paragraphFocus` store.

**Gotchas.**
- The cursor at the *start* of a paragraph maps to the previous paragraph's anchor (because the cursor is "at" the boundary). This is usually fine, but for very short paragraphs the outline jumps one block on every arrow keypress. We snap to the next block if the cursor is within 5px of the block's top.
- A nested block (e.g. a code block inside a list item) has two possible ancestors; we pick the innermost.
- The intersection-observer ratio needs tuning. 0.5 means "the block is at the top half of the viewport"; some users prefer 0.25 (more aggressive).
