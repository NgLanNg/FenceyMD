// Shiki fence renderer — code-fence syntax highlighter. Lazy-loads shiki
// with the language + theme bundle.
//
// For the default 'github' code theme (#8 in ROADMAP v1.1), we use the
// dual-theme trick: each token carries `style="color:#light;
// --shiki-dark:#dark"`, and the `.shiki-block` rule in app.css swaps
// which color is active based on the root data-theme. This means the
// chapter re-themes instantly when the user toggles light/dark mode.
//
// For the 'nord' code theme (#8), we use a single-theme render — shiki
// emits `style="color:#nordToken"` only. The CSS forces the nord
// background and the spans keep their inline token colors regardless of
// the app's light/dark mode. The user picked a deliberately dark code
// style, so we honor that even when the rest of the app is in light mode.
//
// If the user changes the code theme while a chapter is on screen, the
// `setCodeTheme()` export walks the live DOM, restores each shiki
// block to its original `<pre><code>` (carried as `data-source`), and
// re-highlights. The reader doesn't have to reload.
//
// This is the `defaultFor: "code"` renderer — the registry falls
// back to it for unknown fence languages (```js, ```ts, etc.).
//
// Idempotency: because the registry calls this renderer once per
// unknown-lang block, we (a) skip elements that are already inside
// a shiki-rendered pre, and (b) keep a per-area WeakSet of pres
// we've already touched. Calling shiki twice on the same block
// would re-highlight an already-highlighted block (text → text
// round-trip), which is wrong.
import { register, wrapWithCopyButton } from '../registry.js';

// Module-level singletons shared across every call. `_shiki` is the
// resolved highlighter; `_shikiReady` is its in-flight promise (so
// concurrent callers await the same import, not N parallel ones).
// `_codeTheme` / `_dark` are the live theme state the retheme exports
// read — kept at module scope so the registry's per-block `render()`
// and the standalone theme-switch exports all see one source of truth.
let _shiki = null;
let _shikiReady = null;
let _codeTheme = 'github';
let _dark = false;

/**
 * Lazily create (and memoize) the shiki highlighter with our bundled
 * theme + language set. The first caller kicks off the dynamic import;
 * every later caller — and any concurrent caller racing the first —
 * awaits the same `_shikiReady` promise so we only build one highlighter.
 *
 * @returns {Promise<{ shiki: object, highlighter: object }>} the shiki
 *   module plus the configured highlighter instance.
 */
async function getShiki() {
  if (_shiki) return _shiki;
  if (!_shikiReady) {
    _shikiReady = (async () => {
      const shiki = await import('shiki');
      // Bundle all three themes we ship. Adding more later is a one-line
      // change here + a new branch in `pickThemes()` + a matching CSS
      // rule for [data-code-theme="..."].
      const highlighter = await shiki.createHighlighter({
        themes: ['github-light', 'github-dark', 'nord'],
        langs: [
          'js', 'jsx', 'ts', 'tsx', 'json', 'css', 'html', 'xml',
          'py', 'rs', 'go', 'java', 'c', 'cpp', 'cs',
          'sql', 'bash', 'shell', 'sh',
          'yaml', 'yml', 'toml', 'md', 'markdown',
          'php', 'rb', 'kt', 'swift', 'dart', 'lua',
        ],
      });
      _shiki = { shiki, highlighter };
      return _shiki;
    })();
  }
  return _shikiReady;
}

// Pick the shiki `themes` option based on the active code theme. The
// `defaultColor` controls which side of the dual theme shiki considers
// the "primary" one for the wrapper's own background/color.
//
// Shiki v1.29's `themes` option is a *Record* of named entries
// (e.g. `{ light: 'github-light', dark: 'github-dark' }`) — passing
// a bare string makes shiki index it as `[0]` and emit the
// "Theme `n` not found" warning. We always return a Record; for the
// 'nord' path we still register it as a single-entry record so the
// produced HTML uses nord's colors and the CSS rule for
// [data-code-theme="nord"] controls the wrapper background.
//
// @param {string}  codeTheme — active code theme ('github' | 'nord').
// @param {boolean} dark      — app dark mode; only consulted on the
//                              github path (nord is always dark).
// @returns {{ themes: object, defaultColor: string }} shiki options.
function pickThemes(codeTheme, dark) {
  if (codeTheme === 'nord') {
    // Single dark theme — shiki emits one color per span and uses
    // nord's own background. CSS for [data-code-theme="nord"] forces
    // #2E3440 as the wrapper background regardless of app mode.
    return { themes: { dark: 'nord' }, defaultColor: 'dark' };
  }
  // Default: dual github-light / github-dark. Spans carry
  // `--shiki-dark` inline variables; CSS swaps which is active.
  return { themes: { light: 'github-light', dark: 'github-dark' }, defaultColor: dark ? 'dark' : 'light' };
}

