import './app.css';
import 'highlight.js/styles/github.css';
// Katex CSS is required for math to render. Shiki injects its own styles
// per-block (via inline styles on tokens + CSS variables for dual themes),
// so no shiki CSS import is needed.
import 'katex/dist/katex.min.css';
import App from './App.svelte';
import { mount } from 'svelte';

const app = mount(App, { target: document.getElementById('app') });

export default app;
