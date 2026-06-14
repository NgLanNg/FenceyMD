// Mermaid fence renderer — lazy-loads the mermaid module, renders each
// <pre class="mermaid"> to an inline <svg>, attaches the per-diagram
// Copy/PNG/theme-toggle tools.
//
// The reader's `ctx.diagramTools` is invoked with `(pre, name, opts)`
// so the renderer doesn't need to import tauri.js directly — the
// reader wires the diagram-export module in. This keeps the renderer
// pure (no side-effectful imports of the native bridge).
import { register } from '../registry.js';

// Memoized lazy import of the mermaid module. The cache (`_mermaid`)
// guarantees `initialize()` is called against a single shared instance,
// which matters because that global state is what the theme helpers mutate.
let _mermaid = null;
async function getMermaid() {
  if (!_mermaid) _mermaid = (await import('mermaid')).default;
  return _mermaid;
}

// Monotonic counter feeding unique mermaid render ids. mermaid requires a
// distinct id per `render()` call; reusing one corrupts its internal SVG id
// map, so this is bumped for every toggle/relight render.
let _mmdToggleId = 0;

// (Re)initialize mermaid for a light or dark palette. htmlLabels:false
// keeps labels as SVG <text> (also rasterizes cleanly for copy/PNG).
function applyMermaidTheme(m, dark) {
  const cfg = {
    startOnLoad: false,
    // 'strict' encodes HTML in labels and disables click handlers. Diagram
    // source comes from untrusted .md files, so we don't want 'loose' (which
    // permits inline HTML / click-to-JS). We already render labels as SVG
    // <text> (htmlLabels:false), so 'strict' costs us nothing here.
    securityLevel: 'strict',
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

// Flip a single rendered diagram between light and dark, re-rendering from
// the stashed source. Reads/writes the per-diagram `data-mmd-dark` flag (the
// source of truth for this block's current palette) and re-attaches the
// diagram tools so the toggle button reflects the new state. `diagramTools`
// is the reader-injected `(pre, name, opts)` callback; absent in contexts
// (PDF, tests) that don't wire the export module — the re-render still runs.
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

// ── PDF relight helper (CODE-REVIEW P1.3) ────────────────────────────────
// PDFs are always rendered light (build_print_html forces the light
// palette — that's the right call for print, but it means a dark-mode
// reader would ship diagrams drawn in the dark theme over a white PDF
// background, producing an unreadable dark-on-white mess). The fix:
// when the reader is in dark mode, re-render every `.mermaid` to the
// light theme in a CLONE of the chapter (so the live DOM doesn't
// flash), then use the clone's HTML for the PDF payload.
//
// mermaid's `initialize()` is a global mutation — there's no
// per-render "use this theme" call. The "race" risk would be: while
// the relight is in flight, a chapter navigation triggers an
// `enhance()` that calls `applyMermaidTheme(m, dark)`. In practice
// the relight holds the export's `await` chain end-to-end, so any
// in-flight render completes before another starts. After the
// relight, the NEXT in-app render will call `applyMermaidTheme(m,
// ctx.dark)` itself, restoring the correct palette. We touch the
// clone, not the live DOM, so the user never sees a flash.
//
// The deeper fix — threading theme through `ctx` per-render and
// abandoning mermaid's `initialize()`-as-global-state — is the P2
// item in CODE-REVIEW §7 ("renderer theme state lives in
// module-level singletons"). Worth doing, but it's a refactor, not
// a one-line patch. Skipped here.
/**
 * Re-render every `.mermaid` block inside a detached chapter clone to the
 * light palette, in place, for the PDF payload.
 *
 * @param {Element|null} cloneRoot - A clone of the chapter DOM (never the
 *   live tree — see the WHY block above). No-op when null or contains no
 *   mermaid blocks.
 * @returns {Promise<void>} Resolves once all blocks are relit.
 *   Per-block failures are caught and logged; the original SVG is left
 *   intact so the diagram is never dropped from the PDF.
 */
export async function relightMermaidForPdf(cloneRoot) {
  if (!cloneRoot) return;
  const blocks = [...cloneRoot.querySelectorAll('.mermaid')];
  if (!blocks.length) return;
  const m = await getMermaid();
  // The relight always targets the light theme, regardless of what
  // the in-app theme was at this moment. applyMermaidTheme() mutates
  // mermaid's global state, but since the relight is on a clone
  // (not attached to the document) and the live blocks still carry
  // their original data-mmd-dark flag, the next in-app render will
  // re-initialize to the correct theme.
  applyMermaidTheme(m, false);
  for (const pre of blocks) {
    const source = pre.dataset.mmdSource;
    if (!source) continue;
    try {
      const { svg } = await m.render('mmd-pdf-' + (_mmdToggleId++), source);
      pre.innerHTML = svg;
      pre.dataset.mmdDark = '0';
      pre.style.background = '#ffffff';
    } catch (e) {
      // If the relight fails, leave the original SVG in place — the
      // PDF will still render something (mermaid's own dark output on
      // a white background) rather than dropping the diagram.
      // eslint-disable-next-line no-console
      console.warn('[mermaid relight]', e?.message || e);
    }
  }
}

// Renderer def for ```mermaid fences. `render` rasterizes the diagram to an
// inline <svg> in `block.pre`, theming it to `ctx.dark` and wiring the
// per-diagram export tools via `ctx.diagramTools`. See registry.js for the
// `block`/`ctx` shape. Note: this is the live-reader path; the PDF path uses
// relightMermaidForPdf() instead, since build_print_html forces light.
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