/**
 * Resolve the shiki grammar name from a `<code>` element's
 * `language-X` class. Falls back to 'text' when the class is absent or
 * names a grammar we didn't bundle in `getShiki()` — shiki throws on an
 * unknown lang, so we never pass one through.
 *
 * @param {Element} codeEl      — the original `<code>` node.
 * @param {object}  highlighter — the shiki highlighter (for its loaded-lang list).
 * @returns {string} a grammar name guaranteed to be loaded, or 'text'.
 */
function pickLang(codeEl, highlighter) {
  const cls = [...codeEl.classList].find((c) => c.startsWith('language-'));
  if (!cls) return 'text';
  const lang = cls.slice('language-'.length).toLowerCase();
  return highlighter.getLoadedLanguages().includes(lang) ? lang : 'text';
}

/**
 * True when a `<code>` block is a diagram/visual language that shiki
 * must NOT highlight (mermaid/svg/html/excalidraw have their own
 * renderers). Used to exclude these from the highlight sweep so we
 * don't clobber a diagram source with syntax-colored text.
 *
 * @param {Element} codeEl — the `<code>` node to classify.
 * @returns {boolean}
 */
function isDiagramLang(codeEl) {
  return codeEl.classList.contains('language-mermaid')
      || codeEl.classList.contains('language-svg')
      || codeEl.classList.contains('language-html')
      || codeEl.classList.contains('language-excalidraw');
}

// Per-pre dedupe set (see the file header's "Idempotency" note). A
// WeakSet so entries vanish when the DOM node is GC'd after chapter
// navigation — no manual cleanup, no leak across chapters.
const _donePres = new WeakSet();

/**
 * Highlight every eligible `<pre><code>` inside `area`, replacing each
 * with a shiki-rendered `.shiki-block` wrapped in a copy button.
 *
 * Eligibility skips diagram langs, blocks already inside a rendered
 * shiki/diagram pre, and pres recorded in `_donePres` — so a second
 * call on the same area is a no-op (the registry may dispatch us more
 * than once per chapter).
 *
 * @param {Element} area — chapter root to scan.
 * @param {boolean} dark — app dark mode, forwarded to `pickThemes()`.
 * @returns {Promise<void>}
 *
 * Gotchas:
 *   - Reads the live module `_codeTheme`, not a param, so a theme set
 *     between dispatch and render is honored.
 *   - Per-block failures are swallowed to console.warn; one bad fence
 *     never aborts the rest of the chapter.
 *   - Stashes the original source on `data-source` so a later theme
 *     switch can re-render without reloading the chapter.
 */
async function highlightIn(area, dark) {
  const { highlighter } = await getShiki();
  const codeEls = [...area.querySelectorAll('pre code')].filter(
    (el) => !isDiagramLang(el)
        && !el.closest('pre.mermaid, .svg-block, .html-block, .excalidraw-block')
        // Skip already-shiki'd code (a shiki-rendered block contains
        // <pre class="shiki"><code><span>...</span></code></pre>).
        && !el.closest('pre.shiki, pre.shiki-block')
        // Skip pres we already touched in a previous invocation.
        && !_donePres.has(el.parentElement),
  );
  for (const el of codeEls) {
    const pre = el.parentElement;
    if (!pre) continue;
    _donePres.add(pre);
    const lang = pickLang(el, highlighter);
    const code = el.textContent || '';
    const originalClasses = [...el.classList];
    try {
      const { themes, defaultColor } = pickThemes(_codeTheme, dark);
      const html = highlighter.codeToHtml(code, {
        lang,
        themes,
        defaultColor,
      });
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      const newPre = tmp.firstElementChild;
      if (newPre) {
        newPre.classList.add('shiki-block');
        // Stash the original source so a code-theme switch can re-render
        // this block without re-fetching the chapter. Without this we'd
        // need to reload the whole chapter to apply a new code theme.
        newPre.dataset.source = code;
        // Preserve the original language class on the wrapper (defensive:
        // matches pre-Phase 2 behavior and lets the e2e test count langs).
        for (const c of [...pre.classList]) newPre.classList.add(c);
        for (const c of originalClasses) {
          if (c.startsWith('language-') && !newPre.classList.contains(c)) newPre.classList.add(c);
        }
        pre.replaceWith(newPre);
        // Wrap the shiki-rendered pre with a copy button (ROADMAP v1.1 #1).
        // attachCopyButtons() deliberately skips .shiki-block so the
        // shiki path is the only place that gets the wrapper.
        wrapWithCopyButton(newPre);
      }
    } catch (e) {
      console.warn('[shiki]', e?.message || e);
    }
  }
}

