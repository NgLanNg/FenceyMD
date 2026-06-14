// Svelte action + DOM helper: makes any element a "zoomable" target.
// A hover button appears in the top-right corner; clicking it (or the
// element itself) opens the element in a viewport-sized overlay so
// the user can read a wide table or inspect a diagram at full size.
//
// Two entry points:
//   1. `applyZoomable(area)` — a plain DOM helper called from the
//      enhance() pipeline. Walks the area and decorates matching
//      selectors with a hover button. Idempotent.
//   2. `zoomable` — a Svelte action for ad-hoc use in templates.
//
// The actual overlay lives in ZoomOverlay.svelte; this module is
// the per-block plumbing (button, kind label, click → openZoom).

import { openZoom } from './zoom-state.js';

const LABELS = {
  image:   { aria: 'Open image fullscreen',   glyph: 'image' },
  table:   { aria: 'Open table fullscreen',   glyph: 'table' },
  diagram: { aria: 'Open diagram fullscreen', glyph: 'diagram' },
};

// Selectors that opt into zoom. Mirrors the brand's "calm reading"
// rule: visual blocks the user may legitimately want to look at
// larger, and nothing else. Code fences and math stay inline — they
// have their own affordances (copy / scroll) and are rarely the
// target of a zoom.
const ZOOMABLE_SELECTORS = [
  // Inline images in chapter markdown.
  '.chapter-markdown img',
  // Markdown pipe tables (showdown emits a plain <table>).
  '.chapter-markdown > table',
  // CSV cards (created by the csv renderer).
  '.csv-block',
  // Diagram blocks (mermaid / svg / excalidraw / html pass-through).
  '.mermaid-block', '.svg-block', '.excalidraw-block', '.html-block',
  // Raw fence <pre>s that weren't replaced (e.g. slide HTML).
  '.chapter-markdown > pre.slide-svg-block',
  '.chapter-markdown > pre.slide-html-block',
];

/**
 * Map an element to one of the three label kinds in `LABELS`. Order matters:
 * `img` is checked first, then table-like blocks; everything else (mermaid,
 * svg, excalidraw, html, slide fences) falls through to 'diagram'.
 * @param {Element} el a matched zoomable element.
 * @returns {'image'|'table'|'diagram'} the kind key for `LABELS`.
 */
function pickKind(el) {
  if (el.matches('img')) return 'image';
  if (el.matches('table, .csv-block')) return 'table';
  return 'diagram';
}

/**
 * Attach the hover zoom button to a single element and wire its click to
 * openZoom(). The workhorse behind both public entry points.
 *
 * Idempotent: the `data-zoomable-attached` marker guards against double
 * decoration when enhance() re-runs over already-processed DOM, so the
 * button is never appended twice.
 *
 * Trust note: `btn.innerHTML` is a fixed, author-controlled SVG string with
 * no interpolation — no untrusted markdown reaches it. The element being
 * decorated (`el`) may hold untrusted content, but we only append to it.
 * @param {Element} el the element to make zoomable.
 */
function decorate(el) {
  if (el.dataset.zoomableAttached === '1') return;
  el.dataset.zoomableAttached = '1';
  el.classList.add('zoomable');
  // Only force position: relative if the element is currently
  // `position: static` — preserve any pre-existing positioning.
  // (Reading computed style every run is fine; this runs once per
  // element per chapter navigation.)
  // eslint-disable-next-line no-undef
  if (getComputedStyle(el).position === 'static') {
    el.style.position = 'relative';
  }

  const kind = pickKind(el);
  const label = LABELS[kind] || LABELS.image;

  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'zoom-icon-btn';
  btn.setAttribute('aria-label', label.aria);
  btn.title = label.aria;
  btn.dataset.zoomableBtn = '1';
  btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <polyline points="15 3 21 3 21 9"/>
    <polyline points="9 21 3 21 3 15"/>
    <line x1="21" y1="3" x2="14" y2="10"/>
    <line x1="3" y1="3" x2="10" y2="10"/>
  </svg>`;
  btn.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    openZoom(el);
  });
  el.appendChild(btn);
}

/** Plain-DOM helper. Walks `area` and decorates every matching
 *  element with a hover button. Idempotent.
 *
 *  No-ops without a DOM (SSR / non-browser) or when `area` is falsy, so it
 *  is safe to call unconditionally from the enhance() pipeline.
 *  @param {?Element} area subtree root to scan against ZOOMABLE_SELECTORS. */
export function applyZoomable(area) {
  if (!area || typeof document === 'undefined') return;
  for (const sel of ZOOMABLE_SELECTORS) {
    const nodes = area.querySelectorAll(sel);
    for (const el of nodes) decorate(el);
  }
}

/** Svelte action — for ad-hoc per-template use. Same effect as
 *  `applyZoomable()` but on a single element passed via `use:zoomable`.
 *
 *  `opts` is accepted to satisfy the action signature but is currently
 *  unused. `destroy()` undoes decorate(): it removes the button and the
 *  marker so the node can be cleanly re-decorated if the action re-runs.
 *  @param {Element} node element the action is attached to.
 *  @param {object} [opts] reserved; no options are read today.
 *  @returns {{destroy: () => void}} Svelte action lifecycle object. */
export function zoomable(node, opts = {}) {
  decorate(node);
  return {
    destroy() {
      const btn = node.querySelector('[data-zoomable-btn]');
      if (btn) btn.remove();
      node.classList.remove('zoomable');
      delete node.dataset.zoomableAttached;
    },
  };
}
