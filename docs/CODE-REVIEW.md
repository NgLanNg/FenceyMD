# FenceyMD — Code & Feature Review

**Date:** 2026-06-13
**Scope:** full codebase — architecture, style, quality control, security, and a
Definition-of-Done (DoD) matrix for every feature.
**Build state at review:** `cargo test` 21 ✓ · `npm run build` ✓ · e2e **41/41** ✓ ·
`tauri build` produced `FenceyMD.app` + `.dmg`, launched with a live window.

---

## 1. Verdict

FenceyMD is a **production-ready v1** desktop Markdown book reader. The
foundation is senior-grade: a renderer registry instead of an if-chain, stores
split by concern, path-traversal defense on every file op, and a real two-layer
test suite (Rust unit + headless-Chrome e2e). After this review's hardening
pass, there are **no known security holes, data-loss/corruption paths, or
crashes**. Remaining work is polish and coverage (listed in §7), all tracked
rather than silently deferred.

**Quality bar met:** secure against untrusted content · no known data loss ·
responsive · regression-guarded · documented (why-comments + CHANGELOG).

---

## 2. Architecture

Three layers, ~8,300 LOC of source:

```
Svelte 5 UI ──IPC──> Rust/Tauri backend ──> native bundle (.app/.dmg)
 (13 components)      (main.rs, ~2,200 LOC)   (cross-platform)
```

- **UI** — `src/components/*.svelte` (Picker, Library, Sidebar+SidebarTree,
  Reader, Editor, Settings, CrossSearchPanel, OutlinePane, ExcalidrawViewer,
  SlideViewer, ZoomOverlay, TreeNode). Svelte 5 runes (`$state`/`$derived`/
  `$effect`/`$props`).
- **State** — `src/lib/stores/{state,prefs,progress,library,files}.js`,
  re-exported by the `stores.js` barrel so components import from one place.
- **Render pipeline** — `markdown.js` (showdown + `enhance()`) dispatches
  through `registry.js` to per-language renderers in `renderers/`, declared in
  `renderers/manifest.json`. The same manifest is read by Rust for the PDF path.
- **Cross-cutting** — `sanitize.js` (DOMPurify boundary), `anchors.js` (stable
  block IDs), `link-resolver.js` / `cross-search.js`, `diagram-export.js`,
  `tauri.js` (the IPC bridge), `slides.js`.
- **Backend** — `src-tauri/src/main.rs`: ~20 IPC commands (folder scan, progress,
  `write_file`/`rename_file`, clipboard image, `print_pdf` via headless Chrome,
  Excalidraw block splice, file watcher).

**Key invariants:** one source of truth per concern (state in stores, styles in
`app.css` via CSS vars + `data-theme`, native ops behind `tauri.js`); heavy deps
(mermaid, shiki, katex, excalidraw) lazy-loaded per renderer; the WKWebView
can't download / write-clipboard / run AppImage tools, so those route through
Rust.

---

## 3. Feature inventory & Definition-of-Done

**DoD legend** — Built: implemented & runs · Tested: automated coverage
(e2e / Rust unit) · Docs: README/CHANGELOG/why-comments · Verified: confirmed
this session (e2e-green / probe / built+launched).
✅ done · ⚠️ partial · ❌ missing · — n/a

### Rendering pipeline

