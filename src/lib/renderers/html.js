// HTML fence renderer — renders real HTML from a ```html fence (custom
// layouts, styled demos). The body is sanitized first (see sanitize.js): the
// markdown may be untrusted (shared / downloaded / LLM-generated books) and
// this runs in a WebView with IPC authority, so script-execution vectors must
// be stripped while the visual markup is preserved.
import { register } from '../registry.js';
import { sanitizeHtml } from '../sanitize.js';

// Renderer def for ```html fences. `render` replaces `block.pre` with a
// `.html-block` wrapper whose innerHTML is the SANITIZED fence body — never
// assign `body` raw (see file header / sanitize.js: this is the IPC trust
// boundary). `ctx.htmlWrapClass`/`ctx.wrapClassName` let slides override the
// wrapper class for fixed-viewport sizing.
register('html', {
  kind: 'fence',
  render(block, ctx) {
    const { pre, body } = block;
    if (!pre) return;
    const wrap = document.createElement('div');
    // Slides use a different wrapper class for fixed-viewport sizing.
    wrap.className = ctx.htmlWrapClass || ctx.wrapClassName || 'html-block';
    wrap.innerHTML = sanitizeHtml(body);
    pre.replaceWith(wrap);
  },
});
