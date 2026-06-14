// ─────────────────────────────────────────────────────────────────────────────
// Writing + renaming markdown files.
//
// SINGLE RESPONSIBILITY: mutate files on disk (via the Tauri backend) and keep
// the in-memory index (`folderMeta` / `groupMeta` in state.js) consistent with
// the change OPTIMISTICALLY — i.e. patch the stores synchronously so the UI
// updates instantly, before the file-watcher's authoritative re-scan lands.
//
// HOW IT FITS: the Editor calls `saveFile`; the rename UI calls `renameFile`.
// The watcher listener in library.js eventually re-scans and reconciles, so
// the optimistic patches here only need to be correct until that arrives — but
// they MUST be correct, because a stale patch is what the user sees in between.
//
// KEY INVARIANTS / ASSUMPTIONS A MAINTAINER MUST KNOW:
//  - Tauri-only: both functions throw if not running native (no browser write
//    path), since file writes have no localStorage equivalent.
//  - Path-traversal trust boundary lives in Rust: `write_file`/`rename_file`
//    canonicalize and reject any `relPath`/`newName` that escapes the folder.
//    Do NOT relax that by constructing paths here.
//  - The backend progress store is keyed by `diskPath` and is NOT re-keyed on
//    rename, so `renameFile` must migrate the progress entry itself (see below)
//    or the renamed file loses its scroll/bookmark on reload.
// ─────────────────────────────────────────────────────────────────────────────
import { get } from 'svelte/store';
import { TAURI, invoke } from '../tauri.js';
import { folderRoot, folderMeta, groupMeta, progressMap } from './state.js';

/**
 * Write `content` to the file backing `item`, then mirror it into the in-memory
 * index. Throws when not in Tauri or `item` is null (no browser save path).
 * Matches the index entry by routing `path` (unique within the book), so the
 * editor's view stays consistent before the watcher re-scan reconciles.
 */
export async function saveFile(item, content) {
  if (!TAURI || !item) throw new Error('cannot save');
  await invoke('write_file', { folder: get(folderRoot), relPath: item.diskPath, content });
  // Reflect new content in the in-memory index.
  folderMeta.update((arr) => arr.map((f) => (f.path === item.path ? { ...f, content } : f)));
}

/**
 * Rename a file on disk and update the in-memory index optimistically so the
 * UI reflects it immediately (the watcher re-scan reconciles afterwards).
 * Throws when not in Tauri or `item` is null. `newName` is the new file name
 * (the Rust side validates it and rejects path-escaping names).
 *
 * Returns `{ newPath, newRel }`: `newPath` is the group-stripped routing path
 * (use it to navigate to the renamed file), `newRel` the new on-disk-relative
 * path (the progress key). Both `folderMeta` and `groupMeta` are patched, and
 * the saved progress/bookmark is migrated to the new key (see below).
 */
export async function renameFile(item, newName) {
  if (!TAURI || !item) throw new Error('cannot rename');
  const newRel = await invoke('rename_file', {
    folder: get(folderRoot), relPath: item.diskPath, newName,
  });
  const parts = newRel.split('/');
  const fileName = parts[parts.length - 1];
  const grouped = parts.length > 1;
  const newPath = grouped ? parts.slice(1).join('/') : fileName;
  const oldDisk = item.diskPath;

  const patch = (f) =>
    f.diskPath === oldDisk ? { ...f, diskPath: newRel, path: newPath, name: fileName } : f;
  folderMeta.update((arr) => arr.map(patch));
  groupMeta.update((gm) => {
    const out = {};
    for (const k of Object.keys(gm)) out[k] = gm[k].map(patch);
    return out;
  });
  // Migrate any saved progress/bookmark to the new key — both in memory AND in
  // the persisted backend store. The backend store is keyed by diskPath and is
  // NOT rekeyed by rename_file, so without the explicit save below a reload
  // (get_progress) would return the orphaned old key and lose the renamed
  // file's scroll position + bookmark. (The old backend key is left behind but
  // harmless — it references a path that no longer exists.)
  progressMap.update((m) => {
    if (!m[oldDisk]) return m;
    const { [oldDisk]: moved, ...rest } = m;
    invoke('save_progress', {
      folder: get(folderRoot), path: newRel,
      scroll: moved.scroll || 0, bookmarked: !!moved.bookmarked,
    }).catch(() => {});
    return { ...rest, [newRel]: moved };
  });
  return { newPath, newRel };
}
