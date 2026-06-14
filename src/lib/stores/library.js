// Library store: the single owner of "which book is open and where are we in
// it." Responsible for opening folders (native picker, by-path, last-opened),
// route navigation between home/group/chapter, prev/next sibling lookup, and
// the Tauri live-reload watcher that keeps the in-memory index in sync with
// on-disk changes.
//
// How it fits together:
//   - All scan results (from Rust commands `pick_folder`, `open_folder_path`,
//     `open_last`, or the `library-changed` event) funnel through
//     `openScanResult`, which is the one place that builds the chapter index
//     and seeds the cross-chapter search index. Keep that funnel intact so the
//     two indexes never diverge.
//   - Reactive state lives in ./state.js (dependency-free by design); this
//     module is a pure writer of those stores and never holds local copies.
//   - index.js turns raw `{ path, name, content }` records into the
//     { folderName, folderMeta, groupMeta } shape consumed app-wide.
//
// Invariants a maintainer must know:
//   - Every Tauri-only path bails early when !TAURI (browser/dev preview) so
//     the UI degrades gracefully instead of throwing on missing `invoke`.
//   - `folderMeta[].path` is the relative (group-stripped) path used for
//     routing; `diskPath` is the absolute on-disk key. Navigation and sibling
//     lookups key off `path`, never `diskPath`.
import { get } from 'svelte/store';
import { TAURI, invoke, listen, mcpSetActiveFolder, mcpClearActiveFolder } from '../tauri.js';
import { buildIndexFromRecords } from '../index.js';
import { loadProgress } from './progress.js';
import {
  folderName, folderRoot, folderMeta, groupMeta, ready, route,
} from './state.js';
import { navOpen } from './prefs.js';
// ROADMAP v1.1 #2 — cross-chapter search. Rebuild the MiniSearch index
// whenever the open book changes (initial open + Tauri live-reload
// watcher). The panel reads via `runSearch`; this module is the only
// place that touches the indexer.
import { buildSearchIndex } from '../cross-search.js';
import { dlog, dlogStart, dlogEnd } from '../debug-log.js';

// ── Opening folders ──
/**
 * Adopt a freshly-scanned folder as the open book: build the chapter index,
 * publish it to the shared stores, seed cross-chapter search, restore reading
 * progress, then route home and mark the app ready.
 *
 * This is the single entry point all open-paths converge on (picker, by-path,
 * last-opened, and the live-reload event), so the chapter index and the search
 * index are always built from the same scan and can never drift.
 *
 * @param {{ root?: string, folder_name: string, files: Array<{path,name,content}> }} scan
 *   Scan record from a Rust command. `root` is the absolute folder path; when
 *   absent (older/edge payloads) we fall back to `folder_name`.
 * @returns {Promise<void>} resolves once progress is loaded and `ready` is set.
 * @throws re-throws after logging so callers that care (none currently outside
 *   the pickFolder/openFolderPath wrappers) can react; the wrappers swallow it.
 */
export async function openScanResult(scan) {
  const id = dlogStart('openScanResult', { root: scan?.root, fileCount: scan?.files?.length });
  const t0 = performance.now();
  try {
    folderRoot.set(scan.root || scan.folder_name);
    const idx = buildIndexFromRecords(scan.folder_name, scan.files);
    folderName.set(idx.folderName);
    folderMeta.set(idx.folderMeta);
    groupMeta.set(idx.groupMeta);
    dlog('[openScanResult] index built', { ms: Math.round(performance.now() - t0) });
    // Rebuild the cross-chapter search index from the freshly-scanned
    // contents. The panel becomes usable as soon as the user opens it.
    // Best-effort: if the search indexer throws (e.g. a folder has two
    // files with the same basename so the search IDs collide), the scan
    // itself is fine — we just lose search across the new folder until
    // the user restarts or the ID is fixed. Don't let the search error
    // take down the whole folder-open flow.
    try {
      buildSearchIndex(idx.folderMeta);
    } catch (e) {
      dlog('[openScanResult] search-index build failed (non-fatal)', e?.message || String(e));
      console.warn('[search] index build failed (non-fatal):', e?.message || e);
    }
    await loadProgress();
    dlog('[openScanResult] progress loaded');
    route.set({ name: 'home' });
    navOpen.set(false);
    ready.set(true);
    // ROADMAP integration: hand the active folder + full file list to
    // the Rust MCP server so `get_chapter_content` and `get_book_toc`
    // can answer without re-scanning. Best-effort — the app works
    // fine even if the MCP server is down.
    mcpSetActiveFolder(scan.root || scan.folder_name, idx.folderMeta);
    if (TAURI) {
      // Fire-and-forget: the watcher is a nice-to-have, so we don't await it
      // (it would delay `ready`) and we never let its failure abort the open.
      invoke('watch_folder', { path: get(folderRoot) })
        .then(() => dlog('[openScanResult] watch_folder started'))
        .catch((e) => { dlog('[openScanResult] watch_folder err', e?.message || e); console.warn('[watch]', e); });
    }
    dlogEnd(id, 'openScanResult', 'ok', { totalMs: Math.round(performance.now() - t0) });
  } catch (e) {
    dlog('[openScanResult] caught', e?.message || String(e), e?.stack);
    dlogEnd(id, 'openScanResult', 'err', { err: e?.message });
    throw e;
  }
}

