# Excalidraw

## Vision & DoD (5W1H)

**What.** A ` ```excalidraw ` block renders as an interactive Excalidraw canvas — the hand-drawn diagram tool. The reader sees the drawing, can pan/zoom, and (if the author enabled edit mode) can modify it. The drawing is saved back into the same `.md` file as JSON.

**Why.** Some diagrams need the hand-drawn feel of a whiteboard sketch, not the precise lines of mermaid. Excalidraw is the de-facto tool for this. Embedding it in a book means the diagram is part of the book, not a separate file.

**Who.** Authors who want hand-drawn diagrams. Readers who want to interact with or edit them.

**When.** A chapter with ` ```excalidraw ` fences opens. The Excalidraw runtime loads lazily on first use (~1 MB).

**Where.** `src/lib/renderers/excalidraw.js` is the renderer. `src/components/ExcalidrawViewer.svelte` is the runtime viewer that mounts the Excalidraw canvas.

**How (acceptance / DoD).**
- An Excalidraw block mounts a canvas with the saved scene.
- Read mode: the reader can pan/zoom but not modify.
- Edit mode (toolbar pencil icon): the reader can draw; saving writes the JSON back to the markdown file.
- The saved scene is a JSON blob in the fence body — not a separate file.
- The runtime state (selection, scroll, panel sizes) is **stripped** from the saved JSON; only the document (shapes, lines, text) is persisted.

---

## How we implemented it

**What.** A renderer that emits a placeholder (`<div class="excalidraw-block">`), and a Svelte component (`ExcalidrawViewer.svelte`) that mounts the Excalidraw runtime into it. The runtime reads the saved scene JSON from the fence body, and on save, writes the new scene back into the same body.

**Why this shape.** Excalidraw is a heavy JS library with its own internal state model. We don't try to render it as static SVG (that would lose the hand-drawn feel and the interactivity). We mount the actual Excalidraw component, give it the saved scene, and let the user interact.

**When.** Lazy-loaded. First Excalidraw block triggers a ~1 MB download.

**Where.**
- `src/lib/renderers/excalidraw.js` — the renderer placeholder.
- `src/components/ExcalidrawViewer.svelte` — the runtime viewer.
- `src-tauri/src/main.rs` — `locate_excalidraw_block` and `update_excalidraw_block` for save/load.

**How (tech).**
- **Renderer**: `transform(jsonBody) → { html: '<div class="excalidraw-block" data-source="..."></div>', sanitize: false }`. No DOMPurify — the JSON is parsed, not rendered as HTML.
- **Runtime**: `Excalidraw` component from `@excalidraw/excalidraw` is dynamically imported in `ExcalidrawViewer.svelte`. On mount, the component reads `data-source` from the placeholder and calls `initialData`.
- **Save**: the viewer's `onChange` fires; we collect the scene, serialize as JSON, and call `update_excalidraw_block(content, block_index, new_json)` Rust command. The Rust side locates the block (by finding the matching fence) and updates the body.
- **State stripping**: Excalidraw's saved data has `appState` (selection, scroll, etc.) and `elements` (the actual drawing). On save, we only persist `elements`. This is the "editor JSON save: filter to allowlist" pattern.

**Gotchas.**
- Excalidraw's runtime state is *huge* if you save the whole object — 10x larger than the actual document. The allowlist filter is the only thing keeping the markdown file size sane.
- The Rust save command does string-level surgery on the markdown to update the fence body. We use `locate_excalidraw_block` to find the byte range of the block, then a simple substring replacement. This is fast (O(n)) and reliable for the typical case.
- PDF path used to render Excalidraw as an embedded SVG; the export pipeline re-renders the live canvas, inlines the SVG, and ships to Rust. The fix to a "blank Excalidraw in PDF" bug was to wait for the canvas to finish painting before snapshotting.
