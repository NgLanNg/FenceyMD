# Sanitization boundary

## Vision & DoD (5W1H)

**What.** Every piece of content rendered in the app goes through a sanitization step before it hits the DOM. Three surfaces, three profiles:
- **Chapter body** (showdown markdown output): strip `<script>`, `on*` handlers, `javascript:` URLs.
- **HTML fence** (` ```html ` blocks): same as chapter body, plus allow presentational tags the user might want (`details`, `summary`).
- **SVG fence** (` ```svg ` blocks): DOMPurify with the SVG profile, plus the same script/handler stripping.

Mermaid runs with `securityLevel: 'strict'` (a separate, internal hardening).

**Why.** A book can be from anywhere — a trusted source (the user wrote it), a downloaded file, an LLM output, a shared repo. Any of those can contain malicious payloads. The WebView is the app's IPC authority; an XSS in the rendered HTML would let the attacker call arbitrary Tauri commands (file ops, network). The sanitizer is the only thing standing between untrusted content and full IPC authority.

**Who.** Every user, every time. There's no opt-out.

**When.** Every render. The chapter body is sanitized once per render; the html/svg fences are sanitized inside their respective renderers.

**Where.** `src/lib/sanitize.js` is the central module. It exports `sanitizeBody(html)`, `sanitizeHtmlFence(html)`, `sanitizeSvgFence(svg)`.

**How (acceptance / DoD).**
- A `<script>alert(1)</script>` injected into a chapter body is stripped before reaching the DOM.
- An `<img onclick="alert(1)">` is rendered as `<img>` without the handler.
- An `<a href="javascript:alert(1)">` is rendered with the href stripped or normalized.
- The legitimate formatting (bold, italic, lists, code spans) is preserved.
- An `<iframe>` in a chapter body is stripped.
- The Mermaid renderer initializes with `securityLevel: 'strict'`.
- A planted payload in the e2e test is asserted to be absent from the rendered DOM.

---

## How we implemented it

**What.** DOMPurify wrappers for each surface. The wrappers set the appropriate `ALLOWED_TAGS` / `FORBID_TAGS` / `ALLOWED_ATTR` / `FORBID_ATTR` / `USE_PROFILES` for the surface, then call `DOMPurify.sanitize(html)`.

**Why this shape.** DOMPurify is the de-facto XSS sanitizer for the web. It's been audited, has no known XSS bypasses, and is small. We use it instead of writing our own parser because parsers are where the bugs live.

**When.** Every render. The cost is ~1 ms per chapter.

**Where.**
- `src/lib/sanitize.js` — the wrappers.
- `src/lib/markdown.js` — the chapter body is sanitized here.
- `src/lib/renderers/html.js` — the HTML fence is sanitized here.
- `src/lib/renderers/svg.js` — the SVG fence is sanitized here.
- `src/lib/renderers/mermaid.js` — initializes Mermaid with `securityLevel: 'strict'`.
- `e2e-test.mjs` — the planted-payload test.

**How (tech).**
- **DOMPurify**: v3. The `USE_PROFILES.html` profile is the default for chapter bodies; we extend it for the html fence (allow `details`, `summary`).
- **Forbid list**: `<script>`, `<iframe>`, `<object>`, `<embed>`, `<form>`, plus `on*` event handlers, plus `javascript:` URLs.
- **Mermaid**: `mermaid.initialize({ securityLevel: 'strict' })`. This disables Mermaid's own click-handler features and forces no inline scripts.
- **Sanitize-in-place**: we run DOMPurify on the html *before* inserting it via `{@html}`. Svelte's `{@html}` is the unsafe escape hatch; DOMPurify is what makes it safe.
- **The e2e probe**: a ` ```html ` fence with `<script>alert(1)</script>`, `<img onclick="x">`, `<a href="javascript:...">`, `<iframe src="...">`. The e2e test asserts the rendered DOM doesn't contain the dangerous bits.

**Gotchas.**
- DOMPurify is the right tool but it's a *parser*, not a policy. The policy (what's allowed) is in the wrapper functions; the parser (DOM walking) is in DOMPurify. We reviewed both.
- A `javascript:` URL inside `href` is a common bypass attempt. DOMPurify handles it, but only if you pass `ALLOWED_URI_REGEXP` carefully. Our config is strict.
- The Mermaid `securityLevel: 'strict'` is a Mermaid-internal setting, separate from DOMPurify. Both must be set.
- The PDF path is also at risk: the same HTML gets rendered by headless Chrome for printing. The PDF path runs the *same* sanitization before sending to Chrome.
