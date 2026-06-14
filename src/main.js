/**
 * Application entry point.
 *
 * Single responsibility: bootstrap the Svelte 5 app — pull in the global
 * stylesheets the renderer pipeline depends on, install last-resort error
 * capture, and mount the root <App> into #app.
 *
 * How it fits together:
 *   - Side-effect CSS imports here are the *global* styles; per-feature
 *     styles (shiki, mermaid, etc.) are injected by their own renderers.
 *   - App.svelte owns all routing/state; this file does nothing after mount.
 *   - dlog() writes to the in-app debug log (src/lib/debug-log.js), which is
 *     the only crash trail visible inside the Tauri WKWebView shell.
 *
 * Key invariant: a DOM element with id="app" must exist in index.html before
 * this module runs — mount() targets it directly and does not guard for null.
 */
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

// The mounted app instance. Exported mainly so HMR / tooling can reach the
// component handle; nothing in the app consumes this import.
export default app;
