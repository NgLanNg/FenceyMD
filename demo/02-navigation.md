---
title: Navigation
---

# Three ways to move around

The app opens on a **Library** view (the home screen with your
recents and folder cards) and slides into the **Reader** the moment
you pick a chapter. From there, three independent paths take you
anywhere:

- The **sidebar** (left, always visible on wide screens)
- The **sibling arrows** at the bottom of every chapter
- **Keyboard shortcuts**

Each one does the same job, so use whichever feels right.

---

# The sidebar

The sidebar is your book's table of contents. It has four sections,
top to bottom:

1. **Library name**: click it to go back to the folder's home view.
2. **Find input**: type to filter chapters; clearing the box
   brings the full list back.
3. **Action row**: Home (back to library), Settings.
4. **Chapter list**: the actual TOC. Folders are collapsible groups
   with a small count badge; clicking a chapter scrolls the reader to
   it.

On narrow screens (or when you tap the collapse button) the sidebar
slides out as a drawer. Same content, different layout.

The sidebar reflects the **filesystem exactly**: rename a file, the
sidebar updates. Move it to a subfolder, it appears in that group.
There's no separate index to keep in sync.

---

# Sibling arrows

At the bottom of every chapter, you'll see a footer with two
buttons: *Previous* and *Next*, with the title of the chapter you'd
jump to.

These follow the sidebar's order, so:

- "Previous" doesn't mean "earlier in time". It means "above this
  one in the sidebar."
- If you're at the first chapter, *Previous* is hidden.
- If you're at the last, *Next* is hidden.

Siblings are a quick way to read cover-to-cover without using the
sidebar at all.

---

# Keyboard

Most actions have a shortcut. The full list:

| Key | Action |
| --- | --- |
| `←` / `→` | Previous / next chapter (when not typing) |
| `Esc` | Close modals, exit slide view, blur search |
| `Ctrl/Cmd + K` | Focus the sidebar filter |
| `Ctrl/Cmd + F` | Focus the in-chapter find |
| `Ctrl/Cmd + S` | Save the inline editor |
| `Ctrl/Cmd + B` | Toggle bookmark |
| `Ctrl/Cmd + .` | Toggle dark mode |
| `Ctrl/Cmd + ,` | Open settings |

Most of these are also reachable from the toolbar. The keyboard
shortcuts are for when you're reading and don't want to leave the
keyboard.

---

# Library and recents

When you have no folder open, you see the **Library** screen. It has
two regions:

- **Recents**: the folders you've opened recently, most recent
  first. Each row has the folder name, the absolute path (dimmed),
  and a tiny "X" to remove it from the list. Removing a recent
  doesn't delete the folder. It forgets the history.
- **Open Folder**: a single primary button that opens the native
  folder picker.

On a folder that's been opened before, the **Open last** shortcut
(from a recents row, or the keyboard) jumps straight in.

The recents list lives at `~/Library/Application Support/com.mdreader.app/state.json`,
a tiny JSON file. Move it to sync across machines; trash it to
reset.

---

# Search

Two kinds of search:

- **Sidebar filter** (Ctrl/Cmd+K): substring match on chapter
  titles and file paths. Pure client-side; instant.
- **In-chapter find** (Ctrl/Cmd+F): substring match inside the
  current chapter. Highlights all matches; Enter jumps to the next
  one.

Neither is fuzzy or regex. They are deliberately simple, so the
behavior is predictable.
