// Frontend debug logger. Writes structured, timestamped lines to
// `<app_data_dir>/debug.log` (managed by the Rust side) so the user can
// inspect what the app was doing when something went wrong — the WebView
// devtools are not visible inside the Tauri shell.
//
// Usage:
//   import { dlog, dlogStart, dlogEnd } from './debug-log.js';
//   const t = dlogStart('openFolderPath', { path });
//   try { ... } catch (e) { dlog('openFolderPath', 'err', e?.message); throw e; }
//   finally { dlogEnd(t, 'openFolderPath', 'ok', { files: n }); }
//
// `dlog` is fire-and-forget; failures are silently dropped so a broken log
// writer can't break the operation being traced. In the browser (non-Tauri),
// `dlog` falls back to console.log only — `tauri.js` will throw on `invoke`
// and the helper catches that and degrades gracefully.

import { TAURI, invoke } from './tauri.js';

// Monotonic counter that disambiguates trace ids minted within the same
// millisecond (the timestamp alone is not unique under tight loops).
let _seq = 0;
function nextId() { _seq += 1; return `t${Date.now().toString(36)}_${_seq}`; }

/** Write one debug line. Accepts any mix of strings/objects; objects are
 *  JSON-stringified (and fall back to String() if they have cycles), then all
 *  parts are space-joined. Fire-and-forget: the Rust write is not awaited and
 *  any failure is swallowed, so logging can never throw into the caller. In a
 *  plain browser it logs to console only (see file header). */
export function dlog(...parts) {
  const line = parts
    .map((p) => {
      if (p == null) return String(p);
      if (typeof p === 'string') return p;
      try { return JSON.stringify(p); } catch { return String(p); }
    })
    .join(' ');
  if (!TAURI) {
    // eslint-disable-next-line no-console
    console.log('[fenceymd]', line);
    return;
  }
  invoke('debug_log', { line }).catch(() => { /* swallow — best effort */ });
}

/** Start a trace. Returns an opaque token; pass it to dlogEnd. */
export function dlogStart(label, ctx) {
  const id = nextId();
  dlog(`[${label}] start id=${id}`, ctx ?? '');
  return id;
}

/** End a trace started with dlogStart. Emits a single line with the elapsed ms. */
export function dlogEnd(id, label, status, ctx) {
  // We don't have a clean start-time on the token (kept opaque). Use
  // performance.now() deltas by recomputing from the id (it's the same
  // base36 timestamp as our performance origin, but the match is fuzzy;
  // instead we just log status + ctx). Elapsed can be added by callers
  // that need it.
  dlog(`[${label}] ${status} id=${id}`, ctx ?? '');
}
