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
