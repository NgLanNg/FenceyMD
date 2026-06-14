# Open in external editor

## Vision & DoD (5W1H)

**What.** A toolbar button in the Reader that opens the current chapter in the user's external editor (VS Code, Sublime, Vim, Emacs, whatever they have configured). Edits made in the external editor appear in FenceyMD's file watcher within 1-2 seconds.

**Why.** Some users live in their editor. For them, the in-app editor is friction — they want their toolchain, their snippets, their muscle memory. The app should support this without forcing it.

**Who.** Power users with a preferred editor.

**When.** Click the "open in external editor" button (or use the keyboard shortcut). The user's editor opens with the current chapter loaded.

**Where.** The button is in the Reader toolbar. The editor command is set in Settings (a single text field for the editor command, defaulting to the OS default: `code` for VS Code, `subl` for Sublime, etc.).

**How (acceptance / DoD).**
- Clicking the button opens the current chapter in the configured editor.
- If no editor is configured, we try a sensible default (macOS: `open -e`; Linux: `xdg-open`; Windows: the .md file association).
- The user can configure their preferred editor in Settings.
- The spawned process is fire-and-forget; the app doesn't wait for the editor to close.
- External edits appear in FenceyMD (via the file watcher) within 1-2 seconds.
- The editor command is validated (no shell metacharacters) to prevent injection.

---

## How we implemented it

**What.** A Rust Tauri command (`open_in_external_editor`) that:
1. Reads the user's editor preference from the persisted store.
2. Resolves the editor command to an executable + args.
3. Spawns the process (detached, no shell).
4. Returns immediately.

**Why this shape.** Spawning without a shell is the only safe way to handle user-configured commands. The Rust side has filesystem access and can resolve the editor's path.

**When.** Triggered by the toolbar button. The spawn is ~10 ms; the editor's window appears asynchronously.

**Where.**
- `src/components/Reader.svelte` — the toolbar button.
- `src/lib/tauri.js` — `openInExternalEditor` wrapper.
- `src/lib/stores/prefs.js` — `editorCommand` preference.
- `src-tauri/src/main.rs` — `open_in_external_editor` Tauri command.

**How (tech).**
- **Spawn**: `std::process::Command::new(editor).arg(path).spawn()`. The child is detached (no stdin/stdout/stderr wired to the parent). On Unix, no shell; the args are passed verbatim.
- **Validation**: the editor command is rejected if it contains control characters (defense-in-depth). Whitespace, paths, and quotes are allowed (the user might set `"code --wait"` or `"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"`).
- **Default detection**:
  - macOS: check for `code` in PATH, then `subl`, then `mate`, then fall back to `open -t <file>` (TextEdit).
  - Linux: `xdg-open <file>`.
  - Windows: `cmd /c start <file>`.
- **Live file watching**: the existing `notify`-based file watcher fires on the edit; the JS side re-reads the file (or invalidates the chapter cache) and the Reader shows the new content.

**Gotchas.**
- A user setting `code` when VS Code isn't installed gets a spawn error. We log the error and fall back to the OS default.
- `open_in_external_editor` runs without a shell, which means environment variables and shell redirects in the editor command don't work. The user has to use the editor's CLI directly.
- Some editors (`code`, `subl`) block the calling process until the file is closed. We always spawn without `wait()`, so the app doesn't hang.
- The file watcher is 500ms-debounced; an external edit appears in the Reader within 1-2 seconds of the save.