// Restore a shiki-rendered block back to its original `<pre><code>…</code></pre>`
// shape so `highlightIn` can re-render it. No-op if the block wasn't carrying
// a `data-source` (defensive — we only stash it on blocks we own).
//
// @param {Element} area — chapter root whose shiki blocks to revert.
// The paired step before a re-highlight: it also drops each reverted
// pre from `_donePres` so the subsequent `highlightIn` doesn't skip it.
function restoreShikiBlocks(area) {
  const rendered = area.querySelectorAll('pre.shiki, pre.shiki-block');
  for (const pre of rendered) {
    const src = pre.dataset.source;
    if (src == null) continue;
    const fresh = document.createElement('pre');
    // Pull the language-X class off the rendered pre so pickLang() can
    // resolve the grammar. Default to 'text' if the class is missing.
    const langCls = [...pre.classList].find((c) => c.startsWith('language-'));
    const code = document.createElement('code');
    if (langCls) code.className = langCls;
    else code.className = 'language-text';
    code.textContent = src;
    fresh.appendChild(code);
    // Unwrap the .code-block-wrapper that wrapWithCopyButton created so
    // the new pre is the same DOM shape the registry saw on first pass.
    const wrap = pre.closest('.code-block-wrapper');
    if (wrap && wrap.parentNode) {
      wrap.parentNode.replaceChild(fresh, wrap);
    } else if (pre.parentNode) {
      pre.parentNode.replaceChild(fresh, pre);
    }
    // Forget the old pre so _donePres doesn't short-circuit the re-render.
    _donePres.delete(pre);
  }
}

// Registry manifest entry. `defaultFor: 'code'` makes this the fallback
// for any unknown fence lang (see registry resolve()), so ```js/```ts/…
// all route here. `load()` warms the highlighter; `render()` ignores the
// per-block args and instead sweeps the whole `ctx.area` once (highlightIn
// is idempotent, so the registry dispatching us per-block is harmless).
register('shiki', {
  kind: 'fence',
  defaultFor: 'code',
  load() { return getShiki(); },
  async render(block, ctx) {
    const area = ctx.area;
    if (!area) return;
    _dark = !!ctx.dark;
    await highlightIn(area, _dark);
  },
});

// Exposed for the reader's enhance() so the test/import shape stays
// the same as Phase 1.
//
// @param {Element} area — chapter root to highlight.
// @param {boolean} dark — app dark mode.
// @returns {Promise<void>} — thin pass-through to `highlightIn`.
export async function highlightCodeBlocks(area, dark) {
  return highlightIn(area, dark);
}

// Set the active code theme. If the theme changed, re-render every shiki
// block visible right now by restoring + re-highlighting in place. No-op
// when the theme is the same as the current value (avoids pointless
// work on hot reloads / initial mount).
//
// @param {string} themeName — new code theme ('github' | 'nord').
// Safe to call in non-DOM (SSR/test) contexts: it updates the module
// state and returns early when `document` is absent.
export function setCodeTheme(themeName) {
  if (themeName === _codeTheme) return;
  _codeTheme = themeName;
  if (typeof document === 'undefined') return;
  for (const area of document.querySelectorAll('.chapter-markdown')) {
    restoreShikiBlocks(area);
    highlightIn(area, _dark).catch((e) => console.warn('[shiki retheme]', e?.message || e));
  }
}

// Force re-render every shiki block with the CURRENT code theme but the
// NEW dark/light value. Called from the theme subscribe so that switching
// light → dark (or vice versa) emits the dark/light inline color pair
// instead of leaving the previously-rendered single color in place.
// `dark` is the new app theme. No-op outside a DOM context.
export function rethemeForDarkMode(dark) {
  _dark = !!dark;
  if (typeof document === 'undefined') return;
  for (const area of document.querySelectorAll('.chapter-markdown')) {
    restoreShikiBlocks(area);
    highlightIn(area, _dark).catch((e) => console.warn('[shiki retheme]', e?.message || e));
  }
}

// Initialize from the data-attribute set by the prefs store. This is
// what lets a fresh page load pick up the persisted value before the
// first render runs. Runs once at module load (call below). No-op when
// there's no `document` (SSR/test import).
function readInitialCodeTheme() {
  if (typeof document === 'undefined') return;
  const v = document.documentElement.getAttribute('data-code-theme');
  if (v) _codeTheme = v;
}
readInitialCodeTheme();
