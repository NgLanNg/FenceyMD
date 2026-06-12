# Snapshot — capture the app window to the clipboard

A "snapshot" in MD Reader is a screenshot of the app's own window,
pushed to the system clipboard so you can paste it into Slack, Notes,
Finder, Preview, or any other app. Captures the full app layout
(chrome + page content) at native resolution (Retina-aware — a
1100×820 CSS window captures as 2200×1640 px).

> **Roadmap:** the architecture leaves room for region-select later
> (crop just a paragraph, a diagram, a table). The Rust command is
> parameterizable; the JS helper is one extra arg away. Today it's
> "full app layout only".

## How to take a snapshot

**From the keyboard** (anywhere in the reader):

| OS | Shortcut |
|---|---|
| macOS | `⌘⇧S` |
| Windows / Linux | `Ctrl + Shift + S` |

**From the UI:** click the camera icon in the reader toolbar (between
the PDF-export and bookmark icons).

Both paths do the same thing: capture the foreground MD Reader window,
push the PNG to the clipboard, and show a toast like:

```
Copied 2200 × 1640 to clipboard.
```

Then `⌘V` / `Ctrl+V` anywhere.

## What gets captured

- The full OS window including title bar, sidebar, and toolbar.
- Any content currently on screen: chapter markdown, rendered
  diagrams, the slides view, the open zoom overlay, the Settings
  panel — whatever was visible at the moment of capture.
- The native display's pixel scale is preserved (Retina captures at
  2× the CSS size).

What's **not** captured:
- The mouse cursor.
- Content scrolled out of view (this is a window capture, not a
  full-page screenshot — region-select is the answer for that, when
  it lands).
- Other monitors, the menu bar, or the dock.

## Permissions

Capturing **your own app's window** does not require screen-recording
permission on any of the supported platforms:

- **macOS:** xcap goes through `CGWindowListCreateImage`, which allows
  self-capture without the TCC prompt. No "Screen Recording" toggle
  needed in System Settings.
- **Windows:** the app is sampling its own HWND; no special permission.
- **Linux:** X11 ignores the security extension for the focused window;
  Wayland via the `xdg-desktop-portal` screencopy API (xcap handles
  this transparently).

If a future OS release tightens this, the snapshot will fail with a
clean error string in the debug log; the user-visible toast will say
"Snapshot failed: <reason>". See `DEBUG.md` for how to read the log.

## Architecture (for contributors)

```
JS                                      Rust
─────────────────────────              ─────────────────────────────
Reader.svelte                          snapshot_app_to_clipboard
  └─ takeSnapshot()                       ├─ xcap::Window::all()
       │                                  │   └─ filter by pid + "md reader"
       ▼                                  ├─ Window::capture_image()
tauri.js: snapshotApp()                  │   └─ image::RgbaImage
       │                                  └─ arboard::Clipboard::set_image
       ▼                                       │
invoke('snapshot_app_to_clipboard')           ▼
                                       Returns { width, height, bytes }
                                       (for the toast)
```

The toolbar button + `⌘⇧S` keyboard handler both call the same
`takeSnapshot()` function, which awaits `snapshotApp()` and shows a
toast. The Rust command writes trace lines to the debug log at every
step (start, errors, success dimensions) so a failure is diagnosable
without re-running.

## Standalone test harness

The same Rust code path is also exposed as a runnable example, so you
can verify capture + clipboard without going through the WebView. This
is the diagnostic tool of choice when a snapshot stops working.

```bash
# Make sure /Applications/MD Reader.app is open, then:
cd src-tauri
cargo run --release --example snapshot_test
```

Output (success):
```
[snapshot-test] start
[snapshot-test] found 30 windows
[snapshot-test] picked window: app_name=Some("MD Reader") title=Some("MD Reader")
[snapshot-test] captured 2200x1640 (14432000 bytes RGBA)
[snapshot-test] wrote /Users/.../target/snapshot-test-output.png
[snapshot-test] clipboard set ok
[snapshot-test] OK
```

Exit codes are explicit so you can tell which step failed:

| Exit | Step | What to check |
|---|---|---|
| 1 | `xcap::Window::all()` | Window enumeration blocked; check macOS Screen Recording permission (shouldn't be needed for self-capture, but a strict MDM policy could block it) |
| 2 | No MD Reader window | The filter (pid + app name) didn't match anything. Is the .app running with the correct bundle id (`com.mdreader.app`)? |
| 3 | `capture_image()` | The window exists but capture failed. Often a permission or stale-window issue; try restarting the app |
| 4 | Zero-dimension image | Should not happen; the OS reported a 0×0 window. Re-run after a few seconds |
| 5 | PNG save | Output path is wrong or the disk is full. The harness writes to `target/snapshot-test-output.png` |
| 6 | Re-decode | Internal sanity check. If this fails, the captured image is corrupt — file a bug |
| 7 | `Clipboard::new()` | The platform clipboard isn't available (rare; usually means the system is in a locked state) |
| 8 | `Clipboard::set_image` | Clipboard exists but the OS rejected the image. macOS sometimes does this if the image is too large |

Even if exit 7/8 fails, the PNG on disk is still good. You can open
it in Preview and paste it manually.

## Adding region-select (the future cut)

Three small changes:

1. **`snapshot_app_to_clipboard`** — accept an optional
   `region: Option<RegionRect>` arg where `RegionRect = { x, y, w, h }`.
   When set, capture just that rect from the window.
2. **`tauri.js: snapshotApp(region?)`** — pass the arg through.
3. **Reader UI** — add a "region" toggle that overlays a draggable
   selection rectangle on the chapter content; on release, call
   `snapshotApp({ x, y, w, h })`.

The hot path (xcap call, arboard push) doesn't change.

## Related

- `DEBUG.md` — how to read the activity log when something goes wrong
- `DEVLOOP.md` — the broader build / verify loop
- `ROADMAP.md` — v2 entry for AI-anchor-based edits (a different
  feature but the same "surgical capture" instinct)
