# FenceyMD — Handoff brief

For an agent (or engineer) picking up this codebase cold. Read this, then
`docs/CODE-REVIEW.md` (full feature/DoD/UI-DoD audit), `AGENTS.md`, `DEVLOOP.md`.

**Project:** Tauri 2 (Rust) + Svelte 5 desktop Markdown book reader.
**Root:** `/Users/alan/WORKSPACE/Books/desktop-app`.

---

## State of the tree (read before touching git)

- A hardening pass landed many **source** changes that are **uncommitted**.
  Nothing is committed; **do not commit unless the user asks.**
- Build artifacts (`target/`, `dist/`, `node_modules/`) were **untracked** from
  git (`git rm --cached`) — they're gitignored. **Do not `git add` them back.**
- Last verified green: `cargo test` 21 ✓ · `npm run build` ✓ · e2e **41/41** ✓ ·
  `tauri build` → `.app`+`.dmg`, launched with a live window.

## What's already DONE — do not redo (see CODE-REVIEW §1–§6)

Security P0s (DOMPurify on body + html/svg fences, mermaid `strict`, PDF
sandbox-by-default, file-op traversal/symlink hardening); correctness P1s
(Find regex, per-file progress debounce, mutex-poison, write_file, enhance
error-surfacing, scan_folder cap); editor bugs (close-on-nav corruption fix +
e2e test, reset re-themes code, find/replace jerk+advance, ⌘P binding); the
responsive toolbar (container query + asserting e2e test); print (always-light,
CSV wrap, diagram scale); repo hygiene; stale-doc corrections; CHANGELOG.

## What's LEFT — prioritized, concrete (CODE-REVIEW §7 + §3A)

**P1 — real gaps worth closing:**
1. **OS dark-mode auto-detect.** `theme` defaults to `'light'` and ignores the
   system setting. Seed the default from
   `matchMedia('(prefers-color-scheme: dark)')` in `src/lib/stores/prefs.js`
   (`PREFS_DEFAULTS.theme` / the initial `theme` writable), keeping the manual
   toggle as an override. Add an e2e/probe for it.
2. **e2e coverage holes** (move ⚠️→✅): **HTML fence** and **outline pane** have
   no e2e. Add cases to `e2e-test.mjs`. (Outline also needs a docs screenshot.)
3. **Mermaid exported from dark mode prints dark-on-white.** In
   `Reader.svelte` `exportPDF()`, when `$theme === 'dark'`, re-render each
   `.mermaid` to a light theme into the captured HTML before sending to Rust
   (source is on `pre.dataset.mmdSource`; see `renderers/mermaid.js`). Do it on
   a clone, not the live DOM, to avoid a visible flash. Async/racy — be careful.
4. **Narrow-width responsive verification** for Picker / Library / Settings /
   overlays (only the reader toolbar is asserted today). Probe at 1280→400px
   with puppeteer-core, assert no page overflow, fix any.

**P2 — polish:** preview double-render on toggle (`Editor.svelte`); renderer
module-singletons (`_dark`/`_codeTheme` in `shiki.js`) → thread through `ctx`;
the editor's 500 ms external-change heuristic; empty-state visual QA; refresh
`docs/screenshots/`.

**Platform:** Linux bundle via `./scripts/docker-build-linux.sh`; Windows needs
a Windows host / CI (`scripts/build-windows.ps1`, see `BUILD.md`).

**Deferred features — do NOT start unless asked** (ROADMAP): #7 CSV full grid,
#21 wikilinks, #24–28 (highlights, tabs, AI anchor-edit, per-project config,
EPUB).

## How to work & verify (non-negotiable)

```bash
cd desktop-app
npm run build                         # frontend (~20s, catches Svelte errors)
(cd src-tauri && cargo test)          # 21 ✓ — run if you touched Rust
# restart dev BEFORE e2e if you edited a store/module (see landmine #1):
lsof -ti:1420 | xargs kill 2>/dev/null; npm run dev > /tmp/dev.log 2>&1 &
node e2e-test.mjs                     # MUST stay green (currently 41/41)
npm run build:desktop                 # bundle; "builds ≠ works" → launch it:
open "src-tauri/target/release/bundle/macos/FenceyMD.app"
osascript -e 'tell application "System Events" to tell process "fenceymd" to count windows'
```
- e2e uses **puppeteer-core** (explicit Chrome path), runs at **1200px**,
  `?test=1` loads sample data. Write throwaway probes the same way.
- Native dialogs (PDF Save, rename, file-open, snapshot) **cannot** be
  auto-clicked headless — verify by code path and ask the user for a manual try.

## Landmines (each one already cost time)

1. **Vite HMR module identity:** after editing a store/module, the autosave e2e
   test's dynamic `import('/src/lib/stores/progress.js')` can read a *stale*
   module instance → false failure (`lastSavedAt=0`). **Restart the dev server
   before e2e.** It is NOT a real regression.
2. **`$effect` edit-loop:** never read `editing` inside an effect that also
   writes it (historic infinite-reset bug). The close-on-nav effect reads only
   `path`.
3. There were **two `editing` flags** (local in `Reader.svelte`, store in
   `state.js`); reconciled via the path-change effect — don't reintroduce the
   desync.
4. **PDF appearance lives in Rust**, not CSS: `build_print_html` in
   `src-tauri/src/main.rs` renders the PDF via headless Chrome. `app.css`
   `@media print` only affects browser Cmd+P. PDFs are intentionally always
   light. (See memory `pdf-print-architecture`.)
5. **WKWebView** can't `<a download>`, write clipboard images, or run AppImage
   tools → those route through Rust commands via `src/lib/tauri.js`.
6. **Sanitization boundary** (`src/lib/sanitize.js`) is load-bearing security —
   keep body/html/svg sanitized; don't bypass for "convenience."
7. **DMG build:** detach mounted "FenceyMD" volumes (`hdiutil detach -force`)
   before `build:desktop` or `bundle_dmg.sh` fails.

## One-line task to give the agent

> Read `desktop-app/docs/HANDOFF.md` and `docs/CODE-REVIEW.md`, then close the
> P1 gaps in order (OS dark-mode default, HTML-fence + outline e2e, mermaid
> dark-PDF relight, narrow-width responsive verification). After every change
> run `npm run build` + `cargo test` + (restart dev) + `node e2e-test.mjs` and
> keep it green; don't commit; don't start deferred ROADMAP features.