/**
 * Open the native folder picker and adopt the chosen folder. A user cancel
 * resolves quietly (no throw, nothing opened). No-op outside Tauri.
 * Unlike the wrappers below it returns nothing — callers fire it from UI.
 * @returns {Promise<void>}
 */
export async function pickFolder() {
  if (!TAURI) return;
  const id = dlogStart('pickFolder');
  const t0 = performance.now();
  try {
    dlog('[pickFolder] awaiting native dialog');
    const scan = await invoke('pick_folder');
    if (!scan) {
      dlog('[pickFolder] user cancelled');
      dlogEnd(id, 'pickFolder', 'cancelled');
      return;
    }
    dlog('[pickFolder] got scan', { root: scan.root, fileCount: scan.files?.length });
    await openScanResult(scan);
    dlogEnd(id, 'pickFolder', 'ok', { totalMs: Math.round(performance.now() - t0) });
  } catch (e) {
    dlog('[pickFolder] caught', e?.message || String(e), e?.stack);
    dlogEnd(id, 'pickFolder', 'err');
  }
}
/**
 * Open a specific folder by absolute path (e.g. from the recents list).
 * @param {string} path absolute folder path.
 * @returns {Promise<boolean>} true if opened; false outside Tauri, if the path
 *   is missing/empty (scan returns null), or on error. Callers use the boolean
 *   to decide whether to prune a dead recent.
 */
export async function openFolderPath(path) {
  if (!TAURI) return false;
  const id = dlogStart('openFolderPath', { path });
  const t0 = performance.now();
  try {
    dlog('[openFolderPath] invoking open_folder_path');
    const scan = await invoke('open_folder_path', { path });
    if (!scan) {
      dlog('[openFolderPath] returned null (path missing or empty)');
      dlogEnd(id, 'openFolderPath', 'null');
      return false;
    }
    dlog('[openFolderPath] got scan', { root: scan.root, fileCount: scan.files?.length });
    await openScanResult(scan);
    dlogEnd(id, 'openFolderPath', 'ok', { totalMs: Math.round(performance.now() - t0) });
    return true;
  } catch (e) {
    dlog('[openFolderPath] caught', e?.message || String(e), e?.stack);
    dlogEnd(id, 'openFolderPath', 'err');
    return false;
  }
}
/**
 * Reopen the most recently opened folder (used on app launch).
 * @returns {Promise<boolean>} true if a last folder existed and opened; false
 *   outside Tauri, when there is no last folder, or on error.
 */
export async function openLast() {
  if (!TAURI) return false;
  const id = dlogStart('openLast');
  const t0 = performance.now();
  try {
    dlog('[openLast] invoking open_last');
    const scan = await invoke('open_last');
    if (!scan) {
      dlog('[openLast] no last folder');
      dlogEnd(id, 'openLast', 'none');
      return false;
    }
    dlog('[openLast] got scan', { root: scan.root, fileCount: scan.files?.length });
    await openScanResult(scan);
    dlogEnd(id, 'openLast', 'ok', { totalMs: Math.round(performance.now() - t0) });
    return true;
  } catch (e) {
    dlog('[openLast] caught', e?.message || String(e), e?.stack);
    dlogEnd(id, 'openLast', 'err');
    return false;
  }
}
/**
 * Fetch the recently-opened folders list.
 * @returns {Promise<Array>} the list, or [] outside Tauri / on any failure —
 *   the recents UI treats an empty list and an error identically, so failures
 *   are swallowed rather than surfaced.
 */
