// Opening folders, navigation, sibling lookup, and the live-reload watcher.
import { get } from 'svelte/store';
import { TAURI, invoke, listen } from '../tauri.js';
import { buildIndexFromRecords } from '../index.js';
import { loadProgress } from './progress.js';
import {
  folderName, folderRoot, folderMeta, groupMeta, ready, route, editing,
} from './state.js';
import { navOpen } from './prefs.js';
// ROADMAP v1.1 #2 — cross-chapter search. Rebuild the MiniSearch index
// whenever the open book changes (initial open + Tauri live-reload
// watcher). The panel reads via `runSearch`; this module is the only
// place that touches the indexer.
import { buildSearchIndex } from '../cross-search.js';

// ── Opening folders ──
export async function openScanResult(scan) {
  folderRoot.set(scan.root || scan.folder_name);
  const idx = buildIndexFromRecords(scan.folder_name, scan.files);
  folderName.set(idx.folderName);
  folderMeta.set(idx.folderMeta);
  groupMeta.set(idx.groupMeta);
  // Rebuild the cross-chapter search index from the freshly-scanned
  // contents. The panel becomes usable as soon as the user opens it.
  buildSearchIndex(idx.folderMeta);
  await loadProgress();
  route.set({ name: 'home' });
  editing.set(false);
  navOpen.set(false);
  ready.set(true);
  if (TAURI) invoke('watch_folder', { path: get(folderRoot) }).catch((e) => console.warn('[watch]', e));
}

export async function pickFolder() {
  if (!TAURI) return;
  const scan = await invoke('pick_folder');
  if (scan) await openScanResult(scan);
}
export async function openFolderPath(path) {
  if (!TAURI) return false;
  const scan = await invoke('open_folder_path', { path });
  if (scan) { await openScanResult(scan); return true; }
  return false;
}
export async function openLast() {
  if (!TAURI) return false;
  try {
    const scan = await invoke('open_last');
    if (scan) { await openScanResult(scan); return true; }
  } catch (e) { console.warn('[open_last]', e); }
  return false;
}
export async function getRecents() {
  if (!TAURI) return [];
  try { return (await invoke('get_recents')) || []; } catch { return []; }
}
export async function removeRecent(path) {
  if (TAURI) { try { await invoke('remove_recent', { path }); } catch {} }
}

// ── Navigation helpers ──
export function goHome() { editing.set(false); route.set({ name: 'home' }); }
/** Close the current folder and return to the Home (Open Folder) screen. */
export function closeFolder() {
  editing.set(false);
  navOpen.set(false);
  route.set({ name: 'home' });
  ready.set(false);
}
export function goGroup(group) { editing.set(false); route.set({ name: 'group', group }); }
export function goChapter(path) { editing.set(false); route.set({ name: 'chapter', path }); navOpen.set(false); }

export function findItem(path) {
  return get(folderMeta).find((f) => f.path === path);
}

/** Siblings within the current chapter's group (for prev/next). */
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
export async function setupWatcherListener() {
  if (!TAURI) return;
  await listen('library-changed', (e) => {
    const scan = e.payload;
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
