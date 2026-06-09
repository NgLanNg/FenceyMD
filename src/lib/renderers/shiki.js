// Shiki fence renderer — code-fence syntax highlighter. Lazy-loads shiki
// with the language + theme bundle. Dual-theme: the same code block
// re-themes when [data-theme] changes on <html> because each token
// carries both a light color and a `--shiki-dark` inline variable, and
// the `.shiki-block` rule in app.css swaps which one is used.
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

let _shiki = null;
let _shikiReady = null;

async function getShiki() {
  if (_shiki) return _shiki;
  if (!_shikiReady) {
    _shikiReady = (async () => {
      const shiki = await import('shiki');
      const highlighter = await shiki.createHighlighter({
        themes: ['github-light', 'github-dark'],
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

function pickLang(codeEl, highlighter) {
  const cls = [...codeEl.classList].find((c) => c.startsWith('language-'));
  if (!cls) return 'text';
  const lang = cls.slice('language-'.length).toLowerCase();
  return highlighter.getLoadedLanguages().includes(lang) ? lang : 'text';
}

function isDiagramLang(codeEl) {
  return codeEl.classList.contains('language-mermaid')
      || codeEl.classList.contains('language-svg')
      || codeEl.classList.contains('language-html')
      || codeEl.classList.contains('language-excalidraw');
}

const _donePres = new WeakSet();

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
    try {
      const html = highlighter.codeToHtml(code, {
        lang,
        themes: { light: 'github-light', dark: 'github-dark' },
        defaultColor: dark ? 'dark' : 'light',
      });
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      const newPre = tmp.firstElementChild;
      if (newPre) {
        newPre.classList.add('shiki-block');
        // Preserve the existing pre's classes (defensive: matches the
        // pre-Phase 2 behavior so we don't break tests that look for
        // `language-X` on the wrapper).
        for (const c of [...pre.classList]) newPre.classList.add(c);
        for (const c of [...el.classList]) {
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

register('shiki', {
  kind: 'fence',
  defaultFor: 'code',
  load() { return getShiki(); },
  async render(block, ctx) {
    const area = ctx.area;
    if (!area) return;
    await highlightIn(area, !!ctx.dark);
  },
});

// Exposed for the reader's enhance() so the test/import shape stays
// the same as Phase 1.
export async function highlightCodeBlocks(area, dark) {
  return highlightIn(area, dark);
}
