// Thin bridge over the Tauri API. Everything degrades gracefully in a plain
// browser (TAURI === false), so the app still runs for dev/preview.
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

// `isTauri` is injected by the Tauri runtime; absent in a normal browser.
export const TAURI =
  typeof window !== 'undefined' && (window.__TAURI_INTERNALS__ || window.isTauri);

/** Call a Rust command. Thin pass-through to the Tauri core `invoke`.
 *  Throws synchronously-rejected if not running under Tauri, so callers in
 *  browser/preview mode must either guard on `TAURI` first or catch (the
 *  debug-log helpers below rely on the throw to no-op). */
export async function invoke(cmd, args) {
  if (!TAURI) throw new Error('not running in Tauri');
  return tauriInvoke(cmd, args);
}

/** Subscribe to a Tauri event. Resolves to an unlisten function.
 *  In a plain browser there are no events, so this returns a no-op unlisten
 *  immediately — callers can always `(await listen(...))()` to clean up
 *  without branching on `TAURI`. */
export async function listen(event, handler) {
  if (!TAURI) return () => {};
  return tauriListen(event, handler);
}

// ── Binary bridges (the WKWebView has no download/clipboard-image support) ──

/** Save bytes to a user-chosen path via the native save dialog. `bytes`: Uint8Array. */
export async function saveBytes(defaultName, bytes) {
  return invoke('save_export', { defaultName, bytes: Array.from(bytes) });
}

/** Put a PNG image (bytes) on the native clipboard. `bytes`: Uint8Array. */
export async function copyImageBytes(bytes) {
  return invoke('copy_image', { bytes: Array.from(bytes) });
}

/** Generate a PDF of a chapter via headless Chrome. Returns Uint8Array.
 *  PDFs are always rendered light (see build_print_html), so no theme flag. */
export async function printPDF(title, chapterHtml, vars) {
  return invoke('print_pdf', { title, chapterHtml, vars });
}

/** Update the Nth ` ```excalidraw ` block in a markdown file with new JSON
 *  content. Re-reads the file each call so a second save after the first
 *  works even if the in-memory scene on the frontend is stale.
 *  Returns the new file content. */
export async function updateExcalidrawBlock(folder, relPath, blockIndex, newInner) {
  return invoke('update_excalidraw_block', { folder, relPath, blockIndex, newInner });
}

/** Save a pasted clipboard image (PNG bytes) into `<folder>/<relPath>`,
 *  creating the parent dir if missing. The Rust side rejects any `relPath`
 *  that would escape the folder (same canonicalize check as `write_file`).
 *  Returns the absolute path actually written. */
export async function saveClipboardImage(folder, relPath, bytes) {
  return invoke('save_clipboard_image', { folder, relPath, bytes: Array.from(bytes) });
}

/** Open `path` in the user's external editor. `editorOverride` is the
 *  per-user setting from `fenceymd-external-editor` localStorage; when
 *  null/empty the Rust side falls back to the OS default (`open -t` /
 *  `xdg-open` / `cmd /c start`). */
export async function openInExternalEditor(path, editorOverride) {
  return invoke('open_in_external_editor', { path, editorOverride: editorOverride || null });
}

// ── Debug log (file at <app_data_dir>/debug.log, written from JS + Rust) ──

/** Append a structured line to the user-visible debug log. See
 *  src/lib/debug-log.js for the higher-level `dlog()` helper that adds
 *  context. Returns nothing — the writer is best-effort and failures are
 *  swallowed so a broken log can't break the operation. */
export async function debugLog(line) {
  try { await invoke('debug_log', { line }); } catch { /* no-op */ }
}

/** Truncate the debug log. Used by the Settings panel's "Clear" button. */
export async function debugLogClear() {
  try { await invoke('debug_log_clear'); } catch { /* no-op */ }
}

/** Reveal the debug log in Finder/Explorer/xdg-open. */
export async function debugLogReveal() {
  try { await invoke('debug_log_reveal'); } catch { /* no-op */ }
}

