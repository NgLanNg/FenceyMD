# MCP screenshot — better plan

**Status:** design — replaces the current xcap-based `capture_screenshot` tool.

## Why the current implementation isn't good enough

The current `capture_screenshot` uses xcap + the system clipboard + arboard to grab the live window. That's a fine choice for a user-visible ⌘⇧S feature (the user is at the GUI, the window is real, xcap works), but it's a bad choice for an MCP tool the agent calls. Three concrete problems:

1. **Platform-specific.** xcap wraps `CGWindowListCopyWindowInfo` on macOS, X11's `XGetImage` on Linux, and `PrintWindow` on Windows. Each has different quirks. macOS in particular requires the process to have an active `Aqua` session — headless launches (CI, SSH, automation contexts) fail to register the window with WindowServer, and xcap returns 0 windows. On Linux Wayland via xdg-desktop-portal, the screen capture requires user-mediated consent (a "select screen" dialog appears). On Windows, `PrintWindow` on a non-foreground HWND returns black.

2. **First-frame stale.** The first call to xcap after the window opens can return a cached frame (verified in this session: 4 consecutive `capture_screenshot` calls returned byte-identical PNGs of an empty window, then a fifth call after `osascript activate` returned a real frame). The reason is xcap's internal frame cache plus macOS's window-server commit latency.

3. **No Linux/Windows parity.** None of xcap's paths are MCP-friendly across the three target OSes.

## What the agent actually needs

An agent calling `capture_screenshot` wants to "see" the chapter the user is looking at. The agent does NOT need:
- The macOS title bar (irrelevant to the chapter content)
- The Reader's chrome (font toggle, theme button, etc.) — those are controls, not content
- Pixel-perfect fidelity with what the user sees

The agent DOES need:
- A visual representation of the chapter's rendered content
- A reliable, deterministic capture that works on every platform
- Freshness — the capture should reflect the chapter that's currently rendered, not a stale frame

## The better approach: render-on-demand via headless Chrome

The PDF export path already does this. It takes the chapter's HTML, runs it through headless Chrome with a viewport size, and produces a PDF. The same machinery can produce a PNG instead of a PDF.

### Architecture

```
[agent] tools/call capture_screenshot
        │
        ▼
[rust] tool_capture_screenshot
        │
        ├── 1. JS-side: snapshot the live Reader DOM
        │     (Reader.svelte exposes a getRenderedHTML() function
        │      that returns document.querySelector('.reader2')
        │      or .chapter-content's outerHTML)
        │
        ├── 2. Rust receives the HTML via a new Tauri command
        │     get_rendered_html() -> String
        │
        ├── 3. Rust takes the HTML + chapter metadata (title,
        │     word count, reading time)
        │
        ├── 4. Rust writes to a temp file, calls headless Chrome
        │     with --screenshot=path --window-size=1100,820,
        │     gets a PNG
        │
        ├── 5. Rust reads the PNG bytes, base64-encodes, returns
        │
        ▼
[agent] gets PNG bytes (always fresh, always works, platform-agnostic)
```

### Why this is better