| Feature | Built | Tested | Docs | Verified | Notes |
|---|:--:|:--:|:--:|:--:|---|
| Markdown body (showdown) | ✅ | ✅ e2e | ✅ | ✅ | Output **sanitized** (DOMPurify) before `{@html}` |
| Shiki code highlight + copy | ✅ | ✅ e2e (5 langs, copy btns) | ✅ | ✅ | dual github light/dark + nord |
| KaTeX math (inline + block) | ✅ | ✅ e2e + unit (`…embeds_katex`) | ✅ | ✅ | |
| Mermaid diagrams | ✅ | ✅ e2e | ✅ | ✅ | `securityLevel: 'strict'` |
| SVG fence | ✅ | ✅ e2e ×2 (render + namespace) | ✅ | ✅ | **sanitized** (SVG profile) |
| HTML fence | ✅ | ❌ no e2e | ✅ | ⚠️ | sanitized + probe-verified; **needs e2e** |
| Excalidraw (view/edit/save) | ✅ | ✅ e2e + 8 Rust unit | ✅ | ⚠️ | PDF→SVG path code-verified, not device-verified |
| CSV table + numeric align (#5) + row search (#6) | ✅ | ✅ e2e | ✅ | ✅ | full grid sort/paginate/export (#7) **deferred** |
| Slide view (Marp) | ✅ | ✅ e2e | ✅ | ✅ | |

### Reading

| Feature | Built | Tested | Docs | Verified | Notes |
|---|:--:|:--:|:--:|:--:|---|
| Folder picker + recents | ✅ | ✅ unit (scan, ipc dispatch) | ✅ | ✅ | |
| Sidebar nested folder tree | ✅ | ⚠️ indirect | ✅ | ✅ | diagnostic: solid; collapse survives rescans |
| Prev/next + arrow nav | ✅ | ✅ e2e | ✅ | ✅ | |
| In-chapter Find | ✅ | ✅ e2e | ✅ | ✅ | **fixed** stateful-regex skip |
| Cross-chapter search ⌘⇧F | ✅ | ✅ e2e ×4 | ✅ | ✅ | |
| Reading progress + bookmarks | ✅ | ✅ unit | ✅ | ✅ | **fixed** per-file debounce (data loss) |
| Reading time + word count | ✅ | ⚠️ indirect | ✅ | ✅ | |
| Auto-TOC outline pane | ✅ | ❌ no e2e | ⚠️ | ⚠️ | **needs e2e + screenshot** |
| Theme light/dark | ✅ | ✅ e2e | ✅ | ✅ | |
| Font size / family / width | ✅ | ✅ e2e | ✅ | ✅ | |
| Heading jump `gg`/`G` | ✅ | ❌ no e2e | ✅ | ⚠️ | |
| `?` cheatsheet | ✅ | ❌ no e2e | ✅ | ✅ | ⌘P binding **fixed** to match cheatsheet |
| Link-to-md navigation | ✅ | ✅ e2e | ✅ | ✅ | |
| Anchor infra (`data-md-anchor`) | ✅ | ✅ e2e | ✅ | ✅ | foundation for v2 AI vision |
| Zoom overlay | ✅ | ❌ no e2e | ⚠️ | ⚠️ | |
| Wikilinks `[[…]]` (#21) | ❌ | — | — | — | **not built** (optional, degrades gracefully) |

### Editing

| Feature | Built | Tested | Docs | Verified | Notes |
|---|:--:|:--:|:--:|:--:|---|
| Tiptap WYSIWYG + preview split | ✅ | ✅ e2e | ✅ | ✅ | |
| Autosave ⌘S + "saved" indicator | ✅ | ✅ e2e | ✅ | ✅ | |
| Find / replace | ✅ | ✅ e2e | ✅ | ✅ | **fixed** keystroke-jerk + replace-advance + focus |
| Clipboard image paste | ✅ | ✅ e2e + unit | ✅ | ✅ | |
| Paragraph tracking (#22) | ✅ | ✅ e2e | ✅ | ✅ | |
| Open in external editor | ✅ | ❌ native | ✅ | ✅ | **hardened** (control-char guard) |
| Editor closes on navigation | ✅ | ✅ e2e (new) | ✅ | ✅ | **fixed** wrong-file-write corruption |

### Export & output

| Feature | Built | Tested | Docs | Verified | Notes |
|---|:--:|:--:|:--:|:--:|---|
| PDF export (headless Chrome) | ✅ | ✅ unit ×3 | ✅ | ⚠️ | always-light **fixed**; native Save dialog not click-verified |
| Diagram copy / download as PNG | ✅ | ❌ native | ✅ | ⚠️ | routes through Rust (WKWebView limit) |
| App-window snapshot to clipboard | ✅ | ❌ native | ⚠️ | ⚠️ | |

### Settings & platform

| Feature | Built | Tested | Docs | Verified | Notes |
|---|:--:|:--:|:--:|:--:|---|
| Code theme picker | ✅ | ✅ e2e | ✅ | ✅ | |
| "Reopen last folder" toggle | ✅ | ✅ e2e ×2 | ✅ | ✅ | |
| Reset all prefs | ✅ | ✅ e2e | ✅ | ✅ | **fixed** to re-theme live code blocks |
| File rename | ✅ | ⚠️ unit (traversal) | ✅ | ⚠️ | native dialog not click-verified |
| Live file watching | ✅ | ❌ Tauri-only | ✅ | ✅ | **fixed** mutex-poison panic |
| macOS build (.app/.dmg) | ✅ | ✅ built + launched | ✅ | ✅ | |
| Linux build (Docker) | ⚠️ | — | ✅ scripts/BUILD.md | ❌ | not built this session |
| Windows build | ❌ from mac | — | ✅ documented | — | needs Windows host / CI |

---

## 3A. UI Definition of Done (checklist)

A UI-bearing feature is **UI-done** only when every box below is satisfied. This
is the bar applied in the surface scorecard that follows.

- [ ] **Light + dark theme** — legible contrast in both; themed via CSS vars, no
      hard-coded colors.
- [ ] **Responsive** — no horizontal overflow and usable controls from desktop
      down to ~480px (sidebar collapses to a drawer; toolbar sheds non-essentials).
- [ ] **Keyboard accessible** — visible focus ring (`:focus-visible`), sane tab
      order, `Esc` closes overlays, documented shortcuts work.
- [ ] **a11y semantics** — icon-only buttons have `aria-label`/`title`; dialogs
      use `role="dialog"` + `aria-modal`; decorative SVG is `aria-hidden`.
- [ ] **Interactive states** — hover, active, focus, and `disabled` are all
      styled (no dead-looking or indistinguishable controls).
- [ ] **Empty / loading / error states** — every async or list surface renders a
      defined state, never a blank or a silent failure.
- [ ] **Consistent design tokens** — spacing/typography/radius/color come from
      the `--space-*` / `--ink*` / `--surface*` / `--radius-*` scale.
- [ ] **No layout shift** — content doesn't jump as lazy renderers (mermaid,
      shiki, katex) hydrate; reserved space or in-place swap.
- [ ] **Adequate hit targets** — interactive controls ≥ ~28px.
- [ ] **Action feedback** — async actions (save, copy, PDF, snapshot) confirm via
      an indicator/toast.

### UI surface scorecard

Columns: 🌗 light/dark · 📐 responsive · ⌨️ keyboard+focus · ♿ a11y · 🖱 states ·
␀ empty/err/load · 🎨 tokens · 💬 feedback. ✅ met · ⚠️ partial/unverified · ❌ missing · — n/a

| Surface | 🌗 | 📐 | ⌨️ | ♿ | 🖱 | ␀ | 🎨 | 💬 |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| Picker (first run) | ✅ | ⚠️ | ✅ | ✅ | ✅ | ✅ `picker-error` | ✅ | ⚠️ |
| Library / Home | ✅ | ⚠️ | ✅ | ✅ | ✅ | ✅ root-files card | ✅ | — |
| Sidebar + tree | ✅ | ✅ drawer ≤768 | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ dark toggle |
| **Reader + toolbar** | ✅ | ✅ **verified** (container query) | ✅ | ✅ | ✅ disabled PDF | ✅ render-error / "not available" | ✅ | ✅ progress + pdf-toast |
| Editor | ✅ | ⚠️ full overlay | ✅ ⌘S/⌘H/Esc | ✅ | ✅ disabled save | ✅ `editor-error` | ✅ | ✅ "saved Ns ago" |
| Settings (modal) | ✅ | ⚠️ | ✅ Esc + focus-visible | ✅ | ✅ | — | ✅ | ✅ live re-theme |
| Overlays (cheatsheet / rename / zoom / ⌘⇧F) | ✅ | ⚠️ | ✅ Esc closes | ✅ dialog roles | ✅ | ✅ `xsearch-empty` / `rename-error` | ✅ | ✅ |
| Renderer blocks (code/diagram/csv/svg) | ✅ mermaid dark vars | ⚠️ tables scroll / diagrams scale | ✅ copy btns | ✅ tool aria | ✅ hover tools | ✅ `*-error` / `*-empty` / placeholder | ✅ | ✅ copy confirm |

**Cross-cutting UI strengths (verified):** global `*:focus-visible` ring,
`prefers-reduced-motion` honored, 40 `aria-label` + 25 `role` + 48 `title`
attributes, full design-token system, 11 distinct empty/error/loading state
classes, action feedback (save indicator, pdf-toast, copy confirmations).

**Cross-cutting UI gaps (the ⚠️/❌ to close):**
- ❌ **No OS-appearance auto-detect** — `theme` defaults to `'light'` and never
  reads `prefers-color-scheme`, so the app ignores the user's system dark mode
  on first launch. (Easy fix: seed the default from `matchMedia('(prefers-color-scheme: dark)')`.)
- ⚠️ **Responsive is e2e-asserted only for the reader toolbar.** Picker,
  Library, Settings, and overlays have media-query styling but no narrow-width
  assertion — needs a multi-width screenshot/probe pass.
- ⚠️ **Mermaid exported from dark mode prints dark-on-white** (theme consistency
  on the PDF surface).
- ⚠️ **Empty-state visual QA** — the classes exist for every surface, but not
  all have been screenshot-confirmed (e.g. an empty folder, a 0-result filter).

---

## 4. Quality control

**Two-layer automated suite, both green:**

- **Rust unit — 21 passing (+1 `#[ignore]` visual-dump):** folder scan,
  `write_file` traversal + parent-dir creation (2), IPC dispatch through the
  mock runtime, `locate_excalidraw_block` (7: simple, indent, tab, CRLF,
  trailing-info, non-ascii, multi-count), `update_excalidraw_block` round-trip,
  `build_print_html` (katex-embed, forced-light, CSV-wrap), manifest parse,
  `transform_for_pdf` (2), `save_clipboard_image` (write + traversal).
- **e2e — 41 passing (Puppeteer vs the real frontend on `?test=1`):** nav,
  every renderer (svg ×2, katex, shiki, mermaid, excalidraw, csv, slides),
  font/theme/search/bookmark, cross-chapter search (×4), settings (×4), anchors,
  image paste, autosave, find/replace, paragraph tracking, link-to-md, the
  responsive no-overflow assertion, and the new editor-closes-on-nav regression.

**Verification philosophy** (`DEVLOOP.md`): tight browser loop → re-run the
suite after every change → "builds ≠ works," so the bundle is launched and a
window confirmed before "done." Visual changes (print, dark mode, toolbar) are
checked with headless-Chrome screenshots. Native dialogs are explicitly called
out as *not* auto-verifiable rather than implied as covered.

**Coverage gaps (honest):** HTML fence, outline pane, zoom overlay, `gg`/`G`,
and all native-dialog paths (rename, file-open, PDF Save, snapshot) have no
automated test. Linux/Windows bundles are unbuilt here.

---

## 5. Code style & conventions

- **Svelte 5 runes only** — no legacy reactive `$:`. `$effect`s are written to
  track the minimum (a documented hazard: reading `editing` inside an effect
  caused a historic edit-loop; effects now read only what they must).
- **Single stylesheet** (`app.css`) themed via CSS custom properties +
  `data-theme`; layout uses a **container query** on the content pane so the
  reader toolbar responds to its own width, not the viewport.
- **Registry over branching** — adding a fence type = one file in `renderers/`
  + one manifest line; no edits to `markdown.js`.
- **Native ops behind `tauri.js`**, guarded by a `TAURI` check so the same
  frontend runs in a browser for testing.
- **Comments explain *why*, not *what*** — trust boundaries, race fixes, and
  non-obvious choices are documented inline (e.g. the per-file save debounce,
  the always-light PDF rationale, the sanitization boundary).
- **Rust:** path ops canonicalize-and-bounds-check; commands return
  `Result<_, String>` surfaced to the UI; no `unwrap()` on poisoned locks.

---

## 6. Security posture

- **Untrusted content is the threat model** — books are shared / downloaded /
  LLM-generated and render in a WebView with full IPC authority. A **DOMPurify
  boundary** (`sanitize.js`) sanitizes the chapter body, the `html` fence, and
  the `svg` fence; mermaid runs `strict`. Verified: `<script>`/`on*`/
  `javascript:` stripped, presentational markup preserved.
- **PDF pipeline** runs headless Chrome **sandboxed by default**, falling back
  to `--no-sandbox` only if Chrome refuses (constrained Linux/containers).
- **File ops** (`write_file`, `rename_file`, `save_clipboard_image`,
  `scan_folder`) reject path traversal via canonicalized ancestor checks;
  `write_file` also refuses a symlinked leaf escaping the root.
- **`open_in_external_editor`** spawns the user's chosen editor without a shell
  and rejects control characters (defense-in-depth on a user setting).

---

## 7. Known gaps & tech debt (tracked, not hidden)

**Coverage / docs**
- No e2e for: HTML fence, outline pane, zoom overlay, `gg`/`G`.
- `docs/screenshots/` is pre-v1.1 (ROADMAP #18 partial).

**Behavior polish**
- **Mermaid exported from dark mode prints dark-on-white** (Excalidraw is forced
  white; mermaid relight needs async re-render). Stylistic, not broken.
- Editor's external-change detection still uses a 500 ms timestamp heuristic
  (now bypassed for navigation; only used for file-watcher events).
- Preview re-renders twice on the first toggle-on (harmless).
- Renderer theme state lives in module-level singletons (no user-visible desync
  found in the diagnostic, but worth threading through `ctx`).

**Platform**
- Linux bundle unbuilt this run; Windows needs a Windows host / CI.
- Native dialogs (PDF Save, rename, file-open, snapshot) verified by code path,
  not a live click.

**Deferred features (ROADMAP v1.1/v2, intentional)**
- #7 CSV full data grid (sort/paginate/export) · #21 wikilinks · #24 highlights
  & notes · #25 tabs/multi-window · #26 AI anchor-based edit · #27 per-project
  config · #28 EPUB export.

---

## 8. How to re-verify

```bash
cd desktop-app
(cd src-tauri && cargo test)     # 21 ✓
npm run build                    # frontend ✓
npm run dev & node e2e-test.mjs  # 41/41 ✓
npm run build:desktop            # .app + .dmg
open "src-tauri/target/release/bundle/macos/FenceyMD.app"
```
