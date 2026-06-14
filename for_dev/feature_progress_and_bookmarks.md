# Reading progress + bookmarks

## Vision & DoD (5W1H)

**What.** The app remembers, per (folder, chapter), how far the user has scrolled (a fraction 0–1) and whether the chapter is bookmarked. This state persists across restarts. The Library's "Continue Reading" section uses it; the Reader uses it to restore the scroll position when re-opening a chapter.

**Why.** Reopening a book and finding yourself at "where I left off" is a basic expectation of a reader. The user shouldn't have to scroll back to where they were.

**Who.** Any user with a multi-chapter book. The state is per-folder, so each book has its own progress.

**When.** Every scroll event (debounced ~500 ms) in the Reader; on every bookmark toggle. The state is also written on app close (so the last scroll is preserved even if the debounce didn't fire).

**Where.** `src/lib/stores/progress.js` (JS) + `<app_data_dir>/state.json` (Rust persisted). The `progress` field in `state.json` is `Map<folderRoot, Map<relPath, { scroll, bookmarked }>>`.

**How (acceptance / DoD).**
- Scroll position is saved per (folder, chapter).
- Reopening a chapter restores the last scroll position.
- Bookmarking a chapter persists across restarts.
- The Library's "Continue Reading" lists the 3 most recently touched chapters.
- Closing the app without scrolling for 5 seconds (the debounce) still preserves the last scroll on the next launch (because the last save was within those 5 seconds, in normal use).
- A chapter that's been read to the bottom (scroll ≥ 0.95) shows a checkmark in the sidebar.

---

## How we implemented it

**What.** A Svelte writable `progressMap` keyed by chapter path. The Reader's scroll handler updates the map (debounced) and persists to Rust. Rust's `save_progress` command writes to `state.json`.

**Why this shape.** The progress map is small (one entry per chapter) and we don't need reactive queries. A single writable + debounced save is the simplest model. The Rust side is the persistence boundary (avoids direct localStorage).

**When.** Every scroll event in the Reader (debounced ~500 ms). On bookmark toggle (immediate). On app close (flush).

**Where.**
- `src/lib/stores/progress.js` — `progressMap` writable + `saveProgress` action.
- `src/lib/tauri.js` — `saveProgress` wrapper.
- `src-tauri/src/main.rs` — `save_progress` Tauri command.

**How (tech).**
- **Scroll → save**: `Reader.svelte` has a scroll handler that reads `window.scrollY / (document.body.scrollHeight - window.innerHeight)` as the fraction, and calls `saveProgress(folder, path, scroll, bookmarked)`.
- **Debounce**: 500 ms per file. The v1.0 bug was a *shared* timer; fixed in v1.1.
- **Bookmark toggle**: a star icon in the toolbar. Click → `saveProgress(folder, path, scroll, true)`.
- **Restore on mount**: when the Reader mounts a chapter, it reads `$progressMap[path]` and `window.scrollTo(0, savedScroll * scrollHeight)`.
- **Library "Continue Reading"**: filters `folderMeta` by `progressMap[f.diskPath || f.path]?.scroll > 0` and `scroll < 1`; sorts by map insertion order (most recent first); takes the top 3.
- **Sidebar checkmark**: a `f.diskPath in progressMap && progressMap[f.diskPath].scroll >= 0.95` check adds a `.tree-read` class.

**Gotchas.**
- A scroll fraction of 0 is ambiguous: "user opened the chapter and scrolled back to the top" vs "user hasn't scrolled." We treat 0 as "no progress" and 0.05+ as "started."
- A chapter that's been *deleted* from the folder still has a progress entry; we filter these at read time.
- Per-file debounce is important: a shared timer (v1.0) caused the "navigated to chapter B, save targets A's path" data-loss bug.
