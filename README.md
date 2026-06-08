# MD Reader

> A native desktop Markdown book reader.
> Smaller than Obsidian. Lighter than a notebook.
> Markdown *and* HTML. No arguing.

![Library](docs/screenshots/library.png)

---

## Why MD Reader

LLMs hand you one of two things: a wall of markdown, or a sea of HTML.
Most readers pick one and stop. MD Reader picks **neither**. Render
markdown, HTML, and embedded widgets side by side, in a single native
desktop app.

- **Native, not Electron.** ~5 MB DMG vs ~250 MB for Obsidian.
- **Folder = a book.** Pick a directory. Get a sidebar with chapters,
  recents, scroll position, and bookmarks that survive restarts.
- **Slides, diagrams, drawings, PDFs.** Rendered inline, editable
  in place.
- **Offline-first.** No account, no telemetry, no network. Everything
  on disk.

This is the reader for long-form LLM output: drafts, research
notes, books. And the kind you can hand back to an LLM to keep
editing.

---

## At a glance

| ![Reader](docs/screenshots/reader.png) | ![Slides](docs/screenshots/slides.png) | ![Dark](docs/screenshots/library-dark.png) |
|:---:|:---:|:---:|
| **Read** with editorial typography | **Slides** via Marp fences | **Dark** theme, same shape |

| ![HTML](docs/screenshots/html.png) | ![SVG](docs/screenshots/svg.png) | ![Mermaid](docs/screenshots/mermaid.png) |
|:---:|:---:|:---:|
| **HTML** fences → real DOM | **SVG** fences → the graphic | **Mermaid** diagrams, live |

---

## Features

- **Folder = a book.** Chapters in a sidebar, subfolders become
  chapter groups, recents dropdown keeps your last 10 folders.
- **Reading progress + bookmarks.** Per-file scroll position and
  bookmarks survive reloads and restarts.
- **Markdown + HTML rendered side by side.** No conversion, no
  plugin, no format war.
- **Slide view.** Marp-fenced chapters become a navigable deck.
  Arrow keys step through, Esc exits.
- **Inline Mermaid.** Flowcharts, sequence, ER. Live in the
  chapter.
- **Inline Excalidraw.** Open a drawing canvas, save back to the
  same `.md` file. Editor runtime state stripped; only the
  document survives.
- **PDF export.** Headless Chrome renders the live chapter (with
  SVGs inlined) to vector-text PDF.
- **Edit & save.** Swap a chapter into raw-markdown mode, write
  back to disk. Writes are restricted to the folder you opened.
- **Live folder watcher.** External edits show up in the library
  immediately, scroll position preserved.
- **Light/dark, font size, content width, sidebar collapse.**
  Remembered across sessions.
- **Keyboard.** ← / → between chapters. ⌘F in-chapter search.
  ⌘P PDF export. e to edit. Esc to clear.

---

## Install

### macOS

Grab `MD.Reader_1.0.0_aarch64.dmg` from the
[Releases page](https://github.com/NgLanNg/mdreader/releases/latest),
open the DMG, drag **MD Reader** into Applications.

> **Note:** the release DMG is not code-signed. On first launch
> macOS warns it's from an unidentified developer. Right-click the
> app → **Open**, or run
> `xattr -dr com.apple.quarantine "/Applications/MD Reader.app"`.

For an **x86_64 (Intel) Mac**, **Windows**, or **Linux** build,
see [CONTRIBUTING.md](CONTRIBUTING.md) for the build path on each OS.

### Build from source

Requires **Node.js 18+** and **Rust** (https://rustup.rs).

```bash
git clone https://github.com/NgLanNg/mdreader.git
cd mdreader
npm install
npm run dev          # browser preview at http://localhost:1420
                     # append ?test=1 for the bundled tour book
```

For a desktop bundle:

```bash
npm run build:desktop   # → .app + .dmg (mac) / .msi + .exe (win) / .deb + .AppImage (linux)
```

---

## Use

1. **Pick a folder.** ⌘O, or pick from the recents dropdown in
   the sidebar.
2. **Read.** Click a chapter in the sidebar. ← / → moves between
   chapters. ⌘F searches within a chapter.
3. **Slides.** Click the deck icon in a chapter toolbar. Arrow keys
   step through, Esc exits.
4. **Edit & save.** Click the pencil icon (or press e). Save
   writes back to the file. The folder watcher refreshes on
   external edits too.
5. **Diagrams.** Mermaid blocks render inline. The Excalidraw
   icon opens a drawing canvas; save writes the diagram back into
   the chapter.
6. **PDF export.** The PDF icon (or ⌘P). Headless Chrome renders
   the current chapter with vector text.
7. **Preferences.** Gear icon (top-right). Theme, font, width,
   sidebar collapse. All remembered.

The bundled [`demo/`](demo/) is a 10-chapter self-introducing
tour of every feature. Load it with `?test=1`.

---

## Roadmap

**WIP: AI-agent integration.** MD Reader is being designed to be
*AI-native*. The same Rust commands the UI uses will be exposed
to an agent, with the same guarantees: writes stay inside the
folder, Excalidraw saves stay clean, scroll position preserved.
Hand the agent the folder and let it work.

---

## License

MIT. See [LICENSE](LICENSE). Copyright (c) 2026 Alan Nguyen.
