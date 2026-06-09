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

register('excalidraw', {
  kind: 'fence',
  async render(block, ctx) {
    const { pre, body, index } = block;
    if (!pre) return;
    const src = body.trim();
    pre.classList.add('excalidraw-block');
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
      console.warn('[Excalidraw mount]', e?.message || e);
      pre.textContent = src;
    }
  },
});
