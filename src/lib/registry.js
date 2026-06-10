// Renderer registry — the single source of truth for "what does a fence mean".
//
// Today the per-language dispatch is duplicated across `markdown.js`
// (reader), `SlideViewer.svelte` (slides), and `build_print_html` (Rust
// PDF). Every new fence type means writing the same logic in three
// places — and the SVG namespace fix (2024) had to be applied twice.
// Phase 2 collapses that into one registry consumed by all three.
//
// Shape:
//   register(lang, { kind, load, render, lazy })
//     `kind`   — 'fence' | 'inline' | 'math'
//                (informational; dispatch is keyed on `lang` + `ctx.area`)
//     `load`   — optional () => Promise<void>  — warm-up the dep eagerly
//     `render` — required async (block, ctx) => void
//                `block` = { lang, body, codeEl, pre, index } — the source
//                `ctx`   = { area, isPdf, dark, meta }
//     `lazy`   — () => Promise<any>  — the heavy dep module, exposed via
//                `LAZY_LOADS.get(lang)` so the PDF side can know which
//                modules it needs to pre-bundle for offline build.
//
// A renderer's `render(block, ctx)` mutates the DOM: it replaces
// `block.pre` (or `block.codeEl`) with the rendered output. Renderers
// never return HTML — they live entirely in the DOM, which is what
// the reader/slides/PDF consumers all already consume.
//
// The `defaultFor: "code"` manifest entry is registered as a fallback
// for any unrecognized fence language — so ```js, ```ts, etc. all
// fall through to the shiki renderer (which uses lang as the grammar
// hint).
//
// Consumers:
//   - `enhance(area, meta)` (this file) — reads the live DOM and dispatches
//   - `SlideViewer.svelte` — calls `dispatch(block, ctx)` for each fence
//   - `build_print_html` (Rust) — reads the manifest via `include_str!`
//     and maps `lang` to one of: `passthrough` (svg/mermaid/excalidraw
//     become <pre> fallback), `katex`, `shiki`. Anything else becomes
//     a plain `<pre>` block.
import manifest from './renderers/manifest.json';

const registry = new Map();
const lazyLoads = new Map();
let _defaultForCode = null;

const SVG_NS = 'http://www.w3.org/2000/svg';

/**
 * Register a renderer for a fence language. Idempotent — re-registering
 * the same lang replaces the prior entry (used in tests and for hot
 * reloading of plugins later).
 */
export function register(lang, def) {
  if (!lang) throw new Error('register(lang, def): lang is required');
  if (!def || typeof def.render !== 'function') {
    throw new Error(`register(${lang}): def.render is required`);
  }
  registry.set(lang, def);
  if (typeof def.lazy === 'function') lazyLoads.set(lang, def.lazy);
  if (def.defaultFor === 'code') _defaultForCode = lang;
}

/** Introspect the active renderer set. Used by the PDF build to know
 *  which renderers are available at compile time. */
export function getRenderers() {
  return {
    langs: [...registry.keys()],
    defaultForCode: _defaultForCode,
    lazyLoads: new Map(lazyLoads),
  };
}

/** Map<lang, () => Promise<module>>. Exposed so consumers (PDF build)
 *  can pre-bundle the right modules for offline use. */
export const LAZY_LOADS = lazyLoads;

/** Returns the registered renderer for a given lang, or the shiki
 *  fallback for unknown langs, or null if no fallback is registered. */
function resolve(lang) {
  if (registry.has(lang)) return registry.get(lang);
  if (lang === 'mermaid' || lang === 'svg' || lang === 'html' || lang === 'excalidraw') {
    return null; // unknown core lang — caller should preserve the raw block
  }
  if (_defaultForCode && registry.has(_defaultForCode)) return registry.get(_defaultForCode);
  return null;
}

/**
 * Dispatch a parsed block to its renderer. The reader/slides share
 * this entry point; each renderer mutates the DOM in place.
 *
 * `block` = { lang, body, codeEl, pre, index }
 *   - `lang`   — fence info string (lowercased; `text` for empty)
 *   - `body`   — the trimmed text inside the fence
 *   - `codeEl` — the original <code> element (used for shiki to know classes)
 *   -pre     — the wrapping <pre>
 *   - `index`  — 0-based index of this block in the area (for labelling)
 *
 * `ctx` = { area, isPdf, dark, meta, diagramTools }
 *   - `area`          — the chapter root
 *   - `isPdf`         — true when called from the PDF pipeline (Rust builds
 *                       printable HTML; renderers should produce HTML that
 *                       the PDF CSS can style, not mount Svelte components)
 *   - `dark`          — current theme
 *   - `meta`          — { relPath } for Excalidraw save-to-chapter
 *   - `diagramTools`  — (pre, name, opts) => void  injected by the reader
 *                       so renderers don't import tauri-bridge modules
 *
 * The caller is expected to be inside a try/catch — render errors
 * are surfaced to console.warn and the original pre is preserved.
 */
export async function dispatch(block, ctx) {
  const r = resolve(block.lang);
  if (!r) return false;
  try {
    if (r.load) await r.load();
    await r.render(block, ctx);
    return true;
  } catch (e) {
    console.warn(`[renderer:${block.lang}]`, e?.message || e);
    return false;
  }
}

import { stampAnchors } from './anchors.js';

// ── Block discovery ────────────────────────────────────────────────────────

