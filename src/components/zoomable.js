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

function pickKind(el) {
  if (el.matches('img')) return 'image';
  if (el.matches('table, .csv-block')) return 'table';
  return 'diagram';
}

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
 *  element with a hover button. Idempotent. */
export function applyZoomable(area) {
  if (!area || typeof document === 'undefined') return;
  for (const sel of ZOOMABLE_SELECTORS) {
    const nodes = area.querySelectorAll(sel);
    for (const el of nodes) decorate(el);
  }
}

/** Svelte action — for ad-hoc per-template use. Same effect as
 *  `applyZoomable()` but on a single element passed via `use:zoomable`. */
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
