// ─────────────────────────────────────────────────────────────────────────────
// Barrel for the app's reactive state + actions.
//
// SINGLE RESPONSIBILITY: present every store and action under one import path
// (`$lib/stores`) so components never reach into individual submodules. The
// logic lives in the focused submodules under ./stores/ — this file only
// re-exports.
//
// SUBMODULE MAP (the dependency order also matters — state.js is the leaf):
//  - state.js     — shared writable stores (library index, route, progressMap,
//                   search panel). Dependency-free leaf; everything else reads it.
//  - prefs.js     — persisted UI preferences (theme, fonts, layout) mirrored to
//                   localStorage and applied to the DOM.
//  - progress.js  — per-file scroll/bookmark persistence over state.progressMap.
//  - library.js   — opening folders, navigation, sibling lookup, live-reload.
//  - files.js     — writing/renaming files with optimistic index patches.
//
// GOTCHA: `export *` flattens all submodule names into one namespace, so export
// names must stay unique across submodules — a duplicate would silently shadow.
// ─────────────────────────────────────────────────────────────────────────────
export * from './stores/state.js';
export * from './stores/prefs.js';
export * from './stores/progress.js';
export * from './stores/library.js';
export * from './stores/files.js';
