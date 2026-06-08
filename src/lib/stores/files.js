// Writing + renaming markdown files, with optimistic in-memory index updates.
import { get } from 'svelte/store';
import { TAURI, invoke } from '../tauri.js';
import { folderRoot, folderMeta, groupMeta, progressMap } from './state.js';

export async function saveFile(item, content) {
  if (!TAURI || !item) throw new Error('cannot save');
  await invoke('write_file', { folder: get(folderRoot), relPath: item.diskPath, content });
  // Reflect new content in the in-memory index.
  folderMeta.update((arr) => arr.map((f) => (f.path === item.path ? { ...f, content } : f)));
}

/**
 * Rename a file on disk and update the in-memory index optimistically so the
 * UI reflects it immediately (the watcher re-scan reconciles afterwards).
 * Returns the new group-stripped `path` so the caller can navigate to it.
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
  // Migrate any saved progress/bookmark to the new key.
  progressMap.update((m) => {
    if (!m[oldDisk]) return m;
    const { [oldDisk]: moved, ...rest } = m;
    return { ...rest, [newRel]: moved };
  });
  return { newPath, newRel };
}
