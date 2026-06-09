// Markdown rendering + post-processing. showdown is loaded eagerly (it's the
// core renderer); highlight.js, mermaid, excalidraw, katex, and shiki are
// heavy, so they're code-split and loaded on demand the first time a code
// block / diagram / math actually appears.
import showdown from 'showdown';
import { mount as svelteMount } from 'svelte';
import { addDiagramTools } from './diagram-export.js';
import ExcalidrawViewer from '../components/ExcalidrawViewer.svelte';

let _hljs = null;
async function getHljs() {
  if (!_hljs) _hljs = (await import('highlight.js')).default;
  return _hljs;
}

let _mermaid = null;
async function getMermaid() {
  if (!_mermaid) _mermaid = (await import('mermaid')).default;
  return _mermaid;
}

let _katex = null;
// Returns the katex module. We use the bundled CSS to render math (loaded
// once in main.js), and the module's `render` to produce HTML for each
// `$…$` / `$$…$$` match in the rendered chapter.
async function getKatex() {
  if (!_katex) _katex = (await import('katex')).default;
  return _katex;
}

let _shiki = null;
let _shikiReady = null;
// Returns a singleton shiki highlighter preloaded with the language bundle
// and the light/dark theme pair we use for dual-theme code blocks. Shiki is
// expensive to initialize (grammar compilation), so we cache the promise.
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

// Render `$…$` (inline) and `$$…$$` (block) math in `area`. We walk text
// nodes and replace matches with katex-rendered HTML. Block math is split
// onto its own lines; inline math can sit inside any text node (paragraph,
// heading, list item, table cell). We do NOT touch anything inside <pre>,
// <code>, <script>, <style>, or any element with a `katex` ancestor — that
// keeps `$5.00` and code samples safe.
function renderMathInArea(area, katex) {
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
  for (const textNode of targets) {
    const src = textNode.nodeValue;
    const frag = fragmentFromMath(src, katex);
    if (frag) textNode.parentNode.replaceChild(frag, textNode);
  }
}

// Parse a string for `$…$` (inline) and `$$…$$` (block) and return a
// DocumentFragment of plain text nodes + katex HTML span elements. Returns
// null when the input has no math so the caller can skip the DOM swap.
function fragmentFromMath(src, katex) {
  if (!/\$/.test(src)) return null;
  // Block math first ($$…$$), then inline ($…$). We match non-greedily and
  // require the opening/closing to be at the start/end of a "word" so that
  // `$5.00` and `price:$5` don't get eaten.
  const BLOCK = /\$\$([\s\S]+?)\$\$/g;
  const INLINE = /(?<!\\)\$(?!\s)([^\n$]+?)(?<!\\)\$(?!\d)/g;
  let didReplace = false;
  const frag = document.createDocumentFragment();
  let lastIndex = 0;
  // Collect all matches (block + inline) with their ranges.
  const matches = [];
  let m;
  BLOCK.lastIndex = 0;
  while ((m = BLOCK.exec(src))) matches.push({ start: m.index, end: m.index + m[0].length, tex: m[1], display: true });
  INLINE.lastIndex = 0;
  while ((m = INLINE.exec(src))) {
    // Skip if this range overlaps a block match.
    const overlaps = matches.some((b) => m.index >= b.start && m.index < b.end);
    if (overlaps) continue;
    matches.push({ start: m.index, end: m.index + m[0].length, tex: m[1], display: false });
  }
  matches.sort((a, b) => a.start - b.start);
  for (const match of matches) {
    if (match.start > lastIndex) {
      frag.appendChild(document.createTextNode(src.slice(lastIndex, match.start)));
    }
    const span = document.createElement('span');
    span.className = 'math-render';
    try {
      katex.render(match.tex, span, {
        displayMode: match.display,
        throwOnError: false,
        // Keep math theme-neutral: it inherits the surrounding text color.
        // We pass `color` is not set, so katex uses its default `katex`
        // class colors — we override those in app.css for both themes.
      });
    } catch (e) {
      // On a parse failure, leave the original source text so the user can
      // see and fix it.
      span.replaceWith(document.createTextNode(src.slice(match.start, match.end)));
      didReplace = true;
      lastIndex = match.end;
      continue;
    }
    frag.appendChild(span);
    didReplace = true;
    lastIndex = match.end;
  }
  if (!didReplace) return null;
  if (lastIndex < src.length) frag.appendChild(document.createTextNode(src.slice(lastIndex)));
  return frag;
}

