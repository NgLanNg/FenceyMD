// Markdown rendering + post-processing. showdown is loaded eagerly (it's the
// core renderer); highlight.js, mermaid, and excalidraw are heavy, so they're
// code-split and loaded on demand the first time a code block / diagram
// actually appears.
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

  // Syntax highlight ordinary code blocks (lazy hljs; applies once loaded).
  // Skip diagram languages — they have no hljs grammar. Also skip
  // language-html — the block is rendered as live HTML above, not as code.
  const codeEls = [...area.querySelectorAll('pre code')].filter(
    (el) => !el.classList.contains('language-mermaid')
         && !el.classList.contains('language-excalidraw')
         && !el.classList.contains('language-svg')
         && !el.classList.contains('language-html')
  );
  if (codeEls.length) {
    getHljs().then((hljs) => codeEls.forEach((el) => { try { hljs.highlightElement(el); } catch (_) {} }));
  }

  // Text "Copy" buttons for ordinary code blocks (skip diagrams — image tools instead).
  area.querySelectorAll('pre').forEach((pre) => {
    if (pre.closest('.code-block-wrapper')) return;
    if (pre.classList.contains('mermaid') || pre.classList.contains('svg-block') || pre.classList.contains('excalidraw-block')) return;
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
