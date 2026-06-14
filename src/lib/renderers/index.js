// Core renderer barrel — the single import that bootstraps the registry.
//
// Single responsibility: aggregate the side-effect imports of every core
// renderer so that importing this one file populates `registry.js`'s map.
// Each renderer calls `register(lang, def)` at its own module top level, so
// the act of importing them here IS the registration.
//
// WHY a separate barrel (not registration inside registry.js): ESM hoists
// imports, so a side-effect import inside registry.js would run before that
// file's `const registry` initializes, hitting a TDZ on the first
// `register()` call. Keeping the imports here defers them until consumers
// pull the registry in. See the "Bootstrap" note at the foot of registry.js.
//
// Consumers that need the full core set (reader, slides, PDF JS-side
// helpers) should import this once. Consumers that only need a subset
// (e.g. a future plugin) can import individual files instead.
//
// Import order = registration order. It's only load-bearing for collisions
// (a later register() of the same lang wins), which the core set avoids;
// keep it stable regardless. Adding a core renderer: add its file here AND a
// row in manifest.json (the Rust PDF side reads the manifest, not this file).
import './svg.js';
import './html.js';
import './mermaid.js';
import './excalidraw.js';
import './math.js';
import './shiki.js';
import './csv.js';
