// Mermaid fence renderer — lazy-loads the mermaid module, renders each
// <pre class="mermaid"> to an inline <svg>, attaches the per-diagram
// Copy/PNG/theme-toggle tools.
//
// The reader's `ctx.diagramTools` is invoked with `(pre, name, opts)`
// so the renderer doesn't need to import tauri.js directly — the
// reader wires the diagram-export module in. This keeps the renderer
// pure (no side-effectful imports of the native bridge).
import { register } from '../registry.js';

let _mermaid = null;
async function getMermaid() {
  if (!_mermaid) _mermaid = (await import('mermaid')).default;
  return _mermaid;
}

let _mmdToggleId = 0;

// (Re)initialize mermaid for a light or dark palette. htmlLabels:false
// keeps labels as SVG <text> (also rasterizes cleanly for copy/PNG).
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

async function toggleMermaidTheme(pre, name, diagramTools) {
  const m = await getMermaid();
  const nextDark = pre.dataset.mmdDark !== '1';
  applyMermaidTheme(m, nextDark);
  const { svg } = await m.render('mmd-toggle-' + (_mmdToggleId++), pre.dataset.mmdSource || '');
  pre.innerHTML = svg;
  pre.dataset.mmdDark = nextDark ? '1' : '0';
  pre.style.background = nextDark ? '#2a2a2e' : '#ffffff';
  if (diagramTools) diagramTools(pre, name, { dark: nextDark, onToggleTheme: () => toggleMermaidTheme(pre, name, diagramTools) });
}

register('mermaid', {
  kind: 'fence',
  // Lazy-load the mermaid module the first time a mermaid block is rendered.
  load() { return getMermaid(); },
  async render(block, ctx) {
    const { pre, body, index } = block;
    if (!pre) return;
    // Tag the pre so subsequent passes (shiki exclusion, copy-button skip)
    // can detect it. Stash the source so a per-diagram theme toggle can
    // re-render without re-reading the original block.
    pre.classList.add('mermaid');
    pre.dataset.mmdSource = body;
    pre.textContent = body;

    const m = await getMermaid();
    applyMermaidTheme(m, !!ctx.dark);

    // Use the index the registry provides so the diagram name is
    // unique across the chapter and survives re-renders.
    const id = `mmd-${index}-${Date.now()}`;
    const { svg } = await m.render(id, body);
    pre.innerHTML = svg;
    pre.dataset.mmdDark = ctx.dark ? '1' : '0';
    pre.style.background = ctx.dark ? '#2a2a2e' : '#ffffff';
    if (ctx.diagramTools) {
      const name = `diagram-${index + 1}`;
      ctx.diagramTools(pre, name, {
        dark: !!ctx.dark,
        onToggleTheme: () => toggleMermaidTheme(pre, name, ctx.diagramTools),
      });
    }
  },
});