const isDark = () =>
  typeof document !== 'undefined' && document.documentElement.getAttribute('data-theme') === 'dark';

// (Re)initialize mermaid for a light or dark palette so diagram text and edges
// stay legible on either background. htmlLabels:false → SVG <text> (also
// rasterizes cleanly for copy/PNG). 'base' + explicit vars guarantees every
// node/cluster/note is a dark fill with light text — mermaid's stock 'dark'
// theme leaves some boxes light.
function applyMermaidTheme(m, dark) {
  const cfg = {
    startOnLoad: false,
    securityLevel: 'loose',
    htmlLabels: false,
    flowchart: { htmlLabels: false },
  };
  if (dark) {
    cfg.theme = 'base';
    cfg.themeVariables = {
      darkMode: true,
      background: '#2a2a2e',
      primaryColor: '#33333a', primaryTextColor: '#ededeb', primaryBorderColor: '#9a9aa0',
      secondaryColor: '#3a3a42', secondaryTextColor: '#ededeb', secondaryBorderColor: '#9a9aa0',
      tertiaryColor: '#2f2f36', tertiaryTextColor: '#ededeb', tertiaryBorderColor: '#9a9aa0',
      mainBkg: '#33333a', lineColor: '#b6b6b4', textColor: '#ededeb', titleColor: '#ededeb',
      nodeTextColor: '#ededeb',
      noteBkgColor: '#3b3b41', noteTextColor: '#ededeb', noteBorderColor: '#9a9aa0',
      clusterBkg: '#2a2a2e', clusterBorder: '#54545c', edgeLabelBackground: '#2a2a2e',
    };
  } else {
    cfg.theme = 'default';
  }
  m.initialize(cfg);
}

// Re-render a single diagram with the opposite light/dark palette (per-diagram
// override, independent of the app theme).
let _mmdToggleId = 0;
async function toggleMermaidTheme(pre, name) {
  const m = await getMermaid();
  const nextDark = pre.dataset.mmdDark !== '1';
  applyMermaidTheme(m, nextDark);
  const { svg } = await m.render('mmd-toggle-' + (_mmdToggleId++), pre.dataset.mmdSource || '');
  pre.innerHTML = svg;
  pre.dataset.mmdDark = nextDark ? '1' : '0';
  // Make the panel follow the diagram's own theme so a toggled diagram doesn't
  // float on a mismatched background.
  pre.style.background = nextDark ? '#2a2a2e' : '#ffffff';
  addDiagramTools(pre, name, { dark: nextDark, onToggleTheme: () => toggleMermaidTheme(pre, name) });
}

const converter = new showdown.Converter({
  tables: true,
  tasklists: true,
  strikethrough: true,
  simplifiedAutoLink: true,
  openLinksInNewWindow: true,
  ghCodeBlocks: true,
});

export function renderMarkdown(text) {
  return converter.makeHtml(text);
}

/**
 * Enhance freshly-rendered markdown inside `area`: syntax highlighting, inline
 * SVG blocks, copy buttons, diagram image tools, and mermaid diagrams.
 *
 * `meta` (optional) is forwarded to diagram components so they can save back
 * to the source file. Currently used by the Excalidraw editor to know which
 * `.md` file to update on save.
 */
