// Shared reactive state. Kept dependency-free (no sibling imports) so the
// store submodules can all import it without circular-dependency issues.
import { writable } from 'svelte/store';

// ── Library ──
export const folderName = writable('');
export const folderRoot = writable('');
export const folderMeta = writable([]);
export const groupMeta = writable({});
export const ready = writable(false); // a folder is open

// route: { name: 'home' } | { name: 'group', group } | { name: 'chapter', path }
export const route = writable({ name: 'home' });
export const editing = writable(false);

// progress: diskPath -> { scroll, bookmarked }
export const progressMap = writable({});
