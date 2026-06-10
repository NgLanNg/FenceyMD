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

// Cross-chapter search panel — opened with ⌘⇧F. The query is what the user
// typed (used to render result snippets). The pending-jump store lets the
// panel tell the Reader "set your in-chapter search to this string on
// mount" so the match is highlighted automatically when the user picks
// a result. `active` is true while the panel is open.
export const crossSearchOpen = writable(false);
export const crossSearchQuery = writable('');
export const pendingInChapterSearch = writable('');
