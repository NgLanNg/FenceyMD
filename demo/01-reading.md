---
title: The Reading Experience
---

# A chapter, in detail

This is what a chapter looks like at full size. The app uses a
serif body font (Newsreader) for prose, a sans (Inter) for UI, and a
mono (JetBrains Mono) for code, same as you'd see in a printed book.

A small toolbar at the top of every chapter lets you change how it
reads:

- **A− / A+**: font size (S, M, L, XL, 2XL)
- **W− / W+**: content width (the column you read in)
- **☀**: toggle light/dark mode
- The rest is per-chapter tools (slide, edit, etc.)

Your settings are saved per-app, so the next chapter opens the same
way. Change is a click, not a setting.

---

# Body text

This paragraph is the default body style. Justified, line-height 1.8,
serif. Italics, **bold**, and `inline code` all live in this run.

You can resize, but the default is chosen to read comfortably on a
laptop screen for an hour without eye strain. If you read at night,
toggle the sun/moon icon in the toolbar. The dark theme is the
same palette, inverted.

A new paragraph has a bit of space above it; the eye uses the gap as
a hint to break attention. No magic. Typographic convention.

---

# Lists

An unordered list, for when order doesn't matter:

- Drag the **file** to the **folder**. That puts it in the right
  place without renaming.
- Click the **X** on a recents row. That one disappears from the
  recents list (the folder itself is untouched).
- Hold **Shift** while clicking chapters for multi-select, if
  we ever add batch operations.

A numbered list, for steps:

1. Open the folder picker.
2. Pick a directory that contains `.md` files.
3. Pick a chapter from the sidebar.

Nested lists work, too. They indent further with a small bullet shift:

- Outer bullet
  - Inner bullet
    - Deeper still
- Back to outer

That's most of what lists do.

---

# Headings and section breaks

A chapter can have one H1 (the title), any number of H2s (sections),
H3s (subsections), and so on. They show up in the article with a
descending size:

## This is an H2

### This is an H3

#### And an H4 (smaller, used sparingly)

The reading experience doesn't put a giant left rail of contents on
top of the chapter. The chapter is the point. The sidebar on the
*outside* is the only navigation chrome.

---

# Quotes and callouts

A blockquote is a long, indented passage. The reader renders it
italic with a red left border. The eye reads it as "someone's
voice, set apart":

> The best interface is the one you forget you're using. A book
> reader should feel like a book, not like a browser tab pretending
> to be a book.

A callout (the `:::note` style some markdown flavors support) is a
boxed aside:

:::note
**Tip:** if a chapter is too long, hit the slide icon in the toolbar
to break it into deck view. The `---` in your markdown becomes the
slide break.
:::

---

# Tables, footnotes, links

Tables render with a subtle border and a tinted header row. The
following table compares two reading modes:

| Mode | Best for | Slide break? |
| --- | --- | --- |
| Read | Long-form prose, code, anything | No |
| Slides | Presenting, sharing, single-screen | Yes (`---` or H1) |

Links are inline, no underline by default. Hover shows the URL in
the corner. Footnotes (the `[^1]` style) are supported but rarely
needed in a Markdown book.

That's basically all there is to a chapter. On to navigation.
