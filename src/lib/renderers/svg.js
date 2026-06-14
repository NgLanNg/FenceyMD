// SVG fence renderer — preserves the namespace-correct DOMParser re-wrap
// that was the bug Phase 1's registry is meant to prevent from recurring.
//
// Pipeline: parse the fence source as image/svg+xml, extract the inner
// elements, re-wrap them in a freshly-created <svg> (createElementNS) so
// they end up in the SVG namespace regardless of where the host DOM
// inserts them. Malformed SVG yields a `<parsererror>` document; in
// that case we fall back to the raw source as text so the user can see
// and fix it.
import { register } from '../registry.js';
import { sanitizeSvg } from '../sanitize.js';

const SVG_NS = 'http://www.w3.org/2000/svg';

// Renderer def for ```svg fences. `render` replaces `block.pre` with a
// `.svg-block` wrapper containing a re-namespaced, sanitized <svg>. On a
// parse error (or empty-after-sanitize body) it shows the raw source in an
// `.svg-block-error` wrapper instead. `ctx.svgWrapClass`/`ctx.wrapClassName`
// let slides override the wrapper class for fixed-viewport sizing.
register('svg', {
  kind: 'fence',
  render(block, ctx) {
    const { pre, body } = block;
    if (!pre) return;
    const src = body.trim();
    // Slides wrap with a different class so the fixed 16:9 viewport
    // can size the SVG appropriately. The reader uses the default.
    const wrap = document.createElement('div');
    wrap.className = ctx.svgWrapClass || ctx.wrapClassName || 'svg-block';
    // Sanitize the WHOLE <svg> document first (the fence source is untrusted —
    // see sanitize.js): script/on*-handlers/foreignObject-smuggled HTML are
    // stripped, presentational shapes/paths/text/filters survive. We sanitize
    // the full element (not the inner fragment) so DOMPurify parses it in SVG
    // context — a bare `<rect/>` fragment would be dropped as unknown HTML.
    const clean = sanitizeSvg(src);
    const parser = new DOMParser();
    const parsed = parser.parseFromString(clean, 'image/svg+xml');
    if (parsed.querySelector('parsererror') || !clean.trim()) {
      wrap.classList.add('svg-block-error');
      wrap.textContent = src; // show the original source so the user can fix it
    } else {
      const srcSvg = parsed.documentElement;
      const svgEl = document.createElementNS(SVG_NS, 'svg');
      // Carry over the source's sizing attributes. Previously only `viewBox`
      // was copied (with a bogus `0 0 200 80` default), so an SVG authored with
      // `width`/`height` but no viewBox — a common style — was squashed into a
      // 200×80 box at the wrong scale. Preserve width/height/preserveAspectRatio
      // and only synthesize a viewBox when none of those provide sizing.
      for (const attr of ['viewBox', 'width', 'height', 'preserveAspectRatio']) {
        const v = srcSvg.getAttribute(attr);
        if (v != null) svgEl.setAttribute(attr, v);
      }
      if (!svgEl.hasAttribute('viewBox') && !svgEl.hasAttribute('width') && !svgEl.hasAttribute('height')) {
        svgEl.setAttribute('viewBox', '0 0 200 80'); // last-resort sizing
      }
      // Explicit xmlns keeps the rendered SVG in the SVG namespace
      // when serialized into the surrounding HTML namespace.
      svgEl.setAttribute('xmlns', SVG_NS);
      svgEl.innerHTML = srcSvg.innerHTML; // already sanitized above
      wrap.appendChild(svgEl);
    }
    pre.replaceWith(wrap);
  },
});

// The SVG namespace URI, re-exported for consumers that build SVG nodes
// outside this module and need the exact same string (e.g. PDF/slide paths
// that re-wrap diagrams). Kept in sync with the local `SVG_NS` constant.
export const SVG_NS_EXPORT = SVG_NS;
