# Window snapshot to clipboard

## Vision & DoD (5W1H)

**What.** The user presses ⌘⇧S (or clicks the camera icon in the Reader toolbar) and the app's own window is captured to a PNG and pushed to the system clipboard. The user can paste into any other app — Slack, Notes, Preview, a tweet, an email.

**Why.** Users share what they're reading. The natural unit of sharing is "what I see on the screen right now" — not a PDF, not a chapter, just the visual.

**Who.** Anyone who wants to share a moment of their reading.

**When.** ⌘⇧S or the camera icon. Captures the current window, copies to clipboard, shows a toast with the dimensions.

**Where.** Triggered from the Reader toolbar (camera icon between PDF and bookmark). Also bound to ⌘⇧S in the global keymap.

**How (acceptance / DoD).**
- ⌘⇧S captures the current window.
- The PNG is on the system clipboard.
- A toast confirms with the dimensions (e.g. "Copied 2200 × 1640 to clipboard.").
- The capture is at native resolution (Retina: 2× the CSS size).
- Only the foreground FenceyMD window is captured (not the whole screen).
- The cursor is NOT in the capture.
- The capture works while reading, editing, or on the Library view.

---

## How we implemented it

**What.** A Rust Tauri command (`snapshot_app_to_clipboard`) that:
1. Uses `xcap` to enumerate the windows on the system.
2. Finds the FenceyMD window (by app name + owning pid).
3. Captures the window as an RGBA image.
4. Pushes the image to the system clipboard via `arboard`.

**Why this shape.** `xcap` is the standard cross-platform window capture library (works on macOS, Windows, Linux). `arboard` is the standard cross-platform clipboard library. Both are pure-Rust with no JS-side dependencies.

**When.** Triggered by ⌘⇧S or the camera icon. The capture + clipboard push takes ~50 ms.

**Where.**
- `src/components/Reader.svelte` — the toolbar button + keyboard handler.
- `src/lib/tauri.js` — `snapshotApp` wrapper.
- `src-tauri/src/main.rs` — `snapshot_app_to_clipboard` Tauri command.
- `src-tauri/examples/snapshot_test.rs` — the standalone test harness.

**How (tech).**
- **xcap**: `xcap::Window::all()` enumerates windows. We filter by `(pid == our_pid) || app_name.contains("md reader")`. Then `capture_image()` returns an `RgbaImage`.
- **arboard**: `Clipboard::new()` + `set_image(ImageData { width, height, bytes })`. The bytes are the raw RGBA buffer from xcap.
- **Permissions**: on macOS, capturing your own app's window does NOT require Screen Recording permission. `xcap` uses `CGWindowListCreateImage` which allows self-capture. On Windows, sampling your own HWND needs no special permission. On Linux, the focused window is capturable without the X security extension.
- **Standalone test**: `cargo run --release --example snapshot_test` runs the same code path outside the WebView. Writes `target/snapshot-test-output.png` and exits 0-8 with explicit codes per failure step. This is the diagnostic tool when a snapshot stops working.
- **Exit codes**: 1 = window enumeration failed; 2 = no FenceyMD window; 3 = capture failed; 4 = 0×0 image; 5 = PNG save failed; 6 = re-decode failed; 7 = clipboard open failed; 8 = clipboard set failed.

**Gotchas.**
- A headless-launch `open /Applications/Foo.app` from a non-GUI shell (SSH, sandbox) registers the process with the OS but the NSWindow is never instantiated. `xcap` returns 0 windows. **This is a verification-side issue, not a code bug** — the snapshot works on a real user launch from Finder/login.
- The clip is at native (Retina 2×) resolution, which can produce a very large PNG for a 4K monitor. arboard sometimes rejects very large images; we cap at the window's actual pixel dimensions.
- The native Save dialog for "save snapshot to file" (if the user wants that) is separate from the clipboard flow; the current feature only does clipboard.
