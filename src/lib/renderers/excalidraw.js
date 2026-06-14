// Excalidraw fence renderer — mounts a Svelte `ExcalidrawViewer` into
// each fenced block. React + @excalidraw/excalidraw are dynamic-imported
// by the viewer itself, so the initial bundle stays small until the
// user actually hits one of these blocks.
//
// We also stash the original JSON on the wrapper as `data-excalidraw-json`
// so the PDF export can convert it to a static SVG without re-importing
// the source from the file.
import { register } from '../registry.js';
import { mount as svelteMount } from 'svelte';
import ExcalidrawViewer from '../../components/ExcalidrawViewer.svelte';

// Renderer def for ```excalidraw fences. `render` mounts an interactive
// `ExcalidrawViewer` into `block.pre`. `ctx.dark` themes the viewer and
// `ctx.meta.relPath` lets it save edits back to the source chapter. Note:
// this is the live-reader path — the PDF pipeline instead reads the stashed
// `data-excalidraw-json` to produce a static SVG (no Svelte/React mount).
register('excalidraw', {
  kind: 'fence',
  async render(block, ctx) {
    const { pre, body, index } = block;
    if (!pre) return;
    const src = body.trim();
    pre.classList.add('excalidraw-block');
    // Stash the raw JSON on the element so the PDF export can rasterize it
    // without re-reading the source fence from the file.
    pre.dataset.excalidrawJson = src;
    pre.textContent = ''; // clear source; viewer takes over
    try {
      svelteMount(ExcalidrawViewer, {
        target: pre,
        props: {
          json: src,
          dark: !!ctx.dark,
          relPath: ctx.meta?.relPath || '',
          label: `#${index + 1}`,
          blockIndex: index,
        },
      });
    } catch (e) {
      // Mount failed (e.g. the dynamic React/excalidraw import couldn't
      // resolve offline): restore the raw JSON as text so the drawing's
      // source isn't lost, rather than leaving an empty block.
      console.warn('[Excalidraw mount]', e?.message || e);
      pre.textContent = src;
    }
  },
});
