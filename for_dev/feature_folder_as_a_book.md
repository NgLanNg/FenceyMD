# Folder as a book

## Vision & DoD (5W1H)

**What.** FenceyMD is organized around the idea that *a folder is a book*. You point the app at a directory on disk; the directory's `.md` files become the book's chapters, and subdirectories become chapter groups. There's no project file, no manifest, no import step — the filesystem *is* the book structure.

**Why.** Long-form content (a draft, a research corpus, an LLM output dump, a book) lives in folders, not in app-specific databases. Treating the folder as the book means:
- the user's existing file organization is respected — no parallel structure to maintain
- the book is portable — drop the folder into Dropbox, Git, an email attachment, it works
- the book is durable — there's no app lock-in, no proprietary format

**Who.** A user with a folder of `.md` files. No prior setup required.

**When.** The app launches with no folder open → the Library/Home view shows. The user picks a folder → that folder becomes the active book. The user can switch to a different folder at any time (File → Open Folder, ⌘O, or the recents dropdown).

**Where.** The folder lives wherever the user puts it — the app doesn't copy it, doesn't sync it, doesn't modify it without explicit save. Reads are read-only by default; writes (edits, image pastes) go through Rust and are bounded to the active folder root.

**How (acceptance / DoD).**
- A user can pick a folder from the OS file picker.
- A user can pick a folder from the recents dropdown (last 10).
- A user can switch folders at any time; the previous folder is unaffected.
- The active folder persists across restarts (the "reopen last folder on launch" setting).
- Writes (edits, image saves) never escape the active folder — path-traversal is rejected.
- The folder is the source of truth: external edits appear live (file watcher).

---

## How we implemented it

**What.** A folder becomes a book via a single Rust command (`open_folder_path`) that walks the directory, builds a flat list of `.md` files (each with `{relative_path, name, content}`), and ships the result to the JS side as a `ScanResult`.

**Why this shape.** The walk-and-bundle approach is a deliberate trade:
- **Pro**: the JS gets the whole book in one shot. The Reader can navigate between chapters without round-tripping to the backend. The sidebar tree can render the full hierarchy without a second scan.
- **Con**: if the folder is huge, the initial scan is slow. We cap individual file size at 5 MB to bound memory.

**When.**
- `open_folder_path` runs synchronously on the user's "Open Folder" click.
- `open_last` runs once at app startup, if "reopen last folder" is on and a `last_folder` is in the persisted store.
- A live `watch_folder` task re-runs a per-file `notify` watcher on the active folder.

**Where.**
- The folder path is stored in Tauri's managed `McpState.active_folder_root` for the MCP server to read.
- The folder name + file list is stored in JS's `folderRoot` + `folderMeta` + `groupMeta` stores (Svelte writables).
- The folder path is persisted in `<app_data_dir>/state.json` as `last_folder` and in a bounded `recents` list (capped at 12, most-recent first).

**How (tech).**
- **Walk**: `walkdir` crate, filter to `.md` files, skip hidden directories (`.git`, `.obsidian`, anything starting with `.`).
- **Path safety**: each result's `rel_path` is `Path::strip_prefix(root)` so a sibling escape is impossible.
- **JS index**: `buildIndexFromRecords` in `src/lib/index.js` builds the `folderMeta` (all files) and `groupMeta` (only subfoldered files, keyed by top-level segment).
- **State**: `src/lib/stores/library.js#openScanResult` is the single entry point; it sets the stores, kicks off the file watcher, and hands the file list to the MCP server (`mcpSetActiveFolder`).
- **Recents**: `record_open` in `src-tauri/src/main.rs` inserts the folder at the head of `recents`, dedupes, and caps at 12.
- **Path-traversal guard**: `safe_resolve_in_folder` in `src-tauri/src/mcp.rs` is the canonicalize-and-bounds-check used by every file-touching tool and command.

**Gotchas (the ones that cost time).**
- `walkdir` returns the *root itself* as the first entry — we skip entries where `file_type().is_dir()`.
- Hidden directories (`.git`, `.obsidian`) balloon scans; we filter early by checking if any segment starts with `.`.
- The JS-side `buildIndexFromRecords` does a second group-stripping pass for rendering — `desktop-app/README.md` becomes just `README.md` in `folderMeta[].path`, with the full path preserved as `diskPath`. This collides for folders with sibling files of the same basename (we documented this limitation; future fix is to skip the stripping).
