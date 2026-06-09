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

let _katex = null;
async function getKatex() {
  if (!_katex) _katex = (await import('katex')).default;
  return _katex;
}

const BLOCK = /\$\$([\s\S]+?)\$\$/g;
const INLINE = /(?<!\\)\$(?!\s)([^\n$]+?)(?<!\\)\$(?!\d)/g;

// Collect `$…$` / `$$…$$` matches in `src`, returning them sorted by
// start offset. Block matches shadow overlapping inline matches.
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

// Build a DocumentFragment of plain text + katex-rendered spans. Returns
// null when no math was found.
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
export async function renderMathInArea(area) {
  const katex = await getKatex();
  const targets = walk(area);
  for (const textNode of targets) {
    const src = textNode.nodeValue;
    const frag = fragmentFromMath(src, katex);
    if (frag) textNode.parentNode.replaceChild(frag, textNode);
  }
}
