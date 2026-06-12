import './app.css';
import 'highlight.js/styles/github.css';
// Katex CSS is required for math to render. Shiki injects its own styles
// per-block (via inline styles on tokens + CSS variables for dual themes),
// so no shiki CSS import is needed.
import 'katex/dist/katex.min.css';
import App from './App.svelte';
import { mount } from 'svelte';
import { dlog } from './lib/debug-log.js';

// Capture window-level errors and unhandled promise rejections into the
// debug log so the user can see what blew up after the fact. The WKWebView
// devtools aren't visible inside the Tauri shell, so this is the only
// paper trail for a crash that doesn't reach a try/catch.
if (typeof window !== 'undefined') {
  dlog('[boot] main.js loaded');
  window.addEventListener('error', (e) => {
    dlog('[window.error]', e?.message, e?.filename ? `${e.filename}:${e.lineno}:${e.colno}` : '', e?.error?.stack || '');
  });
  window.addEventListener('unhandledrejection', (e) => {
    const reason = e?.reason;
    dlog('[window.unhandledrejection]', reason?.message || String(reason), reason?.stack || '');
  });
}

const app = mount(App, { target: document.getElementById('app') });

export default app;