- **Platform-independent.** Headless Chrome is the same code path on macOS, Windows, Linux. No xcap, no DWM, no xdg-desktop-portal.
- **No WindowServer dependency.** Headless Chrome doesn't need a visible window. It runs offscreen. The .app can be launched from any context (headless, CI, SSH, automation) and `capture_screenshot` works.
- **Always fresh.** The HTML is taken from the live DOM at the moment of the call. No frame cache to invalidate.
- **Smaller, predictable payloads.** The viewport is fixed (1100×820, the .app's CSS size). A chapter renders to ~50-200KB PNG. Not subject to the user's monitor resolution.
- **No native chrome.** The agent gets the chapter content, not the macOS title bar. This is what the agent wants anyway.
- **Reuses existing PDF path.** `build_print_html` already produces render-ready HTML. We can branch on output format (PDF vs PNG) at the headless Chrome invocation.

### What the user-visible ⌘⇧S feature should do

Keep the existing xcap-based path for the user. The user wants a screenshot of *what they see on their screen* — including the title bar, the toolbar, the sidebar. The xcap path is right for that.

The MCP `capture_screenshot` is a **different** feature, even though the name is the same. It serves a different consumer (an agent, not a human).

### Trade-offs accepted

- **~300ms latency** per capture (headless Chrome spin-up if cold, ~50ms if warm). The xcap path is ~50ms. For an agent that calls `capture_screenshot` once per "show me what you see" request, 300ms is fine.
- **No native chrome.** The agent doesn't see the toolbar or sidebar. The user can add a `include_chrome: bool` arg if they want.
- **HTML snapshot overhead.** The JS-side snapshot is ~5ms. The HTML is small (one chapter, sanitized). The network round-trip Rust↔JS is ~1ms.
- **Headless Chrome dependency.** Already required for PDF export. We reuse the existing binary finder (`find_chrome`).

### Implementation plan

#### Step 1: Add `get_rendered_html` Tauri command
- File: `src-tauri/src/main.rs`
- Signature: `pub fn get_rendered_html() -> Result<String, String>`
- JS-side: the Reader exposes a `window.__mdReader = { getRenderedHTML: () => string }` (or similar) that returns `document.querySelector('.chapter-content')?.outerHTML ?? ''`.
- Rust-side: the command reads the global via webview's IPC, returns the string.

#### Step 2: Add Rust-side `chapter_to_png` helper
- File: `src-tauri/src/main.rs`
- Signature: `fn chapter_to_png(html: &str, viewport: (u32, u32)) -> Result<Vec<u8>, String>`
- Implementation:
  1. Reuse the existing `find_chrome()` function (already supports macOS, Linux, Windows paths).
  2. Write HTML to a temp file in the app's cache dir.
  3. Spawn: `chrome --headless --no-sandbox --disable-gpu --hide-scrollbars --screenshot=path --window-size=W,H file:///.../temp.html`
  4. macOS: `--no-sandbox` is needed because Chrome's sandbox requires entitlements the .app doesn't have.
  5. Linux: same, plus `--disable-dev-shm-usage` (small `/dev/shm` on some Linuxes).
  6. Windows: no `--no-sandbox` needed (default Chrome sandbox works on Windows).
  7. Read the PNG bytes, return.
- Platform-specific Chrome flags can be selected via `#[cfg(target_os = "...")]`.
- Cleanup the temp file after the capture (best-effort; not critical).

#### Step 3: Rewrite `tool_capture_screenshot` in mcp.rs
- File: `src-tauri/src/mcp.rs`
- Old behavior: xcap + arboard.
- New behavior:
  1. Call `get_rendered_html` to get the chapter's HTML.
  2. Wrap with the same `<head>` / CSS that `build_print_html` uses (so fonts, code highlighting, mermaid render the same way).
  3. Call `chapter_to_png` with viewport (1100, 820).
  4. Base64-encode the PNG bytes, return.

#### Step 4: Update MCP tool description
- File: `src-tauri/src/mcp.rs#tool_definitions`
- Old: "uses the same xcap pipeline as the in-app ⌘⇧S shortcut"
- New: "Captures the current chapter as a PNG, rendered through headless Chrome. Always fresh, always works (no WindowServer / xcap dependency). Returns the chapter content (not the OS chrome)."

#### Step 5: Keep the user-facing ⌘⇧S unchanged
- File: `src-tauri/src/main.rs#snapshot_app_to_clipboard` (unchanged)
- File: `src/components/Reader.svelte` (unchanged)
- These still use xcap + arboard because the user wants their actual screen.

#### Step 6: Tests
- Rust unit: `chapter_to_png` on a sample HTML returns valid PNG bytes (decode, check dimensions).
- Rust integration: a `capture_screenshot` MCP call against a real .app returns PNG bytes; decode, check it's a PNG of expected dimensions.
- E2E (JS side): the `getRenderedHTML` global exists and returns the chapter content.

#### Step 7: Memory
- Update the agent memory entry on `Tauri 2 macOS Headless Launch` to reflect the new approach: "MCP screenshot no longer depends on xcap / WindowServer; uses headless Chrome, which works in any context."

#### Step 8: Docs
- Update `for_dev/feature_snapshot.md` to add a "MCP screenshot" subsection explaining the new approach.
- Update `for_dev/feature_mcp_capture_screenshot.md` to reflect the new implementation.
- Update the demo chapter 13 with the new behavior.

#### Step 9 (Windows-only optimization, future): WebView2 `CapturePreview`
- Windows users get a 10× speedup if we use the WebView2 `CapturePreview` API instead of spawning a separate Chrome process.
- Tauri's WebView2 backend (`tauri-runtime-wry`) exposes this through the underlying `Microsoft.Web.WebView2.Wpf.WebView2` or `WinUI WebView2` controls.
- Implementation: at the Tauri command level, branch on `#[cfg(target_os = "windows")]`. On Windows, use `Webview::with_webview(|w| { w.CapturePreview(...) })` to write PNG bytes directly. On macOS/Linux, use the headless Chrome path.
- This is a perf optimization, not a correctness one. Defer it until the headless Chrome approach is verified to work on all three OSes.

### Open questions

1. **Capture the whole Reader, or just the chapter content?** The HTML snapshot is `document.querySelector('.chapter-content')?.outerHTML` — that excludes the toolbar. The agent might want the toolbar (to know if the user is in dark mode, for example). The `include_chrome: bool` arg handles this.

2. **What if Chrome isn't installed?** The PDF export path already errors clearly. We'll reuse that error.

3. **What's the maximum PNG size?** Headless Chrome at 1100×820 typically produces 50-200KB. For a chapter with many images, ~500KB. Acceptable for an MCP response.

4. **What if the user is on the home view (Library), not a chapter?** The HTML snapshot returns the library's DOM. Headless Chrome renders it. The agent sees the library view, which is fine.

5. **Should we still keep the xcap path as a fallback?** For now, no. The headless Chrome path always works. If Chrome is missing, the user gets a clear error. If we find a use case where xcap is preferable, we add it back as `include_native_chrome: true`.

### What this means for the original "headless launch" problem

The headless-launch issue is **no longer relevant** for the MCP screenshot. The .app can be launched from any context (CI, SSH, automation), the MCP server starts, the agent connects, `capture_screenshot` works via headless Chrome. The user-visible ⌘⇧S (which uses xcap) still requires a GUI launch, but that's a separate concern (it's a user feature, not an agent feature).

