# FenceyMD — Design Specification

> A calm, local-first desktop app for reading and editing folders of Markdown
> as if they were books. Built with Svelte 5 + Tauri 2. Editorial typography,
> a quiet warm palette, and a distraction-free reading surface.

---

## 1. Product Overview

**What it is.** FenceyMD turns any folder of `.md` files into a navigable book:
files become chapters, subfolders become parts/groups, and the app remembers
where you stopped reading. It renders rich Markdown (tables, code, mermaid
diagrams, inline SVG, math) and offers an inline WYSIWYG editor for quick edits.

**Who it's for.** Writers, students, and engineers who keep notes, drafts, or
documentation as Markdown on disk and want a focused reading experience instead
of a code editor.

**Platform.** macOS desktop (Tauri native shell). Degrades gracefully to a plain
browser for development, where filesystem features are disabled.

**Design north star.** *Print-quality reading, zero chrome.* Every screen should
feel like an editorial page — generous margins, serif body text, restrained
color — not a developer tool.

---

## 2. Design Principles

1. **Reading first.** Content is the interface. Controls recede until needed.
2. **Local & private.** Everything is on disk; no accounts, no network.
3. **Continuity.** The app reopens your last folder and last scroll position.
4. **Quiet color.** A warm paper-white base with a single rust accent. Color
   signals action, never decoration.
5. **Typographic hierarchy over borders.** Structure comes from type scale,
   weight, and whitespace — not boxes and rules.
6. **Graceful states.** Missing files, empty folders, and failed saves all have
   explicit, calm handling.

---

## 3. Design System

### 3.1 Color — Light (default)

| Token | Value | Use |
|---|---|---|
| `--surface` | `#faf9f8` | App background (warm paper white) |
| `--surface-container-lowest` | `#f0f1f0` | Cards, code blocks, raised panels |
| `--surface-container-low` | `#f3f4f3` | Inputs, subtle fills |
| `--surface-variant` | `#dfe3e2` | Hover states, dividers, chips |
| `--ink` | `#2f3333` | Primary text, headings |
| `--ink-secondary` | `#5c5f5f` | Body secondary, captions |
| `--ink-muted` | `#767878` | Metadata, placeholders, hints |
| `--tertiary` | `#a33e34` | **Accent** — links, active state, bookmarks, errors |
| `--tertiary-dim` | `rgba(163,62,52,0.08)` | Accent backgrounds (error banners, highlights) |

### 3.2 Color — Dark (`[data-theme="dark"]`)

| Token | Value |
|---|---|
| `--surface` | `#1a1a1c` |
| `--surface-container-lowest` | `#1c1c1e` |
| `--surface-variant` | `#2e2e30` |
| `--ink` | `#e8e8e6` |
| `--ink-secondary` | `#a8a8a6` |
| `--ink-muted` | `#7a7a78` |
| `--tertiary` | `#e06c5a` (brighter rust for contrast) |
| `--tertiary-dim` | `rgba(224,108,90,0.12)` |

Theme is toggled via the sidebar footer and persisted to `localStorage`
(`fenceymd-theme`). Applied as `data-theme` on `<html>`.

### 3.3 Typography

| Token | Stack | Role |
|---|---|---|
| `--font-serif` | `'Newsreader', Georgia, 'Times New Roman', serif` | Body copy, headings, reading surface, sidebar chapter titles |
| `--font-sans` | `'Inter', -apple-system, system-ui, sans-serif` | UI chrome: toolbars, metadata, buttons, labels |

- Base: `16px` root, body `line-height: 1.7`, antialiased.
- Newsreader carries the *editorial* feel; Inter keeps controls neutral.
- **Reader font-size levels** (user-adjustable, persisted): `S 0.85rem → M 1rem
  → L 1.15rem → XL 1.3rem → 2X 1.5rem`. Indicator chip sits between A− / A+.

### 3.4 Spacing scale

`--space-1` `.25rem` · `-2` `.5` · `-3` `.75` · `-4` `1` · `-5` `1.25` ·
`-6` `1.5` · `-8` `2` · `-10` `2.5` · `-12` `3` · `-16` `4rem`.

### 3.5 Radius

