---
title: Exporting to PDF
---

# One click to a PDF

Every chapter has a **PDF icon** in the toolbar (looks like a
document with a down arrow). Click it, and a few seconds later the
native "Save As" dialog appears, defaulted to the chapter's name
plus `.pdf`.

The output is a real PDF. Vector text, not an image. You can copy
text out of it, search it, print it. A4 portrait, modest margins.

---

# What you get

The PDF mirrors the in-app reader:

- Same fonts, same colors, same headings
- Code blocks, blockquotes, callouts, tables. All rendered.
- Mermaid diagrams, converted to inline SVG so they print crisply
- Excalidraw scenes: same, converted to SVG via Excalidraw's
  own export API
- The chapter's H1 is the page header. Chapter metadata is omitted.
- The sidebar, toolbar, and progress bar are *not* in the output

If the chapter has dark mode toggled on, the PDF uses the dark
palette. If it's on light, light. The choice you make for reading
is the choice you get for the PDF.

---

# What you don't get

A handful of things that are interactive in the app but don't
translate to paper:

- **Excalidraw editing**: the static scene is rendered, not the
  editor. The PDF shows what the scene looks like now.
- **Hover-only controls**: the diagram toolbar (Copy, PNG, theme
  toggle) is hidden in the PDF. It's reader chrome, not content.
- **Search highlights**: if you used `Ctrl/Cmd+F` to find text in
  the chapter, the highlights don't carry over.
- **Slide view**: the PDF renders the *reader* view, not the deck.
  A chapter that splits into 30 slides in slide mode is 30 pages of
  continuous prose in the PDF.

These are deliberate. The PDF is for reading on paper or in another
PDF viewer, not for re-experiencing the app.

---

# A few tips

- **Give it a second**: a PDF can take 1-2 seconds because the app
  renders through a real headless browser. Anything you can see in
  the app, you can get in the PDF.
- **Long chapters**: a 10,000-word chapter is a 30-page PDF. The
  PDF has no per-page break optimization, so if you want shorter
  output, split the chapter with `---` or use slide view.
- **Light or dark**: the PDF matches the current theme. If you
  read in dark mode, the PDF is dark. Switch before exporting if
  you want the other.
