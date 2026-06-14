# Diagram export (PNG)

## Vision & DoD (5W1H)

**What.** For a rendered Mermaid, Excalidraw, or SVG diagram, the user can click a "copy as PNG" or "download as PNG" action. The diagram is rasterized to a PNG (preserving its on-screen appearance) and pushed to the clipboard or saved to disk.

**Why.** Some readers want to drop a diagram into a doc, a slide deck, or an email. The diagram is a real visual artifact; copying the source code isn't useful.

**Who.** Anyone who needs to share a diagram outside the app.

**When.** The user hovers a rendered diagram in the Reader and clicks the copy/download icon. The action is per-diagram; the user picks which one to export.

**Where.** The action is on the diagram's render container (mermaid, excalidraw, svg). `src/lib/diagram-export.js` is the helper.

**How (acceptance / DoD).**
- Hovering a diagram reveals the copy/download icons.
- Copy pushes the PNG to the clipboard.
- Download saves the PNG to a user-chosen path (native Save dialog).
- The PNG preserves the diagram's current theme (light or dark).
- The PNG is at native resolution (Retina 2×).

---

## How we implemented it

**What.** A small JS helper that, given a DOM node (the diagram's container), rasterizes it to a canvas and exports a PNG.

**Why this shape.** We use the browser's built-in `html2canvas`-like trick: serialize the SVG, draw it on a `<canvas>`, then `canvas.toDataURL('image/png')`. The WebView can't `<a download>`, so the file save goes through Rust.

**When.** Triggered by hovering + clicking a diagram's copy/download button.

**Where.**
- `src/lib/diagram-export.js` — the helper.
- `src/lib/tauri.js` — `saveExport` (file save) wrapper.
- `src-tauri/src/main.rs` — `save_export` Tauri command.

**How (tech).**
- **SVG → PNG**: `new XMLSerializer().serializeToString(svg)` → `new Image()` → `ctx.drawImage()` → `canvas.toBlob('image/png')`. The browser handles SVG rasterization natively.
- **Clipboard**: the blob is pushed via `navigator.clipboard.write([new ClipboardItem({'image/png': blob})])`. The clipboard API only supports image blobs on secure contexts (https or localhost), which the WebView provides.
- **Save to disk**: the blob is base64-encoded and sent to Rust's `save_export(path, base64, mime)`. Rust writes the bytes to the user's chosen path. The WebView can't `<a download>` (WKWebView limitation) so this is the only way.
- **Theme awareness**: when the app theme flips, the export uses the current computed styles for the diagram (via `getComputedStyle(svg).color` etc.). For SVG, this is moot (the SVG carries its own colors), but for Mermaid's HTML output it's needed.

**Gotchas.**
- The WebView's clipboard API can be unreliable on some macOS versions; we fall back to a "save to file" suggestion if `navigator.clipboard.write` rejects.
- For very large diagrams, the rasterization can hit canvas size limits (~16k px). We downscale if needed.
- The diagram-export doesn't include the diagram's *background* (transparent PNG) — useful for embedding in slides. The user can re-export with a different background in future versions.
