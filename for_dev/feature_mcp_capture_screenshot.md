# MCP tool: capture_screenshot

## Vision & DoD (5W1H)

**What.** The MCP `capture_screenshot` tool takes no arguments and returns a PNG of the current FenceyMD window. The PNG is base64-encoded in the JSON response. The agent can decode the base64 and pass the image bytes to a vision-capable LLM (Claude, GPT-4V, Gemini) to "see" what the user is looking at.

**Why.** Some agent workflows need visual context — the user says "this is broken, fix it" and the agent should be able to see the bug. Without `capture_screenshot`, the agent can only see the structured data (chapter content, selection, scroll); with it, the agent can see the layout, the rendering, the styling.

**Who.** Any agent that needs visual context. Common case: the user asks the agent to "look at what I'm seeing and tell me what's wrong."

**When.** On agent call. The capture + encode takes ~50 ms.

**Where.** `src-tauri/src/mcp.rs#tool_capture_screenshot`. Reuses the same `xcap` pipeline as the in-app ⌘⇧S shortcut.

**How (acceptance / DoD).**
- The tool returns `{ format: 'png', width, height, bytes, data_b64 }`.
- The PNG is the live FenceyMD window (not the whole screen, not a different app).
- The PNG is at native resolution (Retina 2×).
- The PNG is NOT pushed to the system clipboard (the in-app ⌘⇧S does that; this tool is for the agent only).
- A failed capture returns ERR_TOOL with a clear error message.

---

## How we implemented it

**What.** A Rust function that:
1. Uses `xcap::Window::all()` to find the FenceyMD window (by pid + app name).
2. Calls `capture_image()` to get an RGBA image.
3. Encodes the RGBA as PNG using the `image` crate (`ImageFormat::Png`).
4. Base64-encodes the PNG bytes with the `base64` crate.
5. Returns the result as JSON.

**Why this shape.** Reusing the in-app snapshot pipeline (xcap) means one less code path. Encoding to PNG (rather than sending raw RGBA) is the right format for vision LLMs. Base64 is the standard "binary in JSON" encoding.

**When.** On agent call. The capture is fast; the base64 encoding adds ~30% size overhead but is the cost of doing business in JSON.

**Where.**
- `src-tauri/src/mcp.rs#tool_capture_screenshot` — the tool.
- `src-tauri/src/mcp.rs#tool_definitions` — the schema (no input args).
- `xcap` crate — window enumeration + capture.
- `image` crate — PNG encoding.
- `base64` crate — base64 encoding.

**How (tech).**
- **Window finding**: same logic as the in-app snapshot — `pid == our_pid || app_name().to_lowercase().contains("md reader")`. The `is_minimized()` filter excludes minimized windows.
- **PNG encoding**: `img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)`. The `image` crate handles stride and format conversion correctly. Building PNG by hand is a footgun.
- **Base64 encoding**: `base64::engine::general_purpose::STANDARD.encode(&png_buf)`. The standard alphabet (not URL-safe).
- **Response shape**: `{ format: "png", width, height, bytes, data_b64 }`. The `bytes` is the PNG size; `data_b64` is the base64 string (length is `bytes * 4 / 3`).

**Gotchas.**
- Headless launches (`open /Applications/Foo.app` from a non-GUI shell) don't create a window, so the tool returns "FenceyMD window not found." This is a verification-side issue, not a code bug.
- macOS sometimes rejects large images at the clipboard — but we're not using the clipboard here, so no issue.
- The capture can be 5-10 MB for a 4K Retina display. The base64-encoded JSON payload is ~7-13 MB, which is large for an MCP response. Most clients handle this; if not, the agent can downscale before sending to the LLM.
- The PNG capture is *not* sanitized — the user's app content is captured as-is. This is fine for the user's own machine but would be a problem in a server-rendered scenario.