export function enhance(area, meta = {}) {
  if (!area) return;

  // Inline SVG blocks — parse the fence source (a complete <svg>…</svg>
  // document) and re-wrap the inner elements in a proper <svg> so they
  // render in the SVG namespace regardless of where innerHTML inserts them.
  // Malformed SVG yields a `<parsererror>` document; in that case fall
  // back to the raw source as text so the user can see and fix it
  // instead of staring at the parser's error markup.
  area.querySelectorAll('pre code.language-svg').forEach((block) => {
    const pre = block.parentElement;
    const src = block.textContent.trim();
    const wrap = document.createElement('div');
    wrap.className = 'svg-block';
    const parser = new DOMParser();
    const parsed = parser.parseFromString(src, 'image/svg+xml');
    if (parsed.querySelector('parsererror')) {
      wrap.classList.add('svg-block-error');
      wrap.textContent = src;
    } else {
      const srcSvg = parsed.documentElement;
      const inner = srcSvg.innerHTML;
      const svgEl = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svgEl.setAttribute('viewBox', srcSvg.getAttribute('viewBox') || '0 0 200 80');
      svgEl.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      svgEl.innerHTML = inner;
      wrap.appendChild(svgEl);
    }
    pre.replaceWith(wrap);
  });

  // Inline HTML blocks — render the markup as live DOM. This is a local
  // reader, the markdown is the user's own, and live HTML is the whole
  // point of a ```html fence (embeds, custom components, demos).
  area.querySelectorAll('pre code.language-html').forEach((block) => {
    const pre = block.parentElement;
    const wrap = document.createElement('div');
    wrap.className = 'html-block';
    wrap.innerHTML = block.textContent;
    pre.replaceWith(wrap);
  });

  // Tag mermaid source blocks up-front so the text-copy pass below skips them.
  // Stash the source so a diagram can be re-rendered later (per-diagram theme toggle).
  area.querySelectorAll('pre code.language-mermaid').forEach((block) => {
    const pre = block.parentElement;
    pre.classList.add('mermaid');
    pre.dataset.mmdSource = block.textContent;
    pre.textContent = block.textContent;
  });

  // Syntax highlight ordinary code blocks with shiki (lazy-loaded; dual-theme
  // so the same code block re-themes with the rest of the app). Skip
  // diagram languages — they have their own renderer. Also skip
  // language-html — the block is rendered as live HTML above, not as code.
  const codeEls = [...area.querySelectorAll('pre code')].filter(
    (el) => !el.classList.contains('language-mermaid')
         && !el.classList.contains('language-excalidraw')
         && !el.classList.contains('language-svg')
         && !el.classList.contains('language-html')
  );
  if (codeEls.length) {
    const dark = isDark();
    getShiki()
      .then(({ highlighter }) => {
        codeEls.forEach((el) => {
          const pre = el.parentElement;
          const cls = [...el.classList].find((c) => c.startsWith('language-'));
          const lang = cls ? cls.slice('language-'.length) : 'text';
          const code = el.textContent || '';
          // `shiki` throws if the language isn't loaded. We registered a
          // broad default set in getShiki(); unknown langs fall back to `text`.
          let resolved = highlighter.getLoadedLanguages().includes(lang) ? lang : 'text';
          try {
            const html = highlighter.codeToHtml(code, {
              lang: resolved,
              themes: { light: 'github-light', dark: 'github-dark' },
              defaultColor: dark ? 'dark' : 'light',
            });
            // shiki returns a `<pre><code>…</code></pre>`; we replace the
            // inner HTML of the existing <pre> so the copy button and the
            // existing wrapper structure are preserved.
            const tmp = document.createElement('div');
            tmp.innerHTML = html;
            const newPre = tmp.firstElementChild;
            if (newPre && pre) {
              newPre.classList.add('shiki-block');
              // Preserve any classes the existing pre already had (e.g. mermaid tagging).
              if (pre.classList.contains('shiki-block') === false) {
                for (const c of [...pre.classList]) newPre.classList.add(c);
              }
              // The original language class lives on the <code>, not the <pre>;
              // copy it onto the new pre so downstream tests / styling can
              // tell shiki blocks apart by language.
              for (const c of [...el.classList]) {
                if (c.startsWith('language-') && !newPre.classList.contains(c)) {
                  newPre.classList.add(c);
                }
              }
              pre.replaceWith(newPre);
            }
          } catch (e) {
            // shiki failed for this block — leave it as plain monospace.
            console.warn('[shiki]', e?.message || e);
          }
        });
      })
      .catch((err) => console.warn('[shiki load]', err?.message || String(err)));
  }

  // Math: render `$…$` (inline) and `$$…$$` (block) via katex. Theme-neutral
  // — katex uses default colors, and we override those in app.css per theme.
  if (area.textContent && /\$/.test(area.textContent)) {
    getKatex()
      .then((katex) => { try { renderMathInArea(area, katex); } catch (e) { console.warn('[katex]', e?.message || e); } })
      .catch((err) => console.warn('[katex load]', err?.message || String(err)));
  }

  // Text "Copy" buttons for ordinary code blocks (skip diagrams — image tools instead).
  area.querySelectorAll('pre').forEach((pre) => {
    if (pre.closest('.code-block-wrapper')) return;
    if (pre.classList.contains('mermaid') || pre.classList.contains('svg-block') || pre.classList.contains('excalidraw-block')) return;
    // shiki-rendered blocks have their own copy mechanism (handled below) — skip them.
    if (pre.classList.contains('shiki-block')) return;
    const wrapper = document.createElement('div');
    wrapper.className = 'code-block-wrapper';
    pre.parentNode.insertBefore(wrapper, pre);
    wrapper.appendChild(pre);
    const btn = document.createElement('button');
    btn.className = 'copy-btn';
    btn.textContent = 'Copy';
    btn.addEventListener('click', () => {
      const code = pre.querySelector('code');
      navigator.clipboard.writeText(code ? code.textContent : pre.textContent).then(() => {
        btn.textContent = '✓';
        setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
      });
    });
    wrapper.appendChild(btn);
  });

  // Inline SVG blocks → image copy/download tools (no theme toggle — static SVG).
  area.querySelectorAll('.svg-block').forEach((pre, i) => {
    if (pre.querySelector('svg')) addDiagramTools(pre, `diagram-${i + 1}`);
  });

  // Mermaid diagrams: lazily load, render at the app theme, then attach tools
  // (Copy / PNG + a per-diagram light/dark toggle).
  const mNodes = area.querySelectorAll('.mermaid');
  if (mNodes.length) {
    getMermaid()
      .then((m) => { applyMermaidTheme(m, isDark()); return m.run({ nodes: mNodes }); })
      .then(() => {
        area.querySelectorAll('.mermaid').forEach((pre, i) => {
          if (!pre.querySelector('svg')) return;
          const name = `diagram-${i + 1}`;
          pre.dataset.mmdDark = isDark() ? '1' : '0';
          addDiagramTools(pre, name, { dark: isDark(), onToggleTheme: () => toggleMermaidTheme(pre, name) });
        });
      })
      .catch((err) => console.warn('[Mermaid]', err?.message || String(err)));
  }

  // Excalidraw scenes: mount a React-island viewer into each fenced block.
  // React + @excalidraw/excalidraw are dynamic-imported by the viewer, so the
  // initial bundle stays small until the user actually hits one of these.
  // We also stash the original JSON on the wrapper as `data-excalidraw-json`
  // so the PDF export can convert it to a static SVG without re-importing the
  // source from the file. Each card is labelled ("Excalidraw #1", "#2", …)
  // so the user can refer to a specific scene by index. `blockIndex` is the
  // 0-based index of the excalidraw block in the chapter, used by the Rust
  // save command to find the right fence (re-reads the file each save).
  const exBlocks = area.querySelectorAll('pre code.language-excalidraw');
  exBlocks.forEach((block, i) => {
    const pre = block.parentElement;
    const src = block.textContent.trim();
    pre.classList.add('excalidraw-block');
    pre.dataset.excalidrawJson = src;
    pre.textContent = ''; // clear source; viewer takes over
    try {
      svelteMount(ExcalidrawViewer, {
        target: pre,
        props: {
          json: src,
          dark: isDark(),
          relPath: meta.relPath || '',
          label: `#${i + 1}`,
          blockIndex: i,
        },
      });
    }
    catch (e) { console.warn('[Excalidraw mount]', e?.message || e); pre.textContent = src; }
  });
}
