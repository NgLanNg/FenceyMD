---
title: Drawing with Excalidraw
---

# Scenes you can edit

Mermaid is for fixed pictures. Excalidraw is for scenes you want to
*draw*: hand-sketched shapes, whiteboard notes, diagrams with rough
edges. The chapter becomes a sketchbook. Hover the scene, click
**Edit**, draw, save. The shape you made is now part of the chapter.

Try it now: hover the scene below and click **Edit**.

```excalidraw
{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [
    {
      "id": "rect1",
      "type": "rectangle",
      "x": 100,
      "y": 100,
      "width": 240,
      "height": 80,
      "angle": 0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "#a5d8ff",
      "fillStyle": "solid",
      "strokeWidth": 2,
      "strokeStyle": "solid",
      "roughness": 0,
      "opacity": 100,
      "groupIds": [],
      "frameId": null,
      "roundness": {
        "type": 3
      },
      "seed": 12345,
      "version": 2,
      "versionNonce": 347187908,
      "isDeleted": false,
      "boundElements": [
        {
          "type": "text",
          "id": "text1"
        }
      ],
      "updated": 1780852550201,
      "link": null,
      "locked": false,
      "index": "a0"
    },
    {
      "id": "text1",
      "type": "text",
      "x": 130,
      "y": 130,
      "width": 180,
      "height": 25,
      "angle": 0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "solid",
      "strokeWidth": 1,
      "strokeStyle": "solid",
      "roughness": 0,
      "opacity": 100,
      "groupIds": [],
      "frameId": null,
      "roundness": null,
      "seed": 23456,
      "version": 2,
      "versionNonce": 145118076,
      "isDeleted": false,
      "boundElements": [],
      "updated": 1780852550201,
      "link": null,
      "locked": false,
      "text": "Click Edit to draw",
      "fontSize": 20,
      "fontFamily": 1,
      "textAlign": "center",
      "verticalAlign": "middle",
      "containerId": "rect1",
      "originalText": "Click Edit to draw",
      "lineHeight": 1.25,
      "index": "a1",
      "autoResize": true
    },
    {
      "id": "W1ZU1TkDpl5oWY3zoAEHf",
      "type": "rectangle",
      "x": 477.78125,
      "y": 189.4921875,
      "width": 161.75390625,
      "height": 133.68359375,
      "angle": 0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "solid",
      "strokeWidth": 2,
      "strokeStyle": "solid",
      "roughness": 1,
      "opacity": 100,
      "groupIds": [],
      "frameId": null,
      "index": "a2",
      "roundness": {
        "type": 3
      },
      "seed": 1620101572,
      "version": 17,
      "versionNonce": 1630034556,
      "isDeleted": true,
      "boundElements": [],
      "updated": 1780852607536,
      "link": null,
      "locked": false
    }
  ],
  "appState": {
    "gridSize": null,
    "viewBackgroundColor": "#ffffff"
  },
  "files": {}
}
```

---

# Editing a scene

Hover the scene above. A small **Edit** button appears in the
top-right corner. Click it and the full Excalidraw editor opens in
a panel: toolbar, library, the works.

Draw whatever you want. The editor is the full Excalidraw
toolkit: the same shapes, hand-drawn style, and library you'd
expect.

When you're done, hit **Save**. The scene is written back into the
chapter's `.md` file. The chapter updates, and the next time you
open it, the new scene is what you see.

If you want a portable copy, say to share with someone or open
in the standalone Excalidraw editor, hit **Save as file**. That
opens a save dialog and writes the scene as a standalone
`.excalidraw` file. The chapter isn't touched.

If you change your mind, hit **Cancel**. Nothing is written.

---

# How it lives in your file

The scene is a fenced code block in the Markdown:

````markdown
```excalidraw
{ "type": "excalidraw", "elements": [ ... ] }
```
````

The content is JSON, the same shape Excalidraw uses everywhere.
You can author it by hand, paste it from a tool, or let the editor
write it for you. The Markdown file stays plain text; the app
renders the JSON as a live scene.

When you save, only the JSON inside the fence changes. The rest of
the chapter, the surrounding prose, other chapters. All untouched.

---

# What it's good for

- **Whiteboard sketches**: the hand-drawn feel is the point. Quick
  notes, scratch work, things that look like they came off a
  whiteboard.
- **Architecture diagrams with rough edges**: when the diagram
  matters more than the precision. "Here's how the system hangs
  together" drawn in two minutes.
- **Inline annotations**: a single shape in the middle of a
  paragraph to break up a wall of text.
- **Multiple diagrams in one chapter**: drop a few scenes into a
  long chapter. Each one edits and saves independently.

For tightly structured diagrams (ER, sequence, state machines) reach
for Mermaid instead. For freeform spatial sketches, Excalidraw is
the right tool.

---

# A few things to know

- **Each scene is independent.** A chapter can have as many
  Excalidraw blocks as you like. They edit and save separately.
- **Saving overwrites.** There's no version history. If you want a
  snapshot before big changes, copy the JSON out into a `.txt` file
  first.
- **The scene is local.** Nothing is sent anywhere. No account, no
  cloud sync, no telemetry. The chapter stays on your disk.
- **The scene is the same in PDF export.** When you export the
  chapter, the Excalidraw scene renders as an inline SVG, so the
  PDF shows what the scene currently looks like. Not an older
  version, not an empty placeholder.
