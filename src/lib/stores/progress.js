// Per-file reading progress + bookmarks. Persisted to the Tauri backend, or to
// localStorage in browser/dev mode.
import { get, writable } from 'svelte/store';
import { TAURI, invoke } from '../tauri.js';
import { progressMap, folderRoot, folderName } from './state.js';

export function progressFor(diskPath) {
  return get(progressMap)[diskPath] || { scroll: 0, bookmarked: false };
}

// ROADMAP v1.1 #13 — live scroll progress of the currently open
// chapter, expressed as a fraction in [0, 1]. Sidebar reads this
// store to render a small progress dot in the chapter list. Reader
// writes on every scroll event. Lives in progress.js (next to
// progressMap) so the data path is obvious.
export const chapterScrollFrac = writable(0);

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

let _saveTimer = null;
export function saveProgress(diskPath, scroll, bookmarked) {
  progressMap.update((m) => ({ ...m, [diskPath]: { scroll, bookmarked } }));
  if (TAURI) {
    clearTimeout(_saveTimer);
    _saveTimer = setTimeout(() => {
      invoke('save_progress', { folder: get(folderRoot), path: diskPath, scroll, bookmarked }).catch(() => {});
    }, 400);
  } else {
    try { localStorage.setItem('md-progress::' + get(folderName), JSON.stringify(get(progressMap))); } catch {}
  }
}

export function toggleBookmark(item) {
  if (!item) return;
  const key = item.diskPath || item.path;
  const pr = progressFor(key);
  saveProgress(key, pr.scroll || 0, !pr.bookmarked);
}
