# Live SVG fences

## Vision & DoD (5W1H)

**What.** A `` ```svg `` block renders as the actual graphic. The author writes `<svg viewBox="0 0 100 100">...</svg>` inside the fence; the reader sees the graphic with proper viewBox, namespace, and theme-aware colors.

**Why.** Many books need inline diagrams that aren't mermaid — architecture sketches, mathematical figures, custom icons. SVG is the right tool; markdown can't express it.

**Who.** Authors who want to drop a graphic directly into a chapter without uploading an image file.

**When.** A chapter with ` ```svg ` fences is opened.

**Where.** `src/lib/renderers/svg.js` is the renderer. Sanitization profile is separate from the HTML profile — SVG has its own attack surface (XSS via `<script>` inside `<svg>`, foreign content via `<foreignObject>`).

**How (acceptance / DoD).**
- An SVG fence renders the graphic with correct viewBox.
- The `<svg xmlns="...">` namespace is set (browsers tolerate but markdown-rendered HTML often strips it).
- A planted `<script>` inside the SVG is stripped.
- The graphic respects the app's theme (light/dark via CSS vars).
- The graphic scales to fit the chapter content width.

---

## How we implemented it

**What.** The SVG renderer's `transform` returns the body as-is with a class for the sanitizer to scope rules. The renderer also injects the SVG namespace if missing.

**Why this shape.** Same as the HTML fence — reuse DOMPurify with an SVG-specific profile.

**When.** Same pipeline as HTML.

**Where.**
- `src/lib/renderers/svg.js` — the renderer + namespace injection.
- `src/lib/sanitize.js` — the SVG profile.

**How (tech).**
- **Renderer**: `transform(svgBody) → { html: injectNamespace(svgBody), className: 'svg-block', sanitize: true }`.
- **Namespace injection**: if the `<svg` tag doesn't have `xmlns="http://www.w3.org/2000/svg"`, prepend it. Browsers *usually* tolerate missing namespace but markdown-helper libraries sometimes strip it.
- **Sanitizer profile**: `USE_PROFILES: { svg: true, svgFilters: true }` plus the same script/handler stripping as the HTML profile.
- **CSS**: a `.svg-block` class with `max-width: 100%` and `height: auto` so the graphic scales to the content column.

**Gotchas.**
- The PDF path used to print dark-on-white SVGs when the app was in dark mode. Fixed by forcing a light palette in the PDF renderer.
- Some authors put their SVG inside an HTML fence; the SVG renderer doesn't fire then. The fix is to use ` ```svg ` and let the pipeline do the right thing.
