# MD Reader — Project Status

**Snapshot date:** 2026-06-11
**Branch:** `feat/v1.1-wave-3`
**Plan source of truth:** `ROADMAP.md` (v1.1) + `PLAN.md` (v1.0 phases 1–5, all complete)

---

## TL;DR

- v1.0 (Phases 1–5) **shipped**. Foundation is solid: Tauri + Svelte reader, markdown pipeline, slides, PDF, settings, library, search, anchors, edit mode.
- v1.1 is **mostly shipped** in three waves. Wave 1+2 are committed to history. Wave 3 is committed and on the working branch.
- One open item is the **outline pane visual**: it was disabled in the last commit because of a CSS gap; the fix is staged on disk, not yet committed.

---

## v1.0 — foundation (Phases 1–5, all complete)

| Phase | Outcome |
|---|---|
| 1 | Tauri + Svelte skeleton, chapter model, library tree, reader view |
| 2 | Markdown pipeline (KaTeX, Shiki), renderer registry, theme system |
| 3 | Slide view (Marp), PDF export (Playwright), diagram tooling |
| 4 | Editor (Tiptap), Excalidraw inline, autosave, image handling |
| 5 | OSS readiness: README, SECURITY, THIRD-PARTY-LICENSES, CSV in core, build matrix |

All v1.0 acceptance gates held: `cargo test` green, `npm run build` green, `npm run build:desktop` green, e2e green, code-review sign-off.

---

## v1.1 — current cycle

### What shipped (committed to history)

Commit history tells the story:

