# Find / replace (in editor)

## Vision & DoD (5W1H)

**What.** A find-and-replace dialog (⌘H) inside the editor. The user types a search term, sees live hit counts, steps through matches with Enter (next) or Shift+Enter (previous), and either replaces one match at a time or "Replace All."

**Why.** Long-form writing inevitably needs search-and-replace. The native browser find is limited to the displayed text, not the markdown source.

**Who.** Any user editing a chapter.

**When.** The editor is open; user presses ⌘H (or clicks the find/replace button in the toolbar).

**Where.** `src/components/Editor.svelte` hosts the dialog. Tiptap provides the search API.

**How (acceptance / DoD).**
- ⌘H opens the dialog.
- Typing a term shows the live match count ("1 of 5").
- Enter / Shift+Enter step through matches.
- Replace swaps the current match and advances to the next.
- Replace All swaps every match (with a confirmation if there are many).
- The dialog closes on Esc; focus returns to the editor.

---

## How we implemented it

**What.** A small modal dialog overlaid on the editor, with three inputs (find, replace-with) and four buttons (prev, next, replace, replace-all). Tiptap's prosemirror-search is overkill; we do our own string search on the markdown content.

**Why this shape.** Tiptap's search API is awkward and has well-known issues with regex state. For the common case (plain-text find/replace in markdown source), a 30-line implementation is enough.

**When.** Triggered on ⌘H or toolbar click.

**Where.**
- `src/components/Editor.svelte` — the dialog + handler.

**How (tech).**
- **Search**: linear scan of the editor's markdown content, with the cursor position as the "current" index.
- **Regex state**: a `lastIndex` is reset on every keystroke (the v1.0 bug was "the regex's lastIndex advanced across text nodes, silently skipping matches").
- **Replace**: we compute the new text by splicing in the replacement at the match index, then update the editor with the new markdown.
- **No replace-advance bug**: after a replace, the cursor advances past the replacement so the next search doesn't land on the just-replaced text. v1.0 had a "the same text gets replaced infinitely" bug; fixed.
- **Focus**: the dialog traps focus; Esc closes and returns focus to the editor's last position.

**Gotchas.**
- A `g` regex's `lastIndex` advances across text nodes in JavaScript. We reset it before each membership test.
- The "advance after replace" logic must handle the case where the replacement is shorter than the original (negative offset).
- For very long chapters (100k+ words), the linear scan is ~5 ms per search. Acceptable.
