# Cross-chapter search (⌘⇧F)

## Vision & DoD (5W1H)

**What.** A full-text search panel (⌘⇧F) that searches every chapter in the active book at once. The user types a query; the panel shows ranked results with title, path, and a snippet of the matching text. Clicking a result navigates to that chapter and closes the panel.

**Why.** A book of 50+ chapters needs full-text search. The user's mental model is "where did the author talk about X?" — chapter-by-chapter find doesn't answer that.

**Who.** Any user with a book of > 5 chapters. Smaller books don't really need this; the in-chapter find is enough.

**When.** ⌘⇧F opens the panel. The panel debounces input by 100 ms; results update live as the user types.

**Where.** `src/components/CrossSearchPanel.svelte` is the panel. `src/lib/cross-search.js` is the MiniSearch wrapper.

**How (acceptance / DoD).**
- ⌘⇧F opens the panel.
- Typing a query shows ranked results with title, path, and snippet.
- Each snippet has the matched term highlighted in `<mark>` tags.
- The panel shows a "1 of 5" hit counter and a footer with the count.
- Clicking a result navigates to that chapter and closes the panel.
- Arrow keys / Tab cycle through results.
- Esc closes the panel; focus returns to the Reader.
- A second ⌘⇧F after a search restores the previous query and re-runs it.

---

## How we implemented it

**What.** A Svelte 5 panel component that subscribes to `crossSearchOpen` + `crossSearchQuery` stores. A MiniSearch index is built (or rebuilt) whenever the active folder changes. Searches are run on the client.

**Why this shape.** MiniSearch is fast (~5 ms to search 10 MB of text), gives ranked results with prefix + fuzzy matching, and lets us store the snippet text directly in the document. We don't need a server-side search for a local app.

**When.** ⌘⇧F opens the panel. The search runs on every keystroke (debounced 100 ms).

**Where.**
- `src/components/CrossSearchPanel.svelte` — the panel UI.
- `src/lib/cross-search.js` — `buildSearchIndex`, `runSearch`, `makeSnippet`.
- `src/lib/stores/state.js` — `crossSearchOpen`, `crossSearchQuery` stores.

**How (tech).**
- **Index**: `MiniSearch` v7 with `{ idField: 'id', fields: ['title', 'body', 'fenceText'], storeFields: ['id', 'path', 'name', 'title', 'body', 'fenceText'], searchOptions: { prefix: true, fuzzy: 0.2, boost: { title: 3, body: 1, fenceText: 1 } } }`.
- **Build**: `buildSearchIndex(folderMeta)` is called after every folder open. The index is rebuilt on every folder change; for a 50-chapter book it's ~50 ms.
- **ID uniqueness**: the document `id` is `diskPath || path` — needed because the renderer's `path` is group-stripped (e.g. `README.md`), which collides on `MiniSearch.addAll`'s required unique-id constraint.
- **Snippet**: `makeSnippet(body, query)` finds the first occurrence of the query in the body, takes a window around it, and returns the text + match indices for `<mark>` rendering.
- **Keyboard**: ⌘⇧F toggles; arrows cycle; Enter jumps to the highlighted result; Esc closes.
- **Try/catch in `openScanResult`**: a `MiniSearch: duplicate ID` error (from any future name collision) is logged and the search index is skipped, but the folder still opens. Belt-and-suspenders against the v1.1 bug where a duplicate `README.md` crashed the open flow.

**Gotchas.**
- The ID collision bug we fixed in v1.1: two top-level `README.md` files in a scanned folder both have `path: "README.md"` after group-stripping; MiniSearch's `addAll` requires unique IDs. We use `diskPath` (full group-prefixed path) as the ID.
- The `getDoc(path)` helper in the snippet panel looks up by either `id` or `path` so callers don't have to care which form the search result uses.
- The 100 ms debounce is needed; without it, a fast typer spawns 5+ searches per second, which on a 50-chapter book costs 25 ms each.
- A very long chapter (1 MB markdown) makes the snippet's body large; the panel truncates the body to the first 50 KB before indexing to keep the index small.
