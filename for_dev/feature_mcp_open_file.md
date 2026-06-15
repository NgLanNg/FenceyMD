# MCP tool: open_file with auto-resolve

## Vision & DoD (5W1H)

**What.** The MCP `open_file` tool takes a `path` argument. If the path is relative, it's interpreted as a chapter inside the currently-active book folder. If the path is **absolute**, the resolver automatically figures out which book folder contains the file (from the recents list), switches the active folder to that one, and navigates to the file. The agent never has to know "what book is the user currently looking at" — it just says "open /abs/path/to/x.md."

**Why.** Most agents don't track the user's UI state. They work in terms of absolute paths (the file they want to mention). The auto-resolver means the agent's natural workflow ("open this file I just mentioned") works without coordination.

**Who.** Any agent calling `open_file`. Common case: the user pastes a path from a file the agent just wrote, the agent calls `open_file` so the user can review the change in FenceyMD.

**When.** Every `open_file` call. The resolver runs server-side; the JS side gets a `mcp-folder-changed` event if the folder switched, then navigates.

**Where.** `src-tauri/src/mcp.rs#resolve_open_target`. Returns `(folder_root, relative_path)`. The caller (tool_open_file) emits the events.

**How (acceptance / DoD).**
- A relative path opens inside the active folder (existing behavior).
- An absolute path inside the active folder is treated as relative (no switch).
- An absolute path inside any recent folder switches the active folder to that one and navigates.
- An absolute path whose parent (or any ancestor) is in recents switches to the closest matching ancestor.
- An absolute path with no recents match returns a clear error: "not inside the active folder and not in any recent folder. Open the folder first (or have the user open it), then retry."
- The JS side re-scans the new folder and populates the `folderMeta` store before the navigate lands (no "Content not available" flash).

---

## How we implemented it

**What.** A Rust function `resolve_open_target(app, raw_path) -> Result<(folder_root, rel_path), String>` that runs a 4-step algorithm:

1. **Empty path** → error.
2. **Relative path** → require an active folder; return `(active, raw_path)`.
3. **Absolute path, already inside active folder** → return `(active, canonicalized_relative)`.
4. **Absolute path, search recents** → for each recent folder (most-recent first), check if the file is inside. If yes, return `(folder, rel_path)`.
5. **Absolute path, walk up parents** → for each ancestor (up to 16 levels), check if it's a recents entry. If yes, return `(folder, rel_path)`.
6. **No match** → error.

**Why this shape.** The 4-step algorithm reflects the most-common case first (active folder) before going to the slower recents scan, and the walk-up is the fallback for paths deep in a tree. 16 levels is a sane cap to avoid walking the whole filesystem.

**When.** Every `open_file` MCP call. Same algorithm in `get_chapter_content` (which has the same "where is this file" problem).

**Where.**
- `src-tauri/src/mcp.rs#resolve_open_target` — the resolver.
- `src-tauri/src/mcp.rs#tool_open_file` — the caller, emits `mcp-folder-changed` + `mcp-navigate`.
- `src-tauri/src/mcp.rs#tool_get_chapter_content` — also uses the resolver, but doesn't switch folders (it just needs to find the right file).
- `src-tauri/src/main.rs#mcp_recents` — the recents list accessor (exposed from main to mcp).
- `src/App.svelte` — the `mcp-folder-changed` listener that does the JS-side rescan + navigate.

**How (tech).**
- **Recents**: `mcp_recents(app)` in main.rs returns the persisted recents list filtered to directories that still exist on disk. The recents list is `<app_data_dir>/state.json` `recents` field.
- **Canonicalize**: `Path::canonicalize` resolves symlinks and `..`. The folder's canonical path is compared to the target's canonical path; if `target.starts_with(folder)`, the file is inside.
- **Relative path**: `Path::strip_prefix(folder)` gives us the file's path relative to the folder, which is what the renderer uses.
- **Combined event**: `mcp-folder-changed` payload is `{ root, nav_path }`. The JS handler awaits `scan_path(root)`, awaits `openScanResult(scan)` (which sets `folderMeta`), THEN calls `goChapter(nav_path)`. The combined event ensures the rescan completes before the navigate lands. **This was a real bug in v1.0** where the navigate and rescan were separate events and raced.
- **Group-stripping in the JS**: the renderer's `item.path` is group-stripped (e.g. `README.md` for `desktop-app/README.md`). The JS handler looks up the `folderMeta` item by `diskPath` (the full path) and uses its `path` for routing.
- **Search-index duplicate ID**: the auto-resolver often points to a folder with many sibling `README.md` files. The search index uses `diskPath` as the unique ID, not `path`, to avoid the `MiniSearch: duplicate ID` crash. Wrapped in try/catch in `openScanResult` so a search index error doesn't kill the folder-open flow.

**Gotchas.**
- A folder with 14 sibling `README.md` files (e.g. `Books/`) and an agent asking for `desktop-app/README.md` will land on the *first* `README.md` in `folderMeta` because the `find(diskPath === ...)` matches the first item with that diskPath. The fix (not done yet) is to stop the group-stripping in `buildIndexFromRecords`.
- The walk-up-parents cap of 16 is more than any sane book folder. Beyond that, we give up rather than walk the whole filesystem.
- The resolver's "active folder first" check is critical: without it, the user opening a file via the sidebar would get a redundant folder switch (and a brief rescan + UI flash).
- The combined event is the *only* correct way to do "folder switch + navigate" because Tauri events are sync-emit and async-handle — the navigate handler always runs before any async work in the folder-changed handler.
- **The view-state push has to use `diskPath`, not the group-stripped `path`.** The Reader's $effect (in `src/components/Reader.svelte`) that pushes `current_chapter_path` to the Rust MCP server uses `item?.diskPath || path`. Earlier the effect pushed `path` directly, and `get_current_chapter` returned `ERR_TOOL: could not read chapter: No such file or directory` for any nested file — because Rust joins the stored path to the active folder root, and the group-stripped `path` (e.g. `docs/MCP_SETUP.md`) is missing the `desktop-app/` group prefix the file actually lives at on disk. The fix is one line and matches the pattern used everywhere else in Reader.svelte (progress keys, link resolution, content enhancement — all key off `diskPath` for the same reason). Verified live: `open_file("/Users/alan/WORKSPACE/Books/desktop-app/docs/MCP_SETUP.md")` → `get_current_chapter` returns the 886-word chapter with the right preview.
