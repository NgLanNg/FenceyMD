// ─────────────────────────────────────────────────────────────────────────────
// Shared reactive state — the app's central writable stores.
//
// SINGLE RESPONSIBILITY: declare the bare writable stores that more than one
// store submodule needs to read or write (library index, route, progress map,
// search panel state). It holds data only — no actions, no I/O, no derivations.
//
// HOW IT FITS: this is the leaf of the stores/ dependency graph. It is kept
// DEPENDENCY-FREE (it imports nothing but svelte/store and has no sibling
// imports) precisely so progress.js / files.js / library.js can all import it
// without forming an import cycle. Mutator logic lives in those siblings; this
// file is just the data they share.
//
// KEY INVARIANTS / ASSUMPTIONS A MAINTAINER MUST KNOW:
//  - Do NOT add sibling imports here. A back-edge from this leaf would
//    reintroduce the circular-dependency hazard this split was made to avoid.
//  - `folderMeta` (flat list) and `groupMeta` (grouped map) describe the SAME
//    files in two shapes; library.js rebuilds both together so they stay in
//    sync. `progressMap` is keyed by `diskPath`, the on-disk-relative path —
//    NOT the group-stripped `path` used for routing.
// ─────────────────────────────────────────────────────────────────────────────
import { writable } from 'svelte/store';

// ── Library ──
// Display name of the open folder (the book title shown in the UI).
export const folderName = writable('');
// Absolute root path of the open folder; the key the backend uses for
// progress, recents, and the live-reload watcher.
export const folderRoot = writable('');
// Flat list of every chapter file in the open book (the search/lookup index).
export const folderMeta = writable([]);
// The same files arranged as group -> [items], used to render the sidebar.
export const groupMeta = writable({});
export const ready = writable(false); // a folder is open

// Active view. route: { name: 'home' } | { name: 'group', group } | { name: 'chapter', path }
export const route = writable({ name: 'home' });

// Reading progress for the open book. progress: diskPath -> { scroll, bookmarked }
export const progressMap = writable({});

// Cross-chapter search panel — opened with ⌘⇧F. The query is what the user
// typed (used to render result snippets). The pending-jump store lets the
// panel tell the Reader "set your in-chapter search to this string on
// mount" so the match is highlighted automatically when the user picks
// a result. `active` is true while the panel is open.
export const crossSearchOpen = writable(false);
export const crossSearchQuery = writable('');
export const pendingInChapterSearch = writable('');

// MCP / agent session context (ROADMAP integration). Set when an agent
// calls `open_file` with a `session_context` argument. Phase 2's sidebar
// chat reads this to know which agent+session to spawn. The Rust MCP
// server emits a `mcp:session-context` event on every change; the
// subscriber in App.svelte forwards it to this store.
export const mcpSessionContext = writable(null);
