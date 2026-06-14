// ─────────────────────────────────────────────────────────────────────────────
// Per-file reading progress + bookmarks.
//
// SINGLE RESPONSIBILITY: read/write each chapter's saved scroll position and
// bookmark flag, mirroring `progressMap` (in state.js) to durable storage —
// the Tauri backend when running native, or `localStorage` in browser/dev.
//
// HOW IT FITS: the Reader writes via `saveProgress` on scroll; the sidebar and
// Reader read via `progressFor`. files.js's `renameFile` re-keys an entry here
// when a file moves. library.js calls `loadProgress` after opening a folder.
//
// KEY INVARIANTS / ASSUMPTIONS A MAINTAINER MUST KNOW:
//  - All keys are `diskPath` (the on-disk-relative path), to match the backend
//    store. Never key by the group-stripped routing `path`.
//  - `progressMap` is the source of truth in memory; durable storage is a
//    mirror. Reads always go through the store, not storage.
//  - Persistence is best-effort: backend/localStorage failures are swallowed so
//    a storage hiccup never breaks navigation. The in-memory update still wins.
// ─────────────────────────────────────────────────────────────────────────────
import { get, writable } from 'svelte/store';
import { TAURI, invoke } from '../tauri.js';
import { progressMap, folderRoot, folderName } from './state.js';

/** Snapshot of one file's saved progress, keyed by `diskPath`. Returns a fresh
 *  default `{ scroll: 0, bookmarked: false }` for files never opened, so
 *  callers can read fields without null checks. The returned object is a live
 *  reference into the store when present — treat it as read-only. */
export function progressFor(diskPath) {
  return get(progressMap)[diskPath] || { scroll: 0, bookmarked: false };
}

// ROADMAP v1.1 #13 — live scroll progress of the currently open
// chapter, expressed as a fraction in [0, 1]. Sidebar reads this
// store to render a small progress dot in the chapter list. Reader
// writes on every scroll event. Lives in progress.js (next to
// progressMap) so the data path is obvious.
export const chapterScrollFrac = writable(0);

// ROADMAP v1.1 #14 — timestamp (ms since epoch) of the last editor
// save, or 0 if the editor has never saved. The Editor.svelte
// toolbar subscribes to this for the "Saved 2s ago" indicator and
// the Reader reads it for the unsaved-changes dot in the chapter
// list. Stored in a writable (not derived) so any save path —
// autosave, manual ⌘S, or programmatic — can update it from the
// outside, not just from the Editor component itself.
export const lastSavedAt = writable(0);

// Monotonic counter bumped once per editor save. The Reader uses it to tell
// "this chapter's html just changed because WE saved it" (keep the editor open)
// from "an external file-watcher change" (close the editor to show fresh
// content). This replaces a fragile `Date.now() - lastSavedAt < 500ms` window
// that misfired when a slow save's folderMeta update landed after the window —
// closing the editor mid-edit. A sequence token has no timing assumption.
export const selfSaveSeq = writable(0);

/** Replace `progressMap` with the saved progress for the currently-open book.
 *  Call after a folder is opened (see library.js). On any read/parse failure
 *  it resets to an empty map rather than throwing — a corrupt store must not
 *  block opening the book. localStorage is namespaced per `folderName`. */
export async function loadProgress() {
  const root = get(folderRoot);
  if (TAURI) {
    try { progressMap.set((await invoke('get_progress', { folder: root })) || {}); }
    catch { progressMap.set({}); }
  } else {
    try { progressMap.set(JSON.parse(localStorage.getItem('md-progress::' + get(folderName))) || {}); }
    catch { progressMap.set({}); }
  }
}

// Debounce the disk-write PER FILE. A single shared timer was a data-loss bug:
// navigating from chapter A to chapter B within the 400ms window cleared A's
// pending save, so A's scroll position was updated in memory but never
// persisted to the backend — and was lost on reload. Keyed by diskPath, each
// file's save settles independently. Each timer deletes itself on fire, so the
// map only ever holds the handful of currently-pending writes.
const _saveTimers = new Map();
/** Update one file's progress in memory immediately, then persist it. The
 *  in-memory `progressMap` update is synchronous (UI reflects it at once); the
 *  durable write is debounced per `diskPath` (400ms, Tauri) or written through
 *  to localStorage (browser). `scroll` is the saved offset, `bookmarked` the
 *  flag — both are written verbatim, so callers must pass the full intended
 *  state, not a delta. Backend write errors are swallowed (best-effort). */
export function saveProgress(diskPath, scroll, bookmarked) {
  progressMap.update((m) => ({ ...m, [diskPath]: { scroll, bookmarked } }));
  if (TAURI) {
    clearTimeout(_saveTimers.get(diskPath));
    _saveTimers.set(diskPath, setTimeout(() => {
      _saveTimers.delete(diskPath);
      invoke('save_progress', { folder: get(folderRoot), path: diskPath, scroll, bookmarked }).catch(() => {});
    }, 400));
  } else {
    try { localStorage.setItem('md-progress::' + get(folderName), JSON.stringify(get(progressMap))); } catch {}
  }
}

/** Flip the bookmark flag for a chapter while preserving its saved scroll.
 *  No-op on a null `item`. Accepts either a folderMeta item or anything with a
 *  `diskPath`/`path`, preferring `diskPath` so the key matches the persisted
 *  store (a `path`-keyed entry would orphan the saved scroll). */
export function toggleBookmark(item) {
  if (!item) return;
  const key = item.diskPath || item.path;
  const pr = progressFor(key);
  saveProgress(key, pr.scroll || 0, !pr.bookmarked);
}
