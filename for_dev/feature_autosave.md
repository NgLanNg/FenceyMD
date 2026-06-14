# Autosave

## Vision & DoD (5W1H)

**What.** When the editor is open, the user's edits are saved automatically after 2 seconds of inactivity. A small indicator next to the toolbar shows the current state: "Unsaved" (changes pending), "Saving…" (in-flight), "Saved Ns ago" (last save time). The user can also press ⌘S to force-save immediately.

**Why.** Long-form writing is interrupted by saves. The user shouldn't have to think about "did I save?" — the editor should handle it. The indicator exists for confidence: the user wants to know their work is on disk without having to verify.

**Who.** Any user editing a chapter.

**When.** The editor is open and the user has typed something. The autosave timer starts on the last keystroke; if no new keystroke arrives within 2 seconds, the save fires. Manual ⌘S bypasses the timer.

**Where.** `src/components/Editor.svelte` owns the autosave state. The save itself is `writeFile` (Tauri command). The indicator is in the editor toolbar.

**How (acceptance / DoD).**
- "Unsaved" appears within 1 keystroke after editing.
- "Saving…" appears during the in-flight save (typically < 50 ms).
- "Saved Ns ago" appears after the save completes, with the seconds counting up.
- ⌘S forces an immediate save (skips the 2-second wait).
- If the user types during a save, the "saving" indicator doesn't disappear prematurely — only when the in-flight save completes.
- If the user navigates away mid-debounce, the pending save is flushed.
- A failed save (e.g. permission denied) shows an error in the indicator and re-tries on the next keystroke.

---

## How we implemented it

**What.** A per-file debounced timer in the Editor component. State machine: `unsaved → debouncing → saving → saved → unsaved → ...`. The actual save is a Tauri `write_file` call.

**Why this shape.** A per-file timer (vs a shared one) means each editor instance has its own state. Cancelling and re-arming on every keystroke is the standard debounce pattern. The state machine is explicit so the indicator can render the right text.

**When.** Triggered on every Tiptap `update` event. Fires 2 seconds after the last update.

**Where.**
- `src/components/Editor.svelte` — the debounce + state machine.
- `src/lib/stores/progress.js` — none for autosave (autosave is the editor's responsibility, not the progress map).
- `src/lib/tauri.js` — `writeFile` wrapper.
- `src-tauri/src/main.rs` — `write_file` command.

**How (tech).**
- **Timer**: a `setTimeout` that's cleared and reset on every update. On fire, calls `writeFile` and updates the indicator.
- **State**: `let saveState = 'saved'` with values `'saved' | 'unsaved' | 'saving' | 'error'`. Renders different indicator texts.
- **Time display**: `setInterval(1000)` ticks a `now` value; the indicator computes `now - lastSavedAt` and shows "Saved 1s ago", "Saved 1m ago", etc.
- **Flush on unmount**: the component's `onDestroy` checks for a pending save and flushes it. Prevents lost edits on rapid navigation.
- **Force save**: ⌘S calls a `saveNow()` that bypasses the debounce.

**Gotchas.**
- The v1.0 version had a *shared* debounce timer across all editors. When the user edited chapter A, then navigated to chapter B before the save fired, the save would target B (because the timer was the only state). This was a real data-loss bug — fixed in v1.1.
- The Tiptap `update` event fires on every selection change too (not just typing). We debounce selection-only changes (we don't need to save on every cursor move), but the indicator flickers. Fixed by listening to `transaction` and checking `docChanged`.
