---
title: Editing
---

# Three ways to change a chapter

The app is mostly a reader, but it's not read-only. You can:

- **Edit in place** with a WYSIWYG editor (Tiptap-backed)
- **Rename** the file via the toolbar
- **Bookmark** it so it appears in your recents highlights

All three are reachable from the toolbar icons on the right side of
every chapter.

---

# Edit in place

The **pencil icon** in the toolbar switches the chapter into edit
mode. What you see is a rich text view of the same Markdown. Bold
becomes **bold**, headings become headings, code blocks stay code
blocks, but the chrome is gone.

A small floating toolbar appears when you select text:

- **Bold**, *italic*, ~~strikethrough~~
- Headings, lists, blockquote
- Inline code, code block
- Link
- Undo / redo

When you're done, hit **Save** in the top-right of the editor. The
file on disk is rewritten, the chapter reloads, and your edits are
visible. The Save shortcut is `Ctrl/Cmd + S`; the editor also catches
`Esc` to cancel.

The editor is a thin layer over Markdown. It doesn't try to be
Word. Most things paste cleanly. For anything weird, edit the
`.md` file in any text editor; the app re-reads it the next time
you open the chapter.

---

# Rename

The **rename icon** in the toolbar (looks like a tag) lets you change
the file's name. The new name is sanitized: no slashes, no leading
dots, `.md` is auto-appended if you forget it.

A few rules:

- You can rename to a name that already exists in the same folder.
  the app refuses, with an inline error.
- The rename is atomic: the old file is moved, not copied and
  deleted.
- The chapter you renamed stays open under its new name; the URL
  in the address bar (and your bookmark, if any) updates.

Renaming is the right tool for "I called this `intro` and now it's
`chapter-01`." For mass renames, a terminal is faster.

---

# Bookmark

The **bookmark icon** in the toolbar toggles a per-chapter
bookmark. A bookmarked chapter is visually distinct in the sidebar
(red dot, red text) and shows up in the **Bookmarks** section of
the recents list.

Bookmarks are per-folder, not global. If you bookmark a chapter in
`~/Books/physics`, it doesn't follow you to `~/Books/biology`.

The keyboard shortcut is `Ctrl/Cmd + B`.

---

# What the app deliberately doesn't do

A short list, so you know what to reach for instead:

- **No file creation from the sidebar**: the file is whatever's on
- **No drag-and-drop reordering**: chapters are ordered by filename
- **No undo for rename**: the file is moved on disk. If you renamed
  by mistake, look in the trash (the OS moves it there).

The reasoning: a reader app that quietly rewrites your filesystem
in surprising ways is bad. The app does only what the toolbar
buttons say, and nothing else.
