# Clipboard image paste

## Vision & DoD (5W1H)

**What.** When the user has an image on the clipboard (e.g. a screenshot, a copied file from Finder) and pastes it into the editor, the image is saved as a `.png` file in the `images/` directory next to the chapter, and a markdown image reference is inserted at the cursor.

**Why.** Authors writing documentation, tutorials, or visual notes need to embed images. The friction of "save to disk, copy path, type markdown" is too high; paste should "just work."

**Who.** Anyone writing a chapter that needs an image. Common for tutorials (screenshots), book reviews (cover art), documentation (diagrams).

**When.** The editor is open and has focus. The user pastes (⌘V or right-click → paste). If the clipboard contains image data, the paste triggers our handler instead of Tiptap's default.

**Where.** `src/components/Editor.svelte` handles the paste event. The save is a Tauri command (`save_clipboard_image`) that writes the bytes to disk and returns the relative path. The markdown reference is inserted at the cursor.

**How (acceptance / DoD).**
- Pasting an image from the clipboard saves it to `<chapter_dir>/images/<random>-<timestamp>.png`.
- A markdown image reference `![alt](./images/...)` is inserted at the cursor.
- The alt text defaults to the filename (no extension).
- The path is relative to the chapter, so the chapter is portable.
- Image dimensions are NOT auto-included (the user can add them later if they want).
- A failed save (permission denied, disk full) shows an error in the editor.

---

## How we implemented it

**What.** An `onPaste` handler in the Editor component that:
1. Detects image data in the clipboard event.
2. Converts the image to a base64-encoded byte array.
3. Calls `save_clipboard_image(folder, rel_path, bytes)` Rust command.
4. On success, inserts `![<alt>](<rel_path>)` at the cursor.

**Why this shape.** Tiptap has a default paste handler that strips images. We override it: if the clipboard contains an image, we route through Rust (which has filesystem access); if it contains text, Tiptap handles it normally.

**When.** Triggered on every paste event in the editor. The save is async (~50 ms for a small image).

**Where.**
- `src/components/Editor.svelte` — the paste handler.
- `src/lib/tauri.js` — `saveClipboardImage` wrapper.
- `src-tauri/src/main.rs` — `save_clipboard_image` Tauri command.

**How (tech).**
- **Detection**: `event.clipboardData.items` is iterated; we look for `kind: 'file'` and `type: 'image/...'`.
- **Conversion**: `await clipboardItem.getType('image/png').arrayBuffer()` gives us the raw bytes. We base64-encode for the JSON payload to Rust.
- **Save**: `save_clipboard_image` takes `(folder, rel_path, bytes)`. The Rust side:
  1. Resolves the path inside the folder (canonicalize-and-bounds-check).
  2. Creates the `images/` directory if missing.
  3. Writes the bytes.
  4. Returns the final relative path.
- **Filename**: `images/<8-char-random>-<timestamp>.png`. The random prefix avoids collisions; the timestamp helps when sorting.
- **Insert**: `editor.chain().focus().insertContent(`![${alt}](${relPath})`).run()`. Tiptap's markdown serializer turns this into the right format.

**Gotchas.**
- `dataTransfer` vs `clipboardData`: the paste event uses `clipboardData`, not `dataTransfer`. Easy to get wrong.
- The image bytes can be huge (a 4K screenshot is ~10 MB). We use base64 to avoid binary-in-JSON issues, which inflates the size by 33%. For huge images, we'd want a chunked upload or a temp file.
- macOS sometimes gives us a `image/tiff` instead of `image/png`. The renderer falls back to converting via canvas if the type isn't supported by `<img>` directly.
