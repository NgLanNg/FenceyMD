// Math (katex) renderer — walks text nodes in the area and replaces
// `$…$` (inline) and `$$…$$` (block) with katex-rendered HTML. Theme-
// neutral: katex uses default colors, and we override those in app.css
// for both light and dark themes.
//
// We skip text inside <pre>, <code>, <script>, <style>, and any element
// with a `katex` ancestor — that keeps `$5.00` and code samples safe.
// The `math-skip` attribute is the user-facing escape hatch.
//
// When `ctx.isPdf === true` we DO NOT walk the DOM — the Rust PDF
// pipeline has already inlined katex's stylesheet and parses the
// chapter HTML itself. Calling katex.render() in the browser is
// unnecessary work for the PDF path and would compete with the
// in-Page thread for nothing.
import { register } from '../registry.js';

// Memoized lazy import of katex — keeps it out of the initial bundle until a
// chapter actually contains math.
let _katex = null;
async function getKatex() {
  if (!_katex) _katex = (await import('katex')).default;
  return _katex;
}

// `$$…$$` block math, non-greedy across newlines so adjacent blocks don't
// merge. Capture group 1 is the TeX source.
const BLOCK = /\$\$([\s\S]+?)\$\$/g;
// `$…$` inline math, single-line. The lookarounds guard the common false
// positives that make naive `$…$` matching unusable on prose:
//   (?<!\\)$  — a `\$` escape is literal currency, not a delimiter;
//   $(?!\s)   — no space right after the opening `$` (rules out "$ 5");
//   $(?!\d)   — no digit right after the closing `$` (rules out "$5.00").
const INLINE = /(?<!\\)\$(?!\s)([^\n$]+?)(?<!\\)\$(?!\d)/g;

/**
 * Collect `$…$` / `$$…$$` matches in `src`, sorted by start offset.
 *
 * @param {string} src - Raw text-node value to scan.
 * @returns {Array<{start:number,end:number,tex:string,display:boolean}>}
 *   `display:true` for block math. Block matches are collected first and any
 *   inline match falling inside a block's span is dropped — so the `$$` in a
 *   `$$…$$` block is never mistaken for two inline delimiters.
 */
function collectMatches(src) {
  const matches = [];
  let m;
  BLOCK.lastIndex = 0;
  while ((m = BLOCK.exec(src))) {
    matches.push({ start: m.index, end: m.index + m[0].length, tex: m[1], display: true });
  }
  INLINE.lastIndex = 0;
  while ((m = INLINE.exec(src))) {
    if (matches.some((b) => m.index >= b.start && m.index < b.end)) continue;
    matches.push({ start: m.index, end: m.index + m[0].length, tex: m[1], display: false });
  }
  matches.sort((a, b) => a.start - b.start);
  return matches;
}

/**
 * Build a DocumentFragment interleaving the plain-text runs of `src` with
 * katex-rendered `<span class="math-render">` spans.
 *
 * @param {string} src - The text-node value being replaced.
 * @param {object} katex - The resolved katex module (caller pre-loads it).
 * @returns {DocumentFragment|null} null when `src` contains no math, so the
 *   caller can leave the original text node untouched.
 *
 * On a katex parse error the span is replaced with the verbatim source slice
 * (delimiters included) so a malformed formula is visible and fixable rather
 * than silently dropped. `throwOnError:false` covers most cases; the
 * try/catch is the belt-and-braces guard for anything katex still throws on.
 */
function fragmentFromMath(src, katex) {
  const matches = collectMatches(src);
  if (!matches.length) return null;
  const frag = document.createDocumentFragment();
  let lastIndex = 0;
  for (const match of matches) {
    if (match.start > lastIndex) {
      frag.appendChild(document.createTextNode(src.slice(lastIndex, match.start)));
    }
    const span = document.createElement('span');
    span.className = 'math-render';
    try {
      katex.render(match.tex, span, { displayMode: match.display, throwOnError: false });
    } catch (_) {
      // Parse failure: leave the source text so the user can see it.
      span.replaceWith(document.createTextNode(src.slice(match.start, match.end)));
      lastIndex = match.end;
      continue;
    }
    frag.appendChild(span);
    lastIndex = match.end;
  }
  if (lastIndex < src.length) frag.appendChild(document.createTextNode(src.slice(lastIndex)));
  return frag;
}

/**
 * Find the text nodes under `area` that are eligible for math substitution.
 *
 * @param {Element} area - Chapter root to scan.
 * @returns {Text[]} Text nodes containing at least one `$`, excluding those
 *   inside code/markup or under a `[data-math-skip]` opt-out, and excluding
 *   nodes already inside a rendered `.katex` subtree (so re-running enhance()
 *   doesn't double-process). Collected up front rather than mutated during the
 *   walk, since replacing nodes mid-traversal would invalidate the walker.
 */
function walk(area) {
  const walker = document.createTreeWalker(area, NodeFilter.SHOW_TEXT, null, false);
  const targets = [];
  let node;
  while ((node = walker.nextNode())) {
    const parent = node.parentNode;
    if (!parent) continue;
    const tag = parent.nodeName;
    if (tag === 'CODE' || tag === 'PRE' || tag === 'SCRIPT' || tag === 'STYLE' || tag === 'KBD' || tag === 'NOSCRIPT') continue;
    if (parent.closest && parent.closest('.katex, pre, code, script, style, .katex-display')) continue;
    if (parent.closest && parent.closest('[data-math-skip]')) continue;
    if (!/\$/.test(node.nodeValue)) continue;
    targets.push(node);
  }
  return targets;
}

// Renderer def for math. Unlike the fence renderers it has no `block.pre`;
// the registry triggers it explicitly (see enhance() in registry.js) because
// math lives in text nodes spread through the chapter, not in a single fence.
register('math', {
  kind: 'inline', // not a fence — operates on text nodes
  load() { return getKatex(); },
  async render(block, ctx) {
    // PDF pipeline parses math itself; skip the browser walk.
    if (ctx.isPdf) return;
    const area = ctx.area || block.area;
    if (!area) return;
    const katex = await getKatex();
    const targets = walk(area);
    for (const textNode of targets) {
      const src = textNode.nodeValue;
      const frag = fragmentFromMath(src, katex);
      if (frag) textNode.parentNode.replaceChild(frag, textNode);
    }
  },
});

// Exposed for the legacy `enhance()` path — the reader imports
// `renderMathInArea` so the per-file enhance doesn't have to change.
/**
 * Walk `area` and substitute every `$…$`/`$$…$$` run with katex output, in
 * place. Mirrors the registered renderer's body but is callable directly by
 * legacy reader code that predates the registry. Idempotent: already-rendered
 * `.katex` nodes are excluded by `walk()`, so re-running is safe.
 *
 * @param {Element} area - Chapter root to process.
 * @returns {Promise<void>}
 */
export async function renderMathInArea(area) {
  const katex = await getKatex();
  const targets = walk(area);
  for (const textNode of targets) {
    const src = textNode.nodeValue;
    const frag = fragmentFromMath(src, katex);
    if (frag) textNode.parentNode.replaceChild(frag, textNode);
  }
}
