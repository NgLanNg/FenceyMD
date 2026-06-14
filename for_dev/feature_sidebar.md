# Sidebar chapter tree

## Vision & DoD (5W1H)

**What.** The sidebar (left pane) shows the active book's chapter hierarchy as a nested tree. Top-level files appear in a "Root files" section. Subfolders appear as collapsible groups, each with their own chapter list inside. A search/filter input at the top lets the user quickly narrow the tree to a chapter whose name matches.

**Why.** For a book of 50+ chapters, a flat list is unusable. The tree is the *navigation* surface — the user should be able to find any chapter in two or three clicks. The filter is for when the user knows what they're looking for ("show me the chapter on `outlines`") and doesn't want to scroll.

**Who.** A user reading a multi-chapter book. The tree is always visible while reading; on narrow viewports (< 768 px) it collapses to a drawer that opens from the brand row.

**When.** Always visible when a book is open. The filter is local state — clears when the book changes or when the user explicitly clears the input.

**Where.** Rendered by `src/components/Sidebar.svelte` (the wrapper) and `src/components/SidebarTree.svelte` (the tree itself) and `src/components/TreeNode.svelte` (a single node). The data source is `folderMeta` (root files) + `groupMeta` (subfoldered files) from the library store.

**How (acceptance / DoD).**
- The active book name appears in the sidebar header.
- A search/filter input narrows the tree in real time.
- Each group is collapsible; collapse state is preserved per-book across sessions.
- The currently-open chapter is highlighted in the tree.
- A chapter click navigates to that chapter.
- At viewports < 768 px, the sidebar collapses to a drawer; opening the drawer shows it above all other content (correct z-index).
- Progress (bookmarks, scroll) is reflected in the tree (bookmark icon next to bookmarked chapters, scroll bar on partially-read chapters).

---

## How we implemented it

**What.** A Svelte 5 component tree: `Sidebar` (container) → `SidebarTree` (recursive tree) → `TreeNode` (single node). Each `TreeNode` is a recursive component that renders its children if it's a group.

**Why this shape.** Recursive components in Svelte 5 (with `$props()`) are the natural way to render trees. Each node knows about its own expand/collapse state and is responsible for rendering its children — no central tree-state manager.

**When.** Mounted by `App.svelte` whenever a book is open. Collapse state is persisted to `localStorage` keyed by book path (in `prefs.js`).

**Where.**
- `src/components/Sidebar.svelte` — the wrapper, contains the brand row, recents dropdown, filter input, and the tree.
- `src/components/SidebarTree.svelte` — entry point that takes the `folderMeta` and `groupMeta` arrays.
- `src/components/TreeNode.svelte` — recursive; receives a `{name, path, children?, ...}` node.
- `src/lib/stores/prefs.js` — collapse state persisted in `localStorage` under `fenceymd.sidebarCollapsed.<bookPath>`.

**How (tech).**
- **Svelte 5 runes**: each `TreeNode` uses `$state` for `expanded` (local), `$derived` for `isActive` (compares to `$route.path`).
- **Filter**: a simple substring match on chapter name + path. Case-insensitive, debounced by ~50 ms (so typing in the filter doesn't lag the input).
- **Collapse persistence**: a `$effect` writes the expanded set to `localStorage` whenever it changes; on mount, it's read back. The key is the book's `folderRoot` (absolute path), so each book has independent collapse state.
- **Drawer mode** (< 768 px): the sidebar gets a `navOpen` store value (boolean) that flips on the brand row click. z-index is 100 in drawer mode (above the reader toolbar at 50). The backdrop is 95.
- **Bookmark/sroll indicators**: a `.tree-bookmark` class is added for bookmarked chapters (icon visible); a `.tree-scroll-bar` shows the saved scroll as a thin colored bar on the left of the title.

**Gotchas.**
- The recursive `TreeNode` was historically the cause of edit-loop bugs — the `editing` flag used to be read inside an `$effect` that also wrote it. We tracked that down and the fix was to read only `path` in the route-sync effect.
- Filter performance: for very large books (> 500 chapters), the filter scan is ~10 ms per keystroke — acceptable. Beyond that we'd need a virtualized list.
- `localStorage` key collision: the `folderRoot` is the absolute path; if the user moves the folder, the collapse state is effectively orphaned (silently no longer used). Acceptable.
