# MD Reader — Roadmap (v1.1 and beyond)

`PLAN.md` is the v1.0 plan (Phases 1–5, complete). This file is
what's next. The v1.1 cut below is the work that's queued for the
next release cycle; "v2" is the conversation after that.

## The brand line (read first)

> *MD Reader is a calm, local, native desktop app for reading and
> lightly editing long-form Markdown books. No account, no network,
> no telemetry, no page-level interactivity.*

Every item in this roadmap is in-scope iff it makes a chapter better
to read, or better to lightly edit. Items that pull the user out of
the chapter into a tool are out. "Per-block interactivity" is a
useful nuance (see CSV below): the chapter owns the block, so a
small inspector is part of the chapter; a multi-block page-level
grid is a different app.

The bar for "is this in or out" is always: *does the user come back
to the prose, or does the prose become a launcher for the tool?*

---

## v1.1 — queued for next release

~3–4 weeks of work, all builds on the Phase 1–5 foundation. No new
arch decisions; no plugin model; no data model changes.

| # | Item | Size | Notes |
|---|---|---|---|
| 1 | Code-block copy button | S | Shiki-rendered blocks skip the registry's copy button. Add one (~20 lines + e2e). |
| 2 | Cross-chapter search ⌘⇧F | M | lunr/minisearch at scan time. ⌘⇧F panel, Enter jumps. README already promises the shortcut. |
| 3 | Auto-TOC outline pane | S | Right-side drawer with each chapter's H1+H2. Click to jump. Data is already in the scan. |
| 4 | Clipboard image paste in editor | M | Paste PNG → save to `images/` next to chapter → insert `![alt](./images/…)`. Rust `write_file` exists. |
| 5 | CSV numeric alignment (right-align + thousands sep) | S | CSS-only via a class on numeric columns. Heuristic: ≥80% cells parse as number. |
| 6 | CSV row search | S | `<input>` at top of each CSV block, filters rows on type. |
| 7 | **CSV full data grid** (filter / sort / paginate / export) | M | **Use case**: sample data attached to a chapter needs *inspection*, not just reading. Build in-house, ~150–250 lines, headless over our own CSV renderer. The four affordances, no more: search, click-to-sort, page at >50 rows, export to CSV/MD. |
| 8 | Code theme picker in Settings | S | github-light / github-dark + one extra (one-dark or nord). |
| 9 | Font family choice in Settings | S | Serif (default) / Sans / Mono. |
| 10 | "Reopen last folder on launch" toggle | S | Currently `openLast` is unconditional in Tauri. Make it a setting. |
| 11 | "Reset all prefs" button | S | 5 lines. Saves you when someone gets stuck in a weird state. |
| 12 | Reading time + word count in chapter header | S | "5 min · 1.2k words". |
| 13 | Heading jump shortcuts (`g g` / `G`) | S | + small progress dot in sidebar. |
| 14 | Edit-mode ⌘S autosave + "saved 2s ago" indicator | S | Rust `write_file` is there; wire the shortcut. |
| 15 | Find / replace in editor | M | Tiptap supports it; toolbar buttons + ⌘H. |
| 16 | Onboarding hint on first launch | S | Dismissable tooltip near the sidebar the first time. |
| 17 | `?` keyboard cheatsheet | S | Tiny modal listing all shortcuts. |
| 18 | Refresh `demo/` book + `docs/screenshots/` | S | Demo is the first thing power users open. |
| 19 | "Open file in external editor" button | S | One Rust command + toolbar button. Matches "hand back to an LLM". |
| 20 | **Markdown link-to-md navigation** | M | Click `[text](other.md)` → route to that chapter. Hook on `<a href="*.md">` clicks, resolve relative to current chapter, call the route store. The "book" model falls apart without this. |
| 21 | **Wikilinks `[[other-chapter]]`** | S | Obsidian-style shorthand for #20. Pre-process during markdown render, convert to normal links, let #20 handle navigation. Optional — degrades gracefully if not present. |
| 22 | **Edit-mode paragraph tracking** | M | When in edit mode, the outline pane (or sidebar) highlights the paragraph the cursor is on. Tiptap's `onUpdate` gives selection pos; map to paragraph index. |
| 23 | **Anchor infrastructure** (stable block IDs everywhere) | M | Give every renderable block a stable `data-md-anchor="para-12"`, `data-md-anchor="code-7"`, `data-md-anchor="mermaid-3:nodeA"`, `data-md-anchor="eq-2"`, etc. Required for #22, #20, and the v2 AI vision. Pure plumbing; the user-facing payoff is "I can reference any block by a stable address". |