/** Walk the area and dispatch every fence that has a registered
 *  renderer. The reader's `enhance()` is built on top of this. */
function collectBlocks(area) {
  const out = [];
  // Showdown emits `class="language-X"` plus an `hljs-var` class for
  // some pre-existing tokens; we want any code element whose class
  // list *contains* a `language-X` token. The previous selector
  // `class^="language-"` failed because the class attribute string
  // starts with whatever showdown puts first (e.g. `"js language-js"`).
  const codes = area.querySelectorAll('pre code');
  let i = 0;
  for (const codeEl of codes) {
    const pre = codeEl.parentElement;
    if (!pre) continue;
    const cls = [...codeEl.classList].find((c) => c.startsWith('language-'));
    if (!cls) continue;
    const lang = cls.slice('language-'.length).toLowerCase();
    out.push({ lang, body: codeEl.textContent, codeEl, pre, index: i });
    i += 1;
  }
  return out;
}

/**
 * The reader's `enhance()` — thin loop over the parsed blocks. The
 * math walker stays separate (it operates on text nodes, not fences),
 * but it too is driven from this function so the per-lang if-chain
 * in `markdown.js` disappears.
 */
export async function enhance(area, meta = {}) {
  if (!area) return;

  // 1. Walk all fences and dispatch to the registry.
  const dark = typeof document !== 'undefined'
    && document.documentElement.getAttribute('data-theme') === 'dark';
  const blocks = collectBlocks(area);
  for (const b of blocks) {
    await dispatch(b, {
      area,
      isPdf: false,
      dark,
      meta,
      diagramTools: (container, name, opts) => {
        // Lazy-imported by the reader to avoid pulling in tauri.js
        // until we actually have a diagram to export. The reader
        // also overrides this in dev/Tauri environments — the
        // default just imports the no-op browser variant.
        import('./diagram-export.js').then((m) => m.addDiagramTools(container, name, opts));
      },
    });
  }

  // 2. Math walker — handled by the math renderer. We trigger it
  //    explicitly because math lives in text nodes, not fences.
  if (area.textContent && /\$/.test(area.textContent)) {
    const mathR = resolve('math');
    if (mathR) {
      try {
        if (mathR.load) await mathR.load();
        await mathR.render({ area, lang: 'math', body: '', codeEl: null, pre: null, index: -1 }, { area, isPdf: false, dark, meta });
      } catch (e) {
        console.warn('[renderer:math]', e?.message || e);
      }
    }
  }

  // 3. Copy buttons for plain code blocks (the shiki renderer has
  //    its own copy mechanism; this catches blocks shiki didn't touch
  //    because they're inside a pre.mermaid / .svg-block / etc.).
  attachCopyButtons(area);

  // 4. Stable block anchors (ROADMAP v1.1 #23) — last step, so it
  //    sees the post-render DOM. `stampAnchors` is idempotent: re-runs
  //    on the same area are no-ops, so callers can call `enhance()`
  //    freely on every chapter navigation.
  stampAnchors(area);
}

// Wrap a `<pre>` in a `.code-block-wrapper` with a copy button. Used
// by `attachCopyButtons()` for plain fences and by the shiki renderer
// for highlighted ones. Pre must already be in the DOM.
export function wrapWithCopyButton(pre) {
  if (!pre || pre.closest('.code-block-wrapper')) return null;
  const wrapper = document.createElement('div');
  wrapper.className = 'code-block-wrapper';
  pre.parentNode.insertBefore(wrapper, pre);
  wrapper.appendChild(pre);
  const btn = document.createElement('button');
  btn.className = 'copy-btn';
  btn.textContent = 'Copy';
  btn.setAttribute('aria-label', 'Copy code to clipboard');
  btn.addEventListener('click', () => {
    const code = pre.querySelector('code');
    const text = code ? code.textContent : pre.textContent;
    if (!navigator.clipboard) return;
    navigator.clipboard.writeText(text).then(() => {
      btn.textContent = '✓';
      btn.classList.add('copied');
      setTimeout(() => { btn.textContent = 'Copy'; btn.classList.remove('copied'); }, 1500);
    }).catch(() => {
      // Older browsers / restricted contexts: still surface feedback.
      btn.textContent = '✕';
      setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
    });
  });
  wrapper.appendChild(btn);
  return wrapper;
}

function attachCopyButtons(area) {
  area.querySelectorAll('pre').forEach((pre) => {
    if (pre.closest('.code-block-wrapper')) return;
    if (pre.classList.contains('mermaid') || pre.classList.contains('svg-block') || pre.classList.contains('excalidraw-block') || pre.classList.contains('html-block') || pre.classList.contains('slide-svg-block') || pre.classList.contains('slide-html-block')) return;
    if (pre.classList.contains('shiki-block') || pre.classList.contains('shiki')) return;
    wrapWithCopyButton(pre);
  });
}

// ── Bootstrap the core registry ────────────────────────────────────────────
//
// This file does NOT import the renderers. ESM hoists all `import`
// statements, so a side-effect import here would run before the
// `const registry` on line 40 initializes — the renderers' top-level
// `register(lang, def)` calls would then hit TDZ on `registry`. The
// fix is to let consumers (reader, slides) import the renderer set
// explicitly — see `./renderers/index.js`. Adding a new core renderer
// is: drop a file in `renderers/`, add it to `renderers/index.js`,
// add a row to `renderers/manifest.json`.

export { manifest };
