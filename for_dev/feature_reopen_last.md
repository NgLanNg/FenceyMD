# Reopen last folder on launch

## Vision & DoD (5W1H)

**What.** A toggle in Settings that, when on, makes the app re-open the last folder the user was reading on launch. When off, the app launches to the empty Library view.

**Why.** Most reader apps do this by default. The user expects "open the app, see my book" — not "open the app, navigate to my book, see my book." A toggle exists because some users want a clean start every time (or have a workflow where they always pick a different folder).

**Who.** Default-on for everyone. Power users can opt out.

**When.** On every app launch, if the toggle is on and a `last_folder` is in the persisted store, that folder is opened.

**Where.** `src/lib/stores/prefs.js#reopenLast`. Rust's `open_last` Tauri command reads the persisted state.

**How (acceptance / DoD).**
- Toggle on (default): launching the app re-opens the last folder within 1-2 seconds.
- Toggle off: launching the app shows the empty Library view.
- Toggling the setting at runtime takes effect on the *next* launch.
- If the last folder no longer exists (moved/deleted), the app falls back to the empty Library view and clears the entry.
- The recents list is also persisted separately; toggling doesn't affect the recents dropdown.

---

## How we implemented it

**What.** A Tauri command (`open_last`) that:
1. Reads `state.json` for `last_folder`.
2. If present and the folder exists, scans it and returns the `ScanResult`.
3. If absent or the folder is gone, returns `None`.

**Why this shape.** The Tauri command runs synchronously at app startup (before the window is shown) so the user never sees a flash of the empty Library. The folder existence check prevents a confusing "folder is gone" error on the wrong drive.

**When.** Once per launch, in the `setup` hook. The JS side calls `openScanResult` on the result.

**Where.**
- `src-tauri/src/main.rs#open_last` — the command.
- `src-tauri/src/main.rs#read_store` — reads `last_folder` from `state.json`.
- `src-tauri/src/main.rs#record_open` — updates `last_folder` on every `open_folder_path` call.
- `src/lib/stores/prefs.js#reopenLast` — the toggle (persisted to localStorage).
- `src/components/Settings.svelte` — the toggle UI.

**How (tech).**
- **Tauri command**: returns `Option<ScanResult>`. The JS side handles the `None` case (no folder → empty Library).
- **Store path**: `<app_data_dir>/state.json`. Read once on launch.
- **Folder existence**: `Path::new(last_folder).is_dir()`. If false, we clear the entry (so the next launch is also clean).
- **JS wiring**: `App.svelte`'s onMount calls `invoke('open_last')` and routes through `openScanResult` if the result is non-null.
- **The "always re-pick" UI**: when the toggle is off, the user can still pick a folder from the recents dropdown — the recents list is unaffected by the toggle.

**Gotchas.**
- The "always open last folder" was unconditional in v1.0 (no toggle). v1.1 added the toggle and made it default-on.
- If the user has the toggle off, the recents list still works; they just don't auto-open. The recents dropdown is the manual override.
- The folder existence check is a `is_dir()` call, not a deep scan. A folder with a missing `.md` subdirectory is still "exists" from the auto-open perspective.