### Sequencing for v1.1

**Wave 1 (Week 1)** — small, visible, low-risk:
#1, #5, #6, #8, #9, #10, #11, #12, #13, #14, #16, #17, #18, #19.

**Wave 2 (Week 2)** — the foundational trio:
#20 (link-to-md), #22 (paragraph tracking), #23 (anchor infra).
These three pay for themselves across the whole rest of the roadmap.

**Wave 3 (Week 3–4)** — bigger lifts:
#2 (cross-chapter search), #4 (image paste), #7 (CSV grid), #15 (find/replace),
#21 (wikilinks), #3 (outline pane).

### v1.1 acceptance

Every item holds the same gates as Phase 1–5:
- `cargo test` green
- `npm run build` green
- `npm run build:desktop` green
- `e2e-test.mjs` green, with new cases for each item
- code-review sign-off
- committed to a working branch (never `main`)

---

## v2 — the conversation after v1.1

These are the bigger lifts. They depend on the v1.1 foundation.

| # | Item | Size | Notes |
|---|---|---|---|
| 24 | Highlights + notes per paragraph | L | 4 colors, side gutter, per-folder store. |
| 25 | Tabs / multi-window / multi-book | L | Different feel, but unlocks "draft vs published" workflows. |
| 26 | **AI integration: anchor-based edit** | XL | The future vision. The user points at a block (paragraph, mermaid node, math equation, CSV cell); the anchor is captured; an agent (CLI / IPC / external) returns a surgical diff for that anchor; the editor applies it. Foundation = #22 + #23. The agent surface is "a CLI / IPC endpoint that takes (file, anchor, new_content) and applies the diff". The agent itself can be local subprocess, remote HTTP, or a future in-app chat — the architecture doesn't care. |
| 27 | Per-project config (`.mdreader.toml`) | M | Per-book overrides (theme, fonts, renderers enabled). |
| 28 | EPUB export | L | The "other format" the parking lot called out. |

### How #26 is shaped by #22 + #23

The AI future is **anchor-shaped**, not free-text-shaped. The user's
mental model is "this part was wrong, fix it" — not "rewrite the
whole chapter". Concretely:

- A user clicks a paragraph, hits ⌘E ("explain / edit with AI"),
  the active anchor is captured.
- A user clicks a node inside a mermaid diagram, the anchor is
  `mermaid-3:nodeA` — the agent returns a partial mermaid source
  patch for that node, not a full diagram.
- A user selects text in a code block, the anchor is
  `code-7:lines-3-7`, the agent returns a code diff.
- The Rust side exposes one new command:
  `apply_block_edit(file, anchor, new_inner, format_hint)`.
  It's the same shape as the existing `update_excalidraw_block`
  command, generalized.

The reason we need #22 + #23 in v1.1 is that without stable anchors
on every block, the AI integration has no primitive to consume. So
the "future" feature is in fact three quarters "now" (plumbing) and
one quarter "later" (the agent UX).

---

## Out — by user call

| Item | Reason |
|---|---|
| Cloud sync | Offline-first brand. |
| Telemetry | Same. |
| Code execution REPL blocks | Different app. |
| Page-level multi-block grid (Notion-style) | The brand line. Per-block interactivity (CSV grid) is in; the moment you go page-level you've shipped a different app. |
| Tabs in the v1.x line | Multi-window does this better; defer. |

---

## Sequencing rules of thumb

1. **Foundation before features.** #22 + #23 before any AI surface.
2. **Small before big.** The Wave 1 / Wave 2 / Wave 3 split is
   ordered by risk and dependencies, not user-fame.
3. **Visible before invisible.** The "refresh demo + screenshots"
   item (#18) is the cheapest way to make every other shipped item
   feel real.
4. **Don't pile on tangential ideas.** If a feature doesn't
   strengthen "calm, local, native, long-form reading", it doesn't
   belong on this roadmap.
