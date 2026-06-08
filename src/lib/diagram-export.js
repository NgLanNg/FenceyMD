// Copy / download a rendered diagram (mermaid or inline SVG) as a PNG image.
// Native paths go through Tauri because the WKWebView supports neither image
// clipboard writes nor `<a download>`.
import { TAURI, saveBytes, copyImageBytes } from './tauri.js';

function flashBtn(btn, text) {
  btn.textContent = text;
  setTimeout(() => { btn.textContent = btn.dataset.label; }, 1400);
}

/** Rasterize an <svg> element to a white-background PNG Blob.
 *  Scales so the longest side targets ~2400px (3×–6×) for crisp text/strokes. */
export function svgToPngBlob(svgEl, scale) {
  return new Promise((resolve, reject) => {
    try {
      const rect = svgEl.getBoundingClientRect();
      let w = rect.width, h = rect.height;
      if (!w || !h) {
        const vb = svgEl.viewBox && svgEl.viewBox.baseVal;
        if (vb && vb.width) { w = vb.width; h = vb.height; }
      }
      w = Math.max(1, Math.ceil(w));
      h = Math.max(1, Math.ceil(h));
      if (!scale) scale = Math.min(6, Math.max(3, 2400 / Math.max(w, h)));

      const clone = svgEl.cloneNode(true);
      clone.setAttribute('width', w);
      clone.setAttribute('height', h);
      clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      const src = 'data:image/svg+xml;charset=utf-8,' +
        encodeURIComponent(new XMLSerializer().serializeToString(clone));

      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = w * scale;
        canvas.height = h * scale;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.setTransform(scale, 0, 0, scale, 0, 0);
        ctx.drawImage(img, 0, 0);
        canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('toBlob failed'))), 'image/png');
      };
      img.onerror = () => reject(new Error('svg image load failed'));
      img.src = src;
    } catch (e) {
      reject(e);
    }
  });
}

async function copyDiagram(svgEl) {
  const blob = await svgToPngBlob(svgEl);
  if (TAURI) {
    await copyImageBytes(new Uint8Array(await blob.arrayBuffer()));
  } else {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
  }
}

async function downloadDiagram(svgEl, fileName) {
  const blob = await svgToPngBlob(svgEl);
  if (TAURI) {
    await saveBytes(fileName, new Uint8Array(await blob.arrayBuffer()));
  } else {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = fileName; a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

/**
 * Add Copy / PNG (and optional light-dark toggle) tools to a rendered diagram.
 * The SVG is resolved from the container at click time, so the tools keep
 * working after a diagram is re-rendered (e.g. the per-diagram theme toggle).
 * Idempotent. `opts.onToggleTheme` (if given) adds a sun/moon button;
 * `opts.dark` sets which glyph to show.
 */
export function addDiagramTools(container, baseName, opts = {}) {
  container.querySelector('.diagram-tools')?.remove(); // rebuild (state may have changed)
  container.classList.add('diagram-wrap');

  const tools = document.createElement('div');
  tools.className = 'diagram-tools';
  const svg = () => container.querySelector('svg');

  const mk = (label, title, run, { html = false } = {}) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'diagram-tool-btn';
    if (html) b.innerHTML = label; else b.textContent = label;
    b.dataset.label = label;
    b.dataset.html = html ? '1' : '';
    b.title = title;
    b.addEventListener('click', async () => {
      try { await run(); if (!html) flashBtn(b, '✓'); }
      catch (e) { console.warn('[diagram]', e); if (!html) flashBtn(b, 'Failed'); }
    });
    return b;
  };

  if (opts.onToggleTheme) {
    // Sun when dark (→ switch to light), moon when light (→ switch to dark).
    const sun = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>';
    const moon = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>';
    tools.appendChild(mk(opts.dark ? sun : moon,
      'Toggle this diagram light/dark', () => opts.onToggleTheme(), { html: true }));
  }
  tools.appendChild(mk('Copy', 'Copy diagram as image', () => copyDiagram(svg())));
  tools.appendChild(mk('PNG', 'Download diagram as PNG', () => downloadDiagram(svg(), baseName + '.png')));
  container.appendChild(tools);
}
