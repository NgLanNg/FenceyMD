# Inline editor

## Vision & DoD (5W1H)

**What.** A WYSIWYG markdown editor lives inside the app. The user clicks the pencil icon (or presses `e`) to swap from the read view to the edit view for the current chapter. The editor is built on Tiptap (a ProseMirror wrapper) and provides toolbar buttons for the common formatting (bold, italic, code, link, headings, list, code block, etc.) plus a "preview" tab that shows the rendered HTML.

**Why.** Reading and writing are the same activity at different times. The user shouldn't have to context-switch to VS Code or TextEdit to fix a typo. The editor should match the app's theme so the visual is continuous.

**Who.** Any user editing a chapter. The edit is bounded to the active folder — the user cannot edit files outside the open book.

**When.** Click the pencil icon in the Reader toolbar, or press `e`. The editor opens with the current chapter's content. ⌘S saves (with a "saved Ns ago" indicator). Cancel discards changes. Clicking a sibling chapter closes the editor (no half-save) and opens the new chapter in the reader.

**Where.** `src/components/Editor.svelte` is the editor host. Tiptap is loaded lazily.

**How (acceptance / DoD).**
- The editor opens with the current chapter content.
- Toolbar buttons for bold, italic, code (inline + block), link, headings (H1-H3), lists (bullet + ordered), blockquote, undo/redo.
- ⌘S saves; the saved file is the same markdown the user originally opened.
- "Saved Ns ago" indicator updates live.
- Cancel discards changes.
- The editor's preview tab shows the rendered HTML.
- Navigating to a different chapter closes the editor.
- Pasted images are handled by the image-paste flow.

---

## How we implemented it

**What.** A Svelte 5 component that hosts a Tiptap editor instance. The editor's content is the chapter's raw markdown. On save, we POST the content back to the Rust `write_file` command.

**Why this shape.** Tiptap (ProseMirror) gives us a robust WYSIWYG model with extensions for markdown serialization, code blocks, links, etc. We use `tiptap-markdown` for the markdown round-trip.

**When.** Mounted when the user clicks "Edit" or presses `e`. Unmounted on save, cancel, or chapter navigation.

**Where.**
- `src/components/Editor.svelte` — the host.
- `src/lib/tauri.js` — `writeFile` Tauri command wrapper.
- `src-tauri/src/main.rs` — `write_file` Tauri command.

**How (tech).**
- **Tiptap**: extensions for StarterKit, Link, CodeBlockLowlight, Image, Placeholder, plus a custom `CodeBlockEnterExtension` that overrides Enter to exit a code block (matches word-processor behavior, not Tiptap's default soft-newline).
- **Markdown round-trip**: `tiptap-markdown` v0.9.0. Limitations: toggling off a non-empty code block renders the result as inline `code` spans, not a new code block boundary. Documented workaround; not blocking.
- **Save**: `editor.storage.markdown.getMarkdown()` returns the current content. We POST to `write_file(folder, rel_path, content)`.
- **Debounced autosave** (v1.1 #14): on a per-file timer, the editor saves 2 seconds after the last keystroke. Indicator shows "Unsaved" / "Saving…" / "Saved Ns ago" based on the debounce state.
- **Cancel**: tracks a "saved snapshot" of the markdown on mount; on cancel, restores that snapshot. (Easier than rolling back Tiptap's history.)

**Gotchas.**
- A "shared debounce timer" bug in v1.0 caused data loss when the user navigated between chapters mid-save. The fix: a per-file timer that resets on every keystroke and gets cancelled on unmount.
- The "plain Enter exits code block" extension was a deliberate departure from Tiptap's default. Tiptap's default adds a soft newline (Shift+Enter for hard newline), which surprised prose authors. Our override calls `exitCode()` on Enter inside a code block.
- `tiptap-markdown`'s serializer is the most fragile part of the editor; we keep its version locked and re-test the round-trip on every upgrade.
