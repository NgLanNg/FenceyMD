---
title: Live SVG
---

# Live SVG

A `svg` fence renders as **the graphic itself**. Not the
source, not syntax-highlighted text. The actual vector you'd
see in a browser or design tool.

The LLM ecosystem has settled on SVG as the cheapest graphical
output. Smaller than HTML, deterministic, infinitely zoomable.
With this reader, paste it straight into a chapter:

```svg
<svg viewBox="0 0 200 80" xmlns="http://www.w3.org/2000/svg">
  <rect x="2" y="2" width="196" height="76" rx="8" fill="#f0f1f0" stroke="#9a9aa0"/>
  <circle cx="40" cy="40" r="22" fill="#c25c4a"/>
  <text x="100" y="46" font-family="serif" font-size="20" fill="#242428">live svg</text>
</svg>
```

A second example: three shapes composed into a small diagram.

```svg
<svg viewBox="0 0 240 100" xmlns="http://www.w3.org/2000/svg" style="font-family:Inter,system-ui,sans-serif">
  <line x1="20" y1="50" x2="220" y2="50" stroke="#9a9aa0" stroke-width="1.5"/>
  <circle cx="40"  cy="50" r="6" fill="#83271f"/>
  <circle cx="120" cy="50" r="6" fill="#83271f"/>
  <circle cx="200" cy="50" r="6" fill="#83271f"/>
  <text x="40"  y="78" text-anchor="middle" font-size="11" fill="#56423f">draft</text>
  <text x="120" y="78" text-anchor="middle" font-size="11" fill="#56423f">review</text>
  <text x="200" y="78" text-anchor="middle" font-size="11" fill="#56423f">ship</text>
  <text x="40"  y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">1</text>
  <text x="120" y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">2</text>
  <text x="200" y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">3</text>
</svg>
```

> **Use it for:** inline diagrams, icons, sparklines, workflow
> sketches, anything an LLM produces as a vector. Pair with the
> `html` chapter (right before this one) for the full
> "markdown *and* HTML" experience.

For heavier diagrams, the reader has dedicated `mermaid` and
`excalidraw` fences. See those chapters next.