`--radius-sm` `2px` (chips, small controls) · `--radius-md` `4px` (cards,
buttons, inputs) · `--radius-lg` `8px` (large containers) ·
`--radius-xl` `12px` (sidebar shell, reader toolbar shell, sidebar inner
controls — filter, chapter rows, nav items, recents dropdown, find input).
Pills use `999px`.

### 3.6 Layout widths

`--content-w` `680px` (reading column) · `--landing-w` `820px` (book landing) ·
`--home-w` `980px` (library grid). Reading width is user-adjustable (W− / W+,
clamped 400–1200px); home/landing widths scale proportionally.

### 3.7 Motion

Calm and short. Sidebar width/transform `0.18–0.2s ease`; hovers `0.12–0.2s`.
Honors `prefers-reduced-motion` (transitions collapse to ~0).

---

## 4. App Shell & Navigation

```
┌──────────────┬───────────────────────────────────────┐
│              │                                       │
│   SIDEBAR    │            MAIN CONTENT               │
│  (264px)     │   (centered reading column /          │
│              │    library grid / editor overlay)     │
│              │                                       │
└──────────────┴───────────────────────────────────────┘
```

- **Sidebar** (`264px`, sticky, full height): folder switcher, chapter filter,
  bookmarks, grouped chapter tree, home + theme toggle in footer. Collapsible to
  zero width; a floating hamburger reopens it.
- **Main content**: routes between **Library** (home / group landing) and
  **Reader** (chapter). The **Editor** mounts as a full-screen fixed overlay
  (`z-index: 80`).
- **Mobile (≤768px)**: sidebar becomes an overlay drawer with a backdrop scrim;
  toggled by the hamburger. Driven by `.app-shell.mobile` / `.nav-open` classes.

Routing is store-driven: `route = { name: 'home' | 'group' | 'chapter', … }`.

---

## 5. Screens

### 5.1 Picker (first run / no folder open)

**Purpose:** entry point when no folder is loaded.

- Centered column: title **“FenceyMD”** (serif, 2rem), one-line description
  (serif, muted), primary **Open Folder** button (ink fill, sans).
- **Recent folders** list below: each row = 📁 icon · folder name · dim path · ✕
  remove button. Missing folders show “(missing)”, dimmed to 50%, non-clickable.
- Browser mode shows a hint: *folder access requires the desktop app.*
- Errors render in a rust error pill.

### 5.2 Library — Home

**Purpose:** overview of all groups in the open folder.

- Header: 📚 **Library** kicker, folder-name chip + “N files in M groups” meta.
- Large serif folder title + subtitle.
- **Book grid** of group cards (`--home-w` max). Each card: serif title +
  sans meta (“N files · sorted by chapter” when chapter numbering detected).
  Hover lifts to `--surface-variant`.
- A single-file group opens its chapter directly; otherwise opens the group
  landing. A “Root files” card points users to the sidebar for top-level files.

### 5.3 Library — Group Landing

**Purpose:** browse one group/part as a chapter tree.

- “← Library” back link.
- Header: 📁 icon tile + group title + file count.
- **Chapter list** rendered via recursive `TreeNode` (folders expand/collapse;
  chapters are clickable rows, indented `depth × 20px`).

### 5.4 Reader (chapter)

**Purpose:** the core reading surface.

Top → bottom:
1. **Reading-progress bar** — thin rust fill pinned to top, tracks scroll %.
2. **Back link** — “← {group or Library}”.
3. **Sibling nav** (top & bottom) — prev / next chapter with titles, a center
   “chapters” button, and an “n / total” counter.
4. **Reader tools row** (sans, small):
   - *Left:* in-chapter find (highlights matches, scrolls to first; Esc clears).
   - *Right:* font − / size chip / +, width − / +, theme toggle, Copy link,
     **PDF** export, Bookmark (filled rust when on), **Edit** (Tauri only).
5. **Chapter info** — file path (truncates) + word count.
6. **Rendered Markdown** (`chapter-markdown`, `--content-w`): headings, lists,
   tables, blockquotes; fenced code gets syntax highlighting + a Copy button;
   ` ```mermaid ` renders diagrams; ` ```svg ` renders inline SVG.

State: scroll position persists per file (debounced); ≥95% scrolled marks the
chapter “done” (✓ in sidebar). ← / → arrow keys move between chapters.

### 5.5 Editor (WYSIWYG overlay)