The "no WindowServer, no window registered" is **solved** for the agent's needs.

### Platform-by-platform summary

| Platform | User-visible ⌘⇧S (xcap) | MCP `capture_screenshot` (proposed) |
|----------|-------------------------|-------------------------------------|
| **macOS** | Works if launched from GUI (Aqua session). Headless launch fails — xcap sees no window. | **Headless Chrome** spawns offscreen, renders the chapter HTML, returns PNG. Always works. **No GUI launch required.** |
| **Windows** | `xcap::PrintWindow` works on the .app's own HWND. No headless issue (no Aqua-session equivalent on Windows). | **Default: Headless Chrome** — same code as macOS, spawns `chrome.exe` / `msedge.exe` from `Program Files`. Works everywhere.<br>**Future (Step 9): WebView2 `CapturePreview`** — Tauri's WebView2 backend exposes a fast capture that returns PNG bytes directly without spawning Chrome. ~10ms vs ~300ms. Add a platform branch: `if cfg!(windows) { webview2_capture_preview() } else { headless_chrome() }`. |
| **Linux X11** | `xcap::XGetImage` works on the focused window. Requires the .app to be focused (clicking the window). | **Headless Chrome** works the same as macOS/Windows. **No X server needed** — Chrome runs headless offscreen. |
| **Linux Wayland** | `xcap` uses xdg-desktop-portal, which requires user consent. Annoying. | **Headless Chrome** is the only sensible path. The Tauri 2 Linux webview is WebKitGTK, which has no capture API. Headless Chrome is platform-agnostic and avoids the portal entirely. |

**Conclusion:** headless Chrome is the right *baseline* because it works on all three OSes without OS-specific code paths. The Windows `WebView2.CapturePreview` optimization is a nice-to-have for Windows users (faster) but not required for correctness.

### What I should NOT do

- Don't try to make xcap work in headless contexts (it fundamentally can't — it needs a real WindowServer).
- Don't add platform-specific fallbacks (DWM, xdg-desktop-portal, etc.) — headless Chrome is the one path that works everywhere.
- Don't make the user-visible ⌘⇧S use headless Chrome (the user wants their actual screen, not a re-render).
- **Don't wrap the binary in a shell script.** The right answer is to add CLI subcommands to the Rust binary itself.
