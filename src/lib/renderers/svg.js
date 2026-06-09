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

const SVG_NS = 'http://www.w3.org/2000/svg';

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
    const parser = new DOMParser();
    const parsed = parser.parseFromString(src, 'image/svg+xml');
    if (parsed.querySelector('parsererror')) {
      wrap.classList.add('svg-block-error');
      wrap.textContent = src;
    } else {
      const srcSvg = parsed.documentElement;
      const inner = srcSvg.innerHTML;
      const svgEl = document.createElementNS(SVG_NS, 'svg');
      svgEl.setAttribute('viewBox', srcSvg.getAttribute('viewBox') || '0 0 200 80');
      // Explicit xmlns keeps the rendered SVG in the SVG namespace
      // when serialized into the surrounding HTML namespace.
      svgEl.setAttribute('xmlns', SVG_NS);
      svgEl.innerHTML = inner;
      wrap.appendChild(svgEl);
    }
    pre.replaceWith(wrap);
  },
});

export const SVG_NS_EXPORT = SVG_NS;