**Purpose:** Notion-like inline editing of a chapter, saving back to `.md`.

- Full-screen fixed overlay over the reader.
- **Toolbar** (sans): Bold, Italic, Strikethrough · H1 H2 H3 · bullet, numbered,
  blockquote, code block · undo / redo · **Preview** toggle · file-name chip ·
  Cancel · Save. Active formats highlight (`.is-active`).
- **Editing surface** (`notion-prose`, serif, centered `720px` column):
  content renders *formatted inline* — no raw Markdown visible. Markdown input
  shortcuts work natively (`# `, `- `, `> `, ` ``` `, etc.). Empty doc shows a
  “Start writing…” placeholder.
- **Preview mode** (⌘P): splits into editor (left) + live rendered preview
  (right, full Markdown pipeline incl. highlighting/mermaid). On narrow widths
  the split stacks vertically.
- **Save** (⌘S): serializes the document back to Markdown and writes to disk via
  Tauri; the in-memory index updates and the overlay closes. Failures surface as
  a rust error string in the toolbar — never a silent loss.

---

## 6. Components

| Component | Description | Key states |
|---|---|---|
| **Group card** | Library tile for a group | default / hover |
| **Sidebar chapter row** | Serif title + status glyph | default / hover / **active** (rust left border) / done ✓ / bookmarked ★ |
| **Sidebar group** | Collapsible section w/ caret + count | expanded / collapsed |
| **Tool button** (`tool-btn`) | Small sans icon/label control | default / hover / **bookmarked** (rust) / disabled (PDF busy) |
| **Editor tool** | Toolbar format button | default / hover / **is-active** |
| **Sibling nav button** | Prev/next chapter | present / placeholder (—) at ends |
| **Folder chip** (`folder-name-tag`) | Pill showing current folder/file name | static |
| **Reading-progress bar** | Scroll indicator | width = scroll % |
| **Search highlight** | `<mark>` on find matches | light: pale yellow / dark: amber |
| **Error pill / banner** | Rust-dim background, rust text | shown on failure |
| **Recents row** | Folder switcher entry | default / hover / missing (dimmed) |

---

## 7. Interaction & Behavior Notes

- **Persistence (localStorage):** theme, font-size level, content width, nav
  collapsed state, and (browser fallback) reading progress.
- **Persistence (Tauri backend):** recents list, per-folder reading progress &
  bookmarks, last-opened folder. Commands: `pick_folder`, `open_folder_path`,
  `open_last`, `get_recents`, `remove_recent`, `get_progress`, `save_progress`,
  `write_file`, `watch_folder`.
- **Live reload:** a filesystem watcher emits `library-changed`; the index
  rebuilds in place. If the open chapter is deleted on disk, the reader falls
  back to Home.
- **Keyboard:** ← / → chapter nav (when not typing); ⌘S save, ⌘P preview,
  ⌘B/⌘I formatting (in editor); Esc clears in-chapter search.
- **Empty / edge states:** no folder → Picker; missing recent → inline error +
  refresh; missing chapter content → “Content not available.” stub.

---

## 8. Accessibility & Quality Bar

- Maintain WCAG AA contrast in **both** themes (the dark accent is brightened to
  `#e06c5a` for this reason).
- All icon-only controls carry `aria-label` / `title`.
- Respect `prefers-reduced-motion`.
- Reading column stays within a comfortable measure (~`680px`) at default width.
- *Known polish item:* several clickable `div`s (recents rows, group cards,
  tree folders) use `role="button"` + `tabindex` but still need keyboard
  (Enter/Space) handlers for full keyboard parity.

---

## 9. Tech Mapping (for reviewers)

| Layer | Implementation |
|---|---|
| UI framework | Svelte 5 (runes: `$state`, `$derived`, `$effect`) |
| Native shell | Tauri 2 (Rust backend, `invoke`/`listen` bridge) |
| Markdown render | `showdown` + `highlight.js` + `mermaid` + inline SVG |
| WYSIWYG editor | Tiptap 3 + `tiptap-markdown` (Markdown ↔ doc) |
| PDF export | `html2pdf.js` (lazy-loaded) |
| Styling | Single `app.css` with CSS custom properties; no framework |

---

*This document describes the intended design. Where the build diverges, treat
this spec as the source of truth for review.*
