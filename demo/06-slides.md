---
title: Slide View
marp: true
theme: gaia
_class: lead
paginate: true
---

# Slide view

Any chapter that uses `---` to separate sections can switch into
**slide view**: a one-screen-at-a-time mode where the chapter
becomes a deck.

Hit the **slide icon** in the toolbar.

---

# Why

- A chapter on screen at a meeting, advancing with `→` or `Space`
- A draft you're reviewing one section at a time
- A long doc you want to skim by hitting Next thirty times

You write Markdown, you press a button, you get slides.

---

# Navigation

| Key | Action |
| --- | --- |
| `→` / `Space` | Next slide |
| `←` | Previous slide |
| `Home` / `End` | First / last |
| `Esc` | Exit slide view |

Mouse works too: the bar at the bottom has Prev / Next, and the
dots show where you are.

---

# Powered by Marp

The slide deck is rendered by **Marp**, the same engine used to
build technical presentations out of plain Markdown.

This means you get Marp directives for free:

```
---
theme: gaia
_class: lead
paginate: true
---
```

The first `---` block at the top of this chapter sets the theme
(gaia), the default class (lead for the title slide), and turns on
the page counter (you're seeing it in the bottom-right now).

---

# Themes

Marp ships with five built-in themes. Switch with the front-matter
`theme:` directive:

- `default`: clean, minimal
- `gaia`: warm, modern (this one)
- `uncover`: bold, dark, high contrast
- `invert`: black on white, sharp
- `lead`: large title, big numbers

Custom themes are a CSS file in your project. Drop one in and
reference it by name.

---

# Directives

Marp directives are HTML comments with a leading underscore:

```
<!-- _class: lead -->
<!-- _backgroundColor: white -->
<!-- _color: black -->
<!-- _header: '' -->
<!-- _footer: '' -->
```

They apply to the *current slide only*. Use them for one-off
adjustments: darken the background for a code demo, hide the page
number on the cover slide, add a section divider.

---

# What scales

Slides auto-shrink to fit the window. The shrink is bounded. At
some point the text gets too small to read, and the app keeps
the slide at its natural size with a bit of letterboxing.

The fix isn't fiddling with zoom. It's adding a `---` and splitting
the slide into two. Marp's pagination is your friend.

---

# Splitting: a horizontal rule

You don't need a special tool. A horizontal rule:

```markdown
## The first idea

Body text.

---

## The second idea

Body text.
```

In the reader, the `---` is invisible. In slide view, it becomes
a slide break. Marp picks up the rest: theme, layout, code
highlighting, the works.
