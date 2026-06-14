# File watching (live external edits)

## Vision & DoD (5W1H)

**What.** When the user has a folder open and an external process (their editor, git pull, a script) modifies a `.md` file in that folder, the change appears in FenceyMD within 1-2 seconds. The sidebar updates if files were added/removed; the Reader reloads if the current chapter changed; the editor reloads if it's open on a changed file.

**Why.** A reader is a window into a folder, not a sandbox. The user's external workflow (git, scripts, editors) is the source of truth; the app should reflect it.

**Who.** Any user with a folder open who uses an external tool to modify files.

**When.** Continuously, while a folder is open. The watcher uses a debounced notify (filesystem events) to coalesce bursts (e.g. git checkout) into a single rescan.

**Where.** `src-tauri/src/main.rs#watch_folder` is the Tauri command. `notify-debouncer-mini` is the Rust crate. The JS side listens for Tauri events emitted by the watcher.

**How (acceptance / DoD).**
- Editing a file in the user's external editor appears in the Reader within 1-2 seconds.
- Adding a new `.md` file in the folder appears in the sidebar.
- Deleting a file removes it from the sidebar (and closes the Reader if it was open).
- Renaming a file updates the sidebar entry.
- A burst of changes (e.g. git checkout) coalesces into a single rescan (no thrash).
- The watcher has zero CPU impact when nothing's happening.

---

## How we implemented it

**What.** A Tauri command (`watch_folder`) that:
1. Takes a folder path.
2. Stores the watcher in a `Mutex<Option<Watcher>>` in `WatcherState`.
3. On filesystem events, debounces for 500 ms, then emits a Tauri event to the JS side with the changed path.

**Why this shape.** `notify` is the standard cross-platform filesystem watcher. `notify-debouncer-mini` is the wrapper that coalesces bursts. The Tauri-managed state keeps the watcher alive for the lifetime of the folder.

**When.** Runs as long as a folder is open. Replaced when the user opens a different folder.

**Where.**
- `src-tauri/src/main.rs#watch_folder` — the command.
- `src-tauri/src/main.rs#WatcherState` — the managed state.
- `src/components/App.svelte` — the JS listener.

**How (tech).**
- **Rust crate**: `notify-debouncer-mini = "0.4"` over `notify = "6"`. The debouncer is the standard "wait 500ms, then fire" pattern.
- **Tauri event**: `app.emit("file-changed", &path)`. The JS listener handles it by either re-scanning the folder (for add/remove) or re-reading the file (for modify).
- **Mutex protection**: `Mutex<Option<Watcher>>` because the watcher holds a non-Send guard that can't be shared across threads. The mutex is poisoned-tolerant (`unwrap_or_else(|p| p.into_inner())`).
- **JS handler**: lightweight; for "modify," we just re-read the file. For "add/remove," we re-scan the whole folder.
- **Editor integration**: when the user is editing in the in-app editor and the file changes externally, the editor shows a banner "this file was changed externally" and offers to discard the local changes and reload.

**Gotchas.**
- The v1.0 version had a `Mutex::lock().unwrap()` that panicked on a poisoned lock. v1.1 fixed this with `lock().unwrap_or_else(|p| p.into_inner())`.
- The debouncer is critical. Without it, `git checkout` (which touches hundreds of files) would re-render the chapter hundreds of times.
- On macOS, `notify` uses FSEvents; on Linux, inotify; on Windows, ReadDirectoryChangesW. All three are efficient for typical use.
- The watcher is stopped on folder change (we don't want stale events from a previous folder).
