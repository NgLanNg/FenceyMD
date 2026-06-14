# Live HTML fences

## Vision & DoD (5W1H)

**What.** A `` ```html `` block in a chapter renders as real DOM. The author writes HTML; the reader sees that HTML styled by the app's theme. Common uses: an interactive demo with `<button>` and `<strong>`, a callout card, a definition list, a custom layout that markdown can't express.

**Why.** Markdown is a constrained language — sometimes you need a real `<table>` with merged cells, or a `<details>` element, or a `<figure>` with a figcaption. The HTML fence is the escape hatch.

**Who.** Authors who want richer layouts. The author is trusted (they wrote the file), but the content can also be untrusted (a book downloaded from the internet, LLM output, a shared repo) — so the HTML still goes through sanitization.

**When.** A chapter with ` ```html ` fences is opened. The fences are rendered in the same pass as the rest of the markdown (one render → one html string).

**Where.** `src/lib/renderers/html.js` is the renderer. The output is sanitized via DOMPurify with a profile that allows presentational HTML but strips `<script>`, `on*` handlers, and `javascript:` URLs.

**How (acceptance / DoD).**
- An HTML fence with `<button>` and `<strong>` renders those elements visually correct.
- A planted `<script>` or `onclick` is stripped.
- A planted `javascript:` URL is stripped.
- An HTML fence inside a `details` element preserves the presentational markup.
- The rendered output inherits the app's theme (light/dark via CSS vars).

---

## How we implemented it

**What.** The HTML renderer's `transform` function takes the fence body (the raw HTML inside the fence) and returns it unchanged, plus a `sanitize` flag. The downstream pipeline then runs DOMPurify on the result with the HTML-fence profile.

**Why this shape.** We could have parsed the HTML, walked it, and validated each tag — but DOMPurify is already in the dep tree for chapter-body sanitization. Reusing it for fences keeps the trust boundary in one place.

**When.** A fence is matched in `enhance()` → registry looks up the renderer → `transform` returns the HTML → pipeline wraps it with a marker class for the sanitizer to scope the rules.

**Where.**
- `src/lib/renderers/html.js` — the renderer.
- `src/lib/sanitize.js` — the sanitization profile.
- `src/lib/renderers/manifest.json` — declares `html` as a known fence.

**How (tech).**
- **Renderer**: `transform(htmlBody) → { html, className: 'html-block', sanitize: true }`.
- **Sanitizer**: DOMPurify with `{ ADD_TAGS: ['details', 'summary'], ADD_ATTR: ['open'], FORBID_TAGS: ['script', 'iframe'], FORBID_ATTR: ['onload', 'onclick', 'onerror'] }` (extended for the demo's case).
- **Probe-verified**: a unit-style assertion in e2e-test.mjs plants `<script>alert(1)</script>`, `<img onclick="x">`, `<a href="javascript:alert(1)">`, and `<iframe>`, and asserts the rendered DOM doesn't contain the dangerous bits.

**Gotchas.**
- The HTML fence is the **most security-sensitive** surface in the app, because the author is also potentially untrusted. The DOMPurify profile is the only thing standing between a malicious book and full IPC authority.
- We considered `<iframe sandbox>` for embeddable content but rejected it — adds attack surface, hard to harden. The HTML fence is for content the author writes inline, not for embedding third-party sites.
- Mermaid runs in `strict` mode; the same treatment for HTML would be too restrictive (no inline styles, no inline svg, no rich layouts). We have a fence-specific profile instead.
