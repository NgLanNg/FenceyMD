// Side-effect imports: each renderer calls `register(lang, def)` at
// module top level. Importing this file is what populates the registry.
//
// Consumers that need the full core set (reader, slides, PDF JS-side
// helpers) should import this once. Consumers that only need a subset
// (e.g. a future plugin) can import individual files instead.
import './svg.js';
import './html.js';
import './mermaid.js';
import './excalidraw.js';
import './math.js';
import './shiki.js';
import './csv.js';
