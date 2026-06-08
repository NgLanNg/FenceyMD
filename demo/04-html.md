---
title: Live HTML
---

# Live HTML

An `html` fence renders as **real DOM** in the chapter. Not
syntax-highlighted source. A live element you can see, hover,
and interact with.

This is the answer to the **markdown-vs-HTML argument**. When
an LLM hands you a `html` block (which they increasingly do for
dashboards, interactive docs, and rich formatting), you don't
need a separate tool or a conversion step. Paste it in an
`html` fence. It renders inline, in the same chapter as your
markdown.

```html
<div style="display:flex;gap:12px;align-items:center;font-family:sans-serif">
  <span style="width:36px;height:36px;border-radius:50%;background:#c25c4a"></span>
  <strong>HTML block</strong>
  <em style="color:#54545c">rendered, not shown</em>
</div>
```

A slightly richer example: a self-contained card with
typography, color, and a button.

```html
<div style="border:1px solid #e3e2e1;border-radius:8px;padding:16px;
            font-family:Inter,system-ui,sans-serif;max-width:340px">
  <div style="font-size:12px;letter-spacing:0.05em;color:#8a716e;
              text-transform:uppercase;font-weight:600">HTML fence</div>
  <div style="font-size:18px;font-weight:500;margin:4px 0 8px">
    Anything a blog post can do, the chapter can do.
  </div>
  <div style="display:flex;gap:8px">
    <button style="background:#83271f;color:#fff;border:0;
                   border-radius:4px;padding:6px 12px;font:inherit">
      Primary
    </button>
    <button style="background:#eeeeed;color:#1a1c1c;border:0;
                   border-radius:4px;padding:6px 12px;font:inherit">
      Secondary
    </button>
  </div>
</div>
```

> **Use it for:** embedded demos, custom widgets, dashboard
> tiles, anything you'd put in a blog post. The reader wires up
> the click handlers. The widget is interactive, not a static
> render.

The same rule applies to `svg`. See the next chapter.
