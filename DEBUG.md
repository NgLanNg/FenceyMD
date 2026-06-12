# Debug — when something goes wrong, read the log

The WebView's devtools aren't visible inside the Tauri shell, and
`console.log` lines are lost the moment the app closes. To give
yourself (and us) a paper trail, the app writes a structured
activity log to a file on disk, and the Settings panel can take you
straight to it.

## Where the log lives

`<app_data_dir>/debug.log`. On macOS:

```
~/Library/Application Support/com.mdreader.app/debug.log
```

On Linux:

```
~/.local/share/com.mdreader.app/debug.log
```

On Windows:

```
%APPDATA%\com.mdreader.app\debug.log
```

It's append-only, timestamped (UTC seconds since epoch in `[…]`
brackets at the start of each line), and survives app restarts.

## How to read it

**From the app:** Settings → Debug → "Open log folder" (Reveals in
Finder / Explorer / xdg-open) or "Clear log" (truncates the file).

**From the terminal:**

```bash
# macOS
tail -f ~/Library/Application\ Support/com.mdreader.app/debug.log

# Linux
tail -f ~/.local/share/com.mdreader.app/debug.log

# or just open it
open ~/Library/Application\ Support/com.mdreader.app/debug.log
```

The file is small (a few KB per session). Tail it in another
terminal while you reproduce the bug.

## What gets logged

The log captures four kinds of events:

### 1. Boot

```
[1781245560] [boot] main.js loaded
```

Fires once at app start, right after `main.js` is parsed. If you
don't see this line, the app didn't get far enough to even attach
its error handlers.

### 2. Folder-open chain

```
[1781245560] [openLast] start id=tmqajm748_1
[1781245560] [openLast] invoking open_last
[rust] open_folder_path: path=/Users/.../demo files=14 bytes=33815 elapsed_ms=3
[1781245560] [openLast] got scan {"root":"...","fileCount":14}
[1781245560] [openScanResult] start id=... {"root":"...","fileCount":14}
[1781245560] [openScanResult] index built {"ms":0}
[1781245560] [openScanResult] progress loaded
[rust] watch_folder: started on /Users/.../demo
[1781245560] [openScanResult] ok id=... {"totalMs":12}
[1781245560] [openLast] ok id=... {"totalMs":42}
[1781245560] [openScanResult] watch_folder started
```

Every step in `pickFolder` / `openFolderPath` / `openLast` /
`openScanResult` is traced with a unique id so you can see exactly
where the chain broke. Rust-side lines (prefixed `[rust]`) come from
`scan_folder` / `pick_folder` / the watcher callback.

**Common diagnoses:**

| Symptom in log | Meaning | Fix |
|---|---|---|
| `openLast start` … nothing | JS hung in the await chain; check for a `caught` line further down | Look for the matching `caught` |
| `invoking open_last` … `[rust]` line never appears | The Tauri IPC bridge isn't delivering the call to Rust | Re-open the app; check for an "IPC" error elsewhere |
| `open_folder_path: path=…` says `not a dir` | The path in recents no longer exists | Tauri returns `None`, the UI shows nothing — click "Open Folder" and pick a new one |
| `open_folder_path: files=N bytes=M elapsed_ms=…` is huge | The folder has many large `.md` files; the JSON payload is slow to serialize / deserialize | This is the most likely cause of "click recent, nothing happens" on docs folders. The wait is real, not a hang. |
| `openScanResult ok` but UI never leaves the picker | `editing` flag or `route` stuck; check for a thrown error | Look for an unhandled exception line further down |

### 3. Window snapshots

```
[1781246800] [snapshot] start
[1781246801] [snapshot] ok {"width":2200,"height":1640,"bytes":14432000}
```

Or on failure:

```
[1781246800] [snapshot] start
[1781246801] [snapshot] err window not found
```

The toast in the UI is the user-visible signal; the log is for
agents / future-you. See `SNAPSHOT.md` for the full pipeline and
the standalone test harness.

### 4. Unhandled errors (the catch-all)

```
[1781246000] [window.error] TypeError: …    <filename>:10:5
[1781246010] [window.unhandledrejection] NetworkError: … <stack>
```

These come from the `window.addEventListener('error', …)` and
`unhandledrejection` handlers in `main.js`. They fire for anything
that didn't have its own try/catch — the last line of defense for a
silent failure.

## Recipes

### "I clicked a recent and nothing happened"

1. Open Settings → Debug → Open log folder
2. Reproduce the click
3. Read the log from the bottom up
4. Look for the `[openFolderPath] start` line — if it's missing, the
   click never reached the handler. If it's there but the chain
   stops, the next line tells you which step blocked.

### "The app is slow to open my big docs folder"

1. Open the log
2. Find the most recent `open_folder_path: files=…` line
3. Read `elapsed_ms=…`
4. Anything under 100 ms is fast. 100 ms–1 s is "big folder, wait it
   out". 1 s+ is "consider filtering the folder; the entire scan
   result is shipped as a single JSON payload to the WebView"

### "Snapshot did nothing / showed an error"

1. Open the log
2. Find the `[snapshot]` lines for the most recent attempt
3. If you see `[snapshot] err window not found`: the .app is
   probably running under a different bundle id, or the `app_name`
   filter is wrong. Run the standalone harness to confirm:

   ```bash
   cd src-tauri
   cargo run --release --example snapshot_test
   ```

   If the harness finds the window, the filter is right; the issue
   is the WebView → Rust IPC. If the harness also fails, the
   platform is blocking capture.

### "I want a clean log before reproducing"

Settings → Debug → Clear log. Then reproduce. The next entry will
be `[boot] main.js loaded`.

## Privacy

The log contains:

- Absolute filesystem paths of folders you open
- File names (but not their contents)
- Error messages and stack traces
- Window dimensions and pixel counts
- No chapter content, no user input, no clipboard content

When sharing a log with someone else, the paths may already give
away your project structure — review before pasting publicly.

## Implementation pointers

- `src/lib/debug-log.js` — the `dlog()` helper used throughout
  the JS code
- `src-tauri/src/main.rs` `debug_log` / `debug_log_clear` /
  `debug_log_path_str` / `debug_log_reveal` — the Tauri commands
- `src/main.js` — the window-level error / rejection handlers
- `src/lib/stores/library.js` — folder-open chain instrumentation
