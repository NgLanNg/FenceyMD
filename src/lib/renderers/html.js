// HTML fence renderer — raw HTML pass-through. The reader is a local
// document viewer; the markdown is the user's own; live HTML is the
// whole point of a ```html fence (embeds, custom components, demos).
import { register } from '../registry.js';

register('html', {
  kind: 'fence',
  render(block, ctx) {
    const { pre, body } = block;
    if (!pre) return;
    const wrap = document.createElement('div');
    // Slides use a different wrapper class for fixed-viewport sizing.
    wrap.className = ctx.htmlWrapClass || ctx.wrapClassName || 'html-block';
    wrap.innerHTML = body;
    pre.replaceWith(wrap);
  },
});