export async function getRecents() {
  if (!TAURI) return [];
  try { return (await invoke('get_recents')) || []; } catch { return []; }
}
/**
 * Drop a folder from the recents list (e.g. after it fails to open).
 * Best-effort: errors are swallowed since this is non-critical housekeeping.
 * @param {string} path absolute folder path to forget. No-op outside Tauri.
 * @returns {Promise<void>}
 */
export async function removeRecent(path) {
  if (TAURI) { try { await invoke('remove_recent', { path }); } catch {} }
}

// ── Navigation helpers ──
// (The reader exits edit mode on navigation via a `path`-change $effect in
// Reader.svelte — these helpers only change the route.)
/** Route to the book's table-of-contents (home) without closing the folder. */
export function goHome() { route.set({ name: 'home' }); }
/** Close the current folder and return to the Home (Open Folder) screen. */
export function closeFolder() {
  navOpen.set(false);
  route.set({ name: 'home' });
  ready.set(false);
  // ROADMAP integration: tell the MCP server the active book is gone
  // so its tools return ERR_NO_BOOK_OPEN. Fire-and-forget.
  mcpClearActiveFolder();
}
/** Route to a group's chapter listing. @param {string} group group key. */
export function goGroup(group) { route.set({ name: 'group', group }); }
/**
 * Open a chapter and collapse the nav drawer (mobile-style overlay behaviour).
 * @param {string} path the chapter's relative `path` (not `diskPath`).
 */
export function goChapter(path) { route.set({ name: 'chapter', path }); navOpen.set(false); }

/**
 * Look up a chapter record by its relative path.
 * @param {string} path relative `path` key.
 * @returns {object|undefined} the folderMeta entry, or undefined if not found.
 */
export function findItem(path) {
  return get(folderMeta).find((f) => f.path === path);
}

/**
 * Resolve a chapter's siblings for prev/next navigation. If the chapter lives
 * in a group, siblings are that group's ordered items; otherwise they are the
 * flat folderMeta list. (`list` defaults to folderMeta and is only overridden
 * once a matching group is found, so an ungrouped chapter naturally keeps it.)
 * @param {string} path the chapter's relative `path`.
 * @returns {{group: string|null, list: object[], idx: number,
 *   prev: object|null, next: object|null}} the containing group key (null if
 *   ungrouped), the sibling list, the chapter's index within it (-1 if absent,
 *   which yields prev=next=null), and the adjacent chapters.
 */
export function siblingsOf(path) {
  const gm = get(groupMeta);
  let list = get(folderMeta);
  let group = null;
  for (const g of Object.keys(gm)) {
    if (gm[g].some((i) => i.path === path)) { list = gm[g]; group = g; break; }
  }
  const idx = list.findIndex((f) => f.path === path);
  return {
    group,
    list,
    idx,
    prev: idx > 0 ? list[idx - 1] : null,
    next: idx >= 0 && idx < list.length - 1 ? list[idx + 1] : null,
  };
}

// ── Live reload (Tauri) ──
/**
 * Subscribe (once, at app start) to the Rust `library-changed` event so disk
 * edits re-index the open book live. No-op outside Tauri.
 *
 * Unlike `openScanResult` this deliberately does NOT reset the route to home,
 * load progress, or touch `ready` — it only refreshes the indexes in place so
 * the user stays on whatever chapter they were reading across a reload.
 * @returns {Promise<void>} resolves once the listener is registered.
 */
export async function setupWatcherListener() {
  if (!TAURI) return;
  await listen('library-changed', (e) => {
    const scan = e.payload;
    // Ignore stale/foreign events: the watcher may still be firing for a
    // folder the user has since closed or switched away from.
    if (!scan || scan.root !== get(folderRoot)) return;
    const idx = buildIndexFromRecords(scan.folder_name, scan.files);
    folderName.set(idx.folderName);
    folderMeta.set(idx.folderMeta);
    groupMeta.set(idx.groupMeta);
    // Re-index for cross-chapter search on every live-reload.
    buildSearchIndex(idx.folderMeta);
    // If the open chapter vanished, fall back home.
    const r = get(route);
    if (r.name === 'chapter' && !idx.folderMeta.some((f) => f.path === r.path)) {
      route.set({ name: 'home' });
    }
  });
}
