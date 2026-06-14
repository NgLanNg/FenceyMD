# Library / Home

## Vision & DoD (5W1H)

**What.** The Home view is what the user sees when a book is open but no specific chapter is selected. It shows: the book's name, a recents dropdown (last 10 folders), a "Root files" card listing every chapter at the book's root, and a "Continue Reading" section listing the last few chapters the user was partway through.

**Why.** A reader's first action is usually "where was I?" — not "take me to a specific chapter." The Home view answers that without the user having to think. The recents dropdown also doubles as "switch books" — picking a recent folder re-opens it.

**Who.** A user who has been using the app for more than one session. The Home view is the app's *idle* state.

**When.**
- At app launch (if "reopen last folder on launch" is on and a `last_folder` is in the persisted store).
- When the user clicks the "FenceyMD" brand row in the sidebar (the explicit Home button).
- When the user picks a new folder from the file picker or recents.
- When the user closes the only open chapter (no specific path to navigate to).

**Where.** The Home view lives in `src/components/Library.svelte`, mounted by `App.svelte` when the active route is `home`. It reads from the same stores the Reader uses (`folderMeta`, `progress`), so its data is always fresh.

**How (acceptance / DoD).**
- Book name appears as a large editorial-style title.
- Recents dropdown shows the last 10 folders, most-recent first; clicking one re-opens that folder.
- A "Root files" card lists every `.md` file at the root of the active folder with title + reading-time + word-count.
- A "Continue Reading" section lists the 3 most recently opened chapters that have a non-zero scroll position; clicking one jumps to that chapter at the saved scroll.
- Clicking a chapter anywhere on Home navigates to it (`goChapter(path)`), which closes the Home view and opens the Reader.

---

## How we implemented it

**What.** A Svelte 5 component (`Library.svelte`) that subscribes to the `folderMeta`, `progress`, and `route` stores and renders the three cards. No Rust involved — the Home view is pure derived UI.

**Why this shape.** The Home view has no side effects and no Rust commands. It just visualizes state. Keeping it as a Svelte component means it's instantly reactive: when `progress` updates (a chapter scroll was just saved), the "Continue Reading" list re-orders without a manual refresh.

**When.** Mounted by `App.svelte` whenever `$route.name === 'home'`. The route is set to `home` by `openScanResult` (after a folder is picked) and by `goHome` (when the user clicks the brand row).

**Where.**
- `src/components/Library.svelte` — the component itself.
- `src/lib/stores/library.js` — exports `goHome()` which sets `route = {name: 'home'}`.
- `src/lib/stores/prefs.js` — the recents list (loaded from `state.json` via `get_recents` Tauri command).

**How (tech).**
- **Svelte 5 runes**: `$derived` for filtered lists (e.g. `$folderMeta.filter(f => $progressMap[f.diskPath]?.bookmarked)`), `$state` for local UI state (hover, focused card).
- **Reactive data**: the recents dropdown uses `$derived` over `get_recents()` Tauri command, called once on mount (the list is a fixed size so we don't poll).
- **No Rust calls** on this screen — the recents list is loaded at app start, the progress map is loaded with `loadProgress()` after the folder is picked.
- **Routing**: `goChapter(path)` is the only navigation; the brand-row `onclick` is `goHome()`.
- **No tests** specifically for Home — the e2e suite exercises the routes (e.g. "Library / Home renders" implicitly through the brand-row click in the responsive test).

**Gotchas.**
- The recents list is a *fixed cap* (12) on the Rust side, but the UI may show fewer if some entries are stale (folder no longer exists). We filter the list at read time to grey out / drop missing folders.
- Continue Reading is sorted by *last-touched* time, derived from the progress map's order. The progress map is a `HashMap`, not ordered, so we use the iteration order as a proxy (most-recent first by save time).
