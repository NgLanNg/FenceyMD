# Dev Loop — how changes get made & verified

This is the working loop used to build FenceyMD: how a request becomes a
verified, shippable build. It favors a **tight feedback loop** (fast browser
checks) and **verify-before-ship** (nothing is called done until it's observed
working), because the app spans three layers — Svelte UI, Rust/Tauri backend,
and the native bundle — and a change can pass a build yet still be broken at
runtime.

```
  user request
       │
       ▼
  ① understand ──▶ ② implement ──▶ ③ build frontend ──▶ ④ verify (dev)
       ▲                                                      │
       │                                            pass? ────┤── fail ──▶ back to ②
       │                                                      ▼
       └──────────────────────────── ⑦ confirm ◀── ⑥ bundle ◀── ⑤ compile Rust
                                       (launch app)   (.dmg etc.)   (if backend changed)
```

---

## ① Understand — locate before touching

- Read the relevant files first; never edit blind.
- The codebase map:
  - `src/components/*.svelte` — UI (Picker, Sidebar, SidebarTree, Library, Reader, Editor, Settings)
  - `src/lib/stores/*` — state split into focused modules (`state`, `prefs`, `progress`, `library`, `files`), re-exported by `src/lib/stores.js` (barrel — components import from one place)
  - `src/lib/markdown.js` — render + `enhance()` (highlight, mermaid, diagram tools)
  - `src/lib/diagram-export.js`, `src/lib/tauri.js` — diagram image tools + the Tauri bridge
  - `src-tauri/src/main.rs` — Rust commands (folder scan, progress, `write_file`, `rename_file`, `save_export`, `copy_image`, watcher)
  - `src/app.css` — single stylesheet; theme via CSS custom properties + `data-theme`
- For unfamiliar areas, grep/read rather than assume. Confirm a class is actually
  used before deleting it (substring collisions are easy: `tool-sep` vs `editor-tool-sep`).

## ② Implement — smallest correct change

- Match existing patterns (CSS variables, store barrel, the `TAURI` guard).
- **Key constraint — the macOS WKWebView is not a full browser.** It can't:
  download via `<a download>`, write images to the clipboard, or run AppImage
  tools. Anything binary/native goes through a Rust command (`save_export`,
  `copy_image`) exposed via `src/lib/tauri.js`. PDF export hands the rendered
  chapter HTML to the Rust `print_pdf` command, which renders it with headless
  Chrome (`--print-to-pdf`) into a self-contained, always-light document — so
  text stays crisp vector. (Not `window.print()`, and not canvas rasterization.)

## ③ Build the frontend

```bash
npm run build      # vite — catches syntax/import/Svelte errors fast (~10s)
```
Watch for real errors; the a11y "non-interactive element" warnings are known/benign.

## ④ Verify in the dev server — the core of the loop

The Tauri webview can't be screen-scraped here, and macOS screen-recording is
blocked, so verification runs the **identical frontend** in headless Chrome via
Puppeteer against the Vite dev server. A `?test=1` mode loads sample data
(including a nested subfolder + a mermaid diagram) and exposes editor-only
features in browser mode.

```bash
# dev server (left running across iterations)
npm run dev                     # http://localhost:1420

# regression suite — 40 end-to-end checks
node e2e-test.mjs               # nav, render, font/theme/search/bookmark,
                                # edit-opens, content-loads, preview, etc.
```

- **Always re-run `e2e-test.mjs` after a change** — it's the safety net for the
  deep refactors (lazy-loading, stores split, CSS cleanup) and must stay 40/40.
- For anything visual, capture a **Puppeteer screenshot** and actually look at it
  (light + dark, the affected screen). This is how dark-mode contrast, the
  mermaid theming, the Settings modal, and the Root-files landing were checked.
- For new features, add a throwaway Puppeteer probe that asserts the behavior
  (e.g. "toggle flips `data-mmd-dark` 0→1", "Settings rows = Theme/Font/Width").

If a check fails → back to ②. Nothing proceeds to a bundle on a red suite.

## ⑤ Compile Rust — only if the backend changed

```bash
cd src-tauri && cargo build --release    # verify new commands compile (~50s)
```
Editing `main.rs`/`Cargo.toml` (e.g. adding `rename_file`, `arboard`/`image`)
goes through here before bundling, so a Rust error surfaces on its own.

## ⑥ Bundle the desktop app

```bash
# from desktop-app/
npm run build:desktop            # = tauri build (vite build + cargo + bundle)
# (cargo tauri build also works)
```
Output: `src-tauri/target/release/bundle/{macos,dmg}/…`

**Known macOS gotcha:** `bundle_dmg.sh` fails if a previous DMG is still mounted,
and it sometimes consumes the `.app` into the staging image. Standard cleanup
before/after a build:
```bash
for d in $(hdiutil info | grep /dev/disk | grep -iE 'dmg|md reader' | awk '{print $1}'); do hdiutil detach -force "$d"; done
ls /Volumes | grep -i 'md reader' | while read v; do hdiutil detach -force "/Volumes/$v"; done
```
If the `.app` got consumed, mount the produced `.dmg` and launch from there.

## ⑦ Confirm — observe it actually runs

```bash
open "src-tauri/target/release/bundle/macos/FenceyMD.app"
# confirm the process is alive AND a window exists (init can lag a few seconds):
osascript -e 'tell application "System Events" to tell process "fenceymd" to count windows'
```
"Builds" ≠ "works." Confirm the real app launches with a window before reporting
done. Native dialogs (Save-as-PDF, file rename/open) can't be auto-clicked from
here — those are called out for a manual check.

---

## Cross-platform builds

A Tauri app uses each OS's native webview + toolchain, so it **must be built on
the target OS** (no cross-compile from macOS).

- **macOS** → `npm run build:desktop` → `.app` + `.dmg` (done here).
- **Linux** → buildable from this Mac **via Docker** (a clean Ubuntu container,
  isolated from the host's `node_modules`/target):
  ```bash
  ./scripts/docker-build-linux.sh          # x86_64 (emulated on ARM Macs, ~15 min)
  ./scripts/docker-build-linux.sh arm64    # native, fast
  ```
  → `dist-linux/*.deb`, `*.rpm`, `*.AppImage`. (AppImage needs
  `APPIMAGE_EXTRACT_AND_RUN=1` in a container — the script sets it.)
- **Windows** → **cannot** be built from macOS (MSI needs WiX/MSVC; Windows
  containers need a Windows host). Options: run `scripts/build-windows.ps1` on a
  Windows machine, or GitHub Actions CI with a Windows runner. See `BUILD.md`.

During a run of rapid visual tweaks, the Linux build is **paused** — each
emulated build is ~15 min, so it's done once at the end when the look is settled,
not per change.

---

## Conventions that keep the loop honest

- **Re-run the suite after every change.** 40/40 or it's not done.
- **Look at screenshots** for visual work — light and dark.
- **One source of truth per concern:** state in `stores/`, styles in `app.css`
  via CSS variables (so light/dark "just works"), native ops behind `tauri.js`.
- **Keep `?test=1` representative** — when a feature needs a scenario (nested
  folders, diagrams), add it to the sample data so it's continuously testable.
- **Be honest about what wasn't verified** (native dialogs, on-target Win/Linux
  runs) instead of implying coverage.