// ── Window snapshot (screen capture → clipboard) ───────────────────────────

/** Snapshot the FenceyMD window to the system clipboard. Returns the
 *  dimensions of the captured image so the UI can show a confirmation
 *  toast ("Copied 1100 × 820 to clipboard") without re-encoding.
 *
 *  Today: full app layout. The Rust command is `snapshot_app_to_clipboard`.
 *  Future: pass `{ x, y, w, h }` to take a region-only capture; the
 *  JS side is structured so that addition is one extra optional arg.
 *
 *  Throws if the platform refuses (e.g. permission denied on a future
 *  macOS). The caller should surface a friendly message in that case
 *  rather than retry. */
export async function snapshotApp() {
  if (!TAURI) throw new Error('snapshot requires Tauri runtime');
  return invoke('snapshot_app_to_clipboard');
}

// ── MCP server (ROADMAP integration: AI agent control) ─────────────────────

/**
 * Set the active folder + metadata on the Rust MCP server. Called from
 * the JS `openScanResult` flow so the MCP `get_chapter_content` and
 * `get_book_toc` tools can answer without re-scanning. Idempotent;
 * the Rust side caches the latest payload.
 */
export async function mcpSetActiveFolder(root, files) {
  if (!TAURI) return;
  try { await invoke('mcp_set_active_folder', { root, files }); }
  catch (e) { /* best-effort; the MCP server is opportunistic */ }
}

/** Clear the active folder cache when the user closes the book. */
export async function mcpClearActiveFolder() {
  if (!TAURI) return;
  try { await invoke('mcp_clear_active_folder'); } catch { /* no-op */ }
}

/**
 * Push the live view state (route, scroll, selection, current chapter)
 * to the Rust MCP server. The MCP tools read this snapshot to answer
 * `get_current_chapter` and `get_selected_text` without round-tripping
 * back to the WebView. We push on navigation + on scroll throttle
 * + on selection change.
 */
export async function mcpUpdateViewState(view) {
  if (!TAURI) return;
  // Lazy-import dlog to avoid a circular dep with debug-log.js →
  // tauri.js. The cost is a one-time import on the first call.
  try {
    const { dlog } = await import('./debug-log.js');
    dlog('[mcp] pushing view state', view);
    await invoke('mcp_update_view_state', { view });
  } catch (e) {
    try {
      const { dlog } = await import('./debug-log.js');
      dlog('[mcp] view state push failed', e?.message || String(e));
    } catch { /* even dlog failed — silent */ }
  }
}

/**
 * Diagnostic for the Settings panel: returns the port-file path + the
 * cached session_context (if any agent has called `open_file` with
 * one). Shape: `{ port_file: string, session_context: object|null }`.
 */
export async function mcpStatus() {
  if (!TAURI) return null;
  try { return await invoke('mcp_status'); } catch { return null; }
}

// ── Agent registration (Settings → AI agent control) ───────────────────────

/**
 * List every supported AI agent with its detected/registered state. Each
 * entry: `{ id, display_name, detected, registered, config_path }`. Returns
 * an empty list outside Tauri so the Settings section just renders nothing.
 */
export async function agentsDetect() {
  if (!TAURI) return [];
  try { return await invoke('agents_detect'); } catch { return []; }
}

/**
 * Register FenceyMD's MCP server into the agent `id`'s own config file
 * (idempotent, non-destructive). Throws with a human-readable message on
 * failure so the Settings UI can surface it inline.
 */
export async function agentsRegister(id) {
  if (!TAURI) throw new Error('agent registration requires the desktop app');
  return invoke('agents_register', { id });
}

/** Remove FenceyMD from agent `id`'s config. No-op if it wasn't registered. */
export async function agentsUnregister(id) {
  if (!TAURI) throw new Error('agent registration requires the desktop app');
  return invoke('agents_unregister', { id });
}
