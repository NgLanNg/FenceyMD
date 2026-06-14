// Markdown rendering + post-processing. showdown is loaded eagerly
// (it's the core renderer). Heavy deps (katex, shiki, mermaid,
// excalidraw, highlight.js) are code-split and lazy-loaded by the
// individual renderers registered in `src/lib/renderers/`. The
// `enhance()` entry point here is a thin loop over the registered
// renderers — see `src/lib/registry.js` for the dispatch logic.
//
// Phase 2 of PLAN.md collapses the per-language if-chain that used
// to live here into a single dispatch. Adding a new fence type means
// dropping a new file in `renderers/` and adding an entry to
// `renderers/manifest.json` — no changes needed in this file.
import showdown from 'showdown';
import { enhance as registryEnhance } from './registry.js';
import { sanitizeHtml } from './sanitize.js';
// Import the renderer set so it registers itself with the registry
// at module init. Consumers that want a different renderer set can
// import specific files instead.
import './renderers/index.js';

const converter = new showdown.Converter({
  tables: true,
  tasklists: true,
  strikethrough: true,
  simplifiedAutoLink: true,
  openLinksInNewWindow: true,
  ghCodeBlocks: true,
});

/**
 * Render a markdown source string to a sanitized HTML body string.
 *
 * @param {string} text - Raw chapter markdown (may be untrusted; see WHY below).
 * @returns {string} Sanitized HTML, ready to be inserted via Svelte `{@html}`
 *   and then post-processed by `enhance()`.
 *
 * Note: the returned HTML is NOT yet enhanced — fence bodies are still inert
 * `<pre><code class="language-*">` placeholders. Call `enhance()` on the
 * mounted DOM node afterwards to activate syntax highlighting, math, diagrams,
 * etc. Empty/whitespace input renders to an empty body.
 */
export function renderMarkdown(text) {
  // showdown passes raw HTML in the source straight through, and the result is
  // inserted via Svelte's {@html}. Since chapter markdown can be untrusted
  // (shared / downloaded / LLM-generated) and this runs in a WebView with IPC
  // authority, sanitize the rendered body to strip script-execution vectors.
  // This runs BEFORE enhance(), so the fence placeholders (<pre><code
  // class="language-*">) and heading ids/classes the pipeline depends on are
  // preserved — only executable markup is removed. The fence renderers
  // (svg/html) sanitize their own rendered output separately.
  return sanitizeHtml(converter.makeHtml(text));
}

/**
 * Enhance freshly-rendered markdown inside `area`: syntax highlighting,
 * inline SVG, mermaid diagrams, excalidraw scenes, math, and HTML
 * pass-through. The dispatch goes through the registry — there are
 * no per-language `if` branches here.
 *
 * `meta` (optional) is forwarded to renderers that need to save back
 * to the source file (Excalidraw uses it to know which `.md` file to
 * update on save).
 */
export function enhance(area, meta = {}) {
  return registryEnhance(area, meta);
}
