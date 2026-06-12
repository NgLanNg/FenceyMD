// Thin bridge over the Tauri API. Everything degrades gracefully in a plain
// browser (TAURI === false), so the app still runs for dev/preview.
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

// `isTauri` is injected by the Tauri runtime; absent in a normal browser.
export const TAURI =
  typeof window !== 'undefined' && (window.__TAURI_INTERNALS__ || window.isTauri);

export async function invoke(cmd, args) {
  if (!TAURI) throw new Error('not running in Tauri');
  return tauriInvoke(cmd, args);
}

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

/** Generate a PDF of a chapter via headless Chrome. Returns Uint8Array. */
export async function printPDF(title, chapterHtml, dark, vars) {
  return invoke('print_pdf', { title, chapterHtml, dark, vars });
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
 *  per-user setting from `md-reader-external-editor` localStorage; when
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