- `523edbf` — CHANGELOG, SECURITY, Cargo.lock
- `5742123` — ROADMAP + code-block copy button on shiki blocks *(#1)*
- `1157050` — Wave 1+2: code-block copy, CSV polish, search, outline, anchors, settings, editor extras *(#1, #5, #6, #8, #9, #10, #11, #12, #13, #14, #16, #17, #18, #19)*
- `85ecfd0` — Three responsive/scale/crop bug fixes
- `e0926e5` — Markdown link-to-md navigation + paragraph-focus consumer *(#20, #22 reader-side)*
- `c3c702a` — Editor image paste, ⌘S autosave + indicator, find/replace, paragraph tracking *(#4, #14, #15, #22)*
- `2b0d54a` — 6 e2e tests for editor + link-to-md + responsive
- `f8c1825`, `eaa2d25`, `608594c` — Three iteration fixes to autosave/editor/onboarding
- `30dd054` — Removed v1.1 #16 onboarding hint (demo folder teaches users now)

### v1.1 item status

| # | Item | Size | Status |
|---|---|---|---|
| 1 | Code-block copy button | S | ✅ done |
| 2 | Cross-chapter search ⌘⇧F | M | ✅ done (in search/anchors commit) |
| 3 | Auto-TOC outline pane | S | ⚠️ **rebuilt today, not committed** |
| 4 | Clipboard image paste in editor | M | ✅ done |
| 5 | CSV numeric alignment | S | ✅ done |
| 6 | CSV row search | S | ✅ done |
| 7 | CSV full data grid (filter/sort/page/export) | M | ❓ unclear — review e2e coverage |
| 8 | Code theme picker in Settings | S | ✅ done |
| 9 | Font family choice in Settings | S | ✅ done |
| 10 | "Reopen last folder" toggle | S | ✅ done |
| 11 | "Reset all prefs" button | S | ✅ done |
| 12 | Reading time + word count | S | ✅ done |
| 13 | Heading jump shortcuts (`g g` / `G`) | S | ✅ done |
| 14 | Edit-mode ⌘S autosave + indicator | S | ✅ done |
| 15 | Find / replace in editor | M | ✅ done |
| 16 | Onboarding hint | S | ❌ **removed in `30dd054`** — demo folder replaced it |
| 17 | `?` keyboard cheatsheet | S | ✅ done |
| 18 | Refresh demo + screenshots | S | ⚠️ partial — `demo/` refreshed, screenshots are pre-v1.1 |
| 19 | "Open in external editor" | S | ✅ done |
| 20 | Markdown link-to-md navigation | M | ✅ done |
| 21 | Wikilinks `[[other-chapter]]` | S | ❓ unclear — needs verify |
| 22 | Edit-mode paragraph tracking | M | ✅ done |
| 23 | Anchor infrastructure | M | ✅ done (data-md-anchor plumbing in place) |

Items marked ❓ need a quick audit; the commit messages describe what's there but I haven't grep-verified each one today.

### Wave 3 specifically (current branch: `feat/v1.1-wave-3`)

The two "Wave 3" commits on this branch are the editor/link-to-md batch and its e2e coverage. They land #4, #14, #15, #20, #22, plus six regression tests. Clean working tree from those.

---

## Today's work — the outline pane fix

**Problem.** Commit `132417c` disabled the outline pane:
> "no CSS, was rendering inline and broken; default outlineVisible to '0'"

The component `src/components/OutlinePane.svelte` had class names (`outline-pane`, `outline-pane-header`, `outline-pane-list`, etc.) but **no matching rules in `src/app.css`**. Result: the panel rendered as raw unstyled content inline, breaking the layout. Quick fix landed: zero out the default and don't mount the component.

**What I did (2026-06-11, on `feat/v1.1-wave-3`).** Two files modified, **not yet committed**:

- `src/components/Reader.svelte` — added `outlineVisible` state, hover zone in template with `onmouseenter` / `onmouseleave` toggling it, conditional `{#if outlineVisible}` mount of `OutlinePane` inside the zone. `OutlinePane` import added.
- `src/app.css` — added the missing CSS:
  - `.outline-hover-zone` — fixed top-right at `top: 52px; right: 12px; z-index: 200`
  - `.outline-trigger` — opacity 0.3 → 1.0 on hover (150ms)
  - Full `.outline-pane` styles: absolute below the icon, 260px wide, 70vh max, rounded, shadow, padded, indented h2/h3, active-state highlight in `--tertiary`

**Behavior.** A faded list icon sits in the top-right of the reader. Hover it and the chapter outline panel slides in below the icon, anchored to it. Move off the panel and it closes. The original brand intent (calm, minimal, opt-in chrome) is preserved — no permanent sidebar.

**Not done.**
- No e2e test added for the outline yet (the disabled commit removed the only coverage there was).
- No screenshot for the docs gallery.
- No CHANGELOG entry.
- No commit. Files are modified, staged for your review.

---

## Decisions and open questions

### Decisions taken (just by working on it)

- **Hover trigger, not click.** The reference screenshots from this session show a hamburger-style icon in the top-right that only shows its panel on hover. That's the "calm" choice — chrome is invisible until the user reaches for it. Click-toggle would also work but feels heavier.
- **Fixed to viewport, not inside the article flow.** The hover zone uses `position: fixed; top: 52px; right: 12px;` so it stays put while the chapter scrolls. Anchored to the icon, not the right edge of the article column.
- **No backdrop / dim.** It's a panel, not a modal. The chapter stays interactive.
- **CSS variables, not hard-coded colors.** Reuses `--paper`, `--ink`, `--ink-muted`, `--tertiary`, `--surface-variant` so it follows the theme.

### Open questions for the user

1. **Commit the outline-pane fix today, or fold it into a v1.1 release commit later?** Working tree is currently two-file dirty.
2. **Item #18 (screenshots)** — the docs gallery at `docs/screenshots/` is from pre-v1.1. Should I regenerate them? The reference shots the user shared this session are a good target style: small, white card, minimal.
3. **Items #7 (CSV grid) and #21 (wikilinks)** — I marked them ❓. Want me to verify they shipped, or are they known-deferred?
4. **Item #16** was deliberately removed (demo teaches now). The ROADMAP table still lists it as "in". Should the ROADMAP be updated to reflect the removal, or kept as a historical record?
5. **The branch name `feat/v1.1-wave-3` is still around** — is this the cut for a v1.1 release PR, or are we going to keep stacking?

---

## What still needs doing before v1.1 ships

1. Decide on the five open questions above.
2. Add an e2e case for the outline pane (hover → headings visible → click → scroll).
3. Regenerate `docs/screenshots/` to match the v1.1 UI.
4. Update `ROADMAP.md` to mark shipped items and reflect the #16 removal.
5. Final `cargo test` / `npm run build` / `npm run build:desktop` / `e2e-test.mjs` green run on a clean tree.
6. CHANGELOG entry for v1.1.
7. Code-review sign-off, then merge `feat/v1.1-wave-3` to main and tag a release.

---

## File index for the curious

- `PLAN.md` — v1.0 plan, the foundation, all done.
- `ROADMAP.md` — v1.1 + v2 conversation. This is the plan for the current cycle.
- `CHANGELOG.md` — release notes, root level.
- `DEVLOOP.md` — dev loop + test count, kept in sync as e2e grows.
- `docs/screenshots/` — pre-v1.1 gallery, needs refresh.
- `src/components/OutlinePane.svelte` — the rebuilt outline (CSS is in `app.css` now).
- `src/components/Reader.svelte` — reader host, where the outline trigger is wired.
- `src/app.css` — single CSS file, all styles live here.
