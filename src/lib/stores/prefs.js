// ─────────────────────────────────────────────────────────────────────────────
// Persisted UI preferences store.
//
// SINGLE RESPONSIBILITY: own every user-facing reading/display preference
// (theme, reading font size, content width, nav state, view mode, code theme,
// font family, reopen-last, outline pane, onboarding) as Svelte writable stores,
// and keep each one mirrored to `localStorage` so it survives app restarts.
//
// HOW IT FITS: components import the individual stores and the small mutator
// helpers (`toggleTheme`, `adjustFontSize`, …) instead of touching
// `localStorage` or DOM attributes directly. The module-init subscribe block
// is the ONLY place that applies a pref to the DOM / persists it — components
// never write `data-*` attributes or `localStorage` themselves. The renderer
// collaborator is `shiki.js` (re-theming code blocks on a dark/light flip).
//
// KEY INVARIANTS / ASSUMPTIONS A MAINTAINER MUST KNOW:
//  - Every persisted key is `fenceymd-*`; `resetAllPrefs()` relies on that
//    prefix to wipe them all. New prefs MUST keep the prefix.
//  - The subscribe block runs at module-import time and fires once per store
//    immediately, so DOM attributes are seeded before first paint. Guarded by
//    `typeof document !== 'undefined'` so the module is import-safe in SSR/tests.
//  - All `localStorage` access goes through `lsGet`/`lsSet`, which swallow
//    errors: the Tauri webview can deny storage early in startup or in private
//    modes, and a thrown `localStorage` must never break pref application.
//  - `theme` is the one pref that follows the OS until the user makes an
//    explicit choice — see the OS-appearance blocks below.
// ─────────────────────────────────────────────────────────────────────────────
import { writable, get } from 'svelte/store';
import { rethemeForDarkMode } from '../renderers/shiki.js';

// Crash-safe localStorage accessors. `localStorage` can throw (disabled,
// quota, private mode, early Tauri webview) — we degrade to the default
// rather than letting a storage failure break pref load/persist.
const lsGet = (k, d) => { try { return localStorage.getItem(k) ?? d; } catch { return d; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch {} };

// ── v1 → v2 prefs migration: `md-reader-*` → `fenceymd-*` (rebrand) ──
//
// Runs once at module import, BEFORE the stores below hydrate from
// localStorage. For every old-prefixed key present, copy the value to
// the matching new-prefixed key (only if the new key is absent — that
// would mean a fresh install with no migration to do, or a user who
// already migrated), then delete the old key. Idempotent: a second
// run on an already-migrated localStorage is a no-op.
const PREFIX_OLD = 'md-reader-';
const PREFIX_NEW = 'fenceymd-';
function migratePrefsPrefix() {
  if (typeof localStorage === 'undefined') return;
  try {
    for (let i = localStorage.length - 1; i >= 0; i -= 1) {
      const k = localStorage.key(i);
      if (!k || !k.startsWith(PREFIX_OLD)) continue;
      const newKey = PREFIX_NEW + k.slice(PREFIX_OLD.length);
      if (localStorage.getItem(newKey) == null) {
        try { localStorage.setItem(newKey, localStorage.getItem(k) ?? ''); } catch {}
      }
      localStorage.removeItem(k);
    }
  } catch {}
}
migratePrefsPrefix();

// ── OS-appearance detection (CODE-REVIEW P1.1) ──────────────────────────────
// Seed the default theme from the system's prefers-color-scheme when the
// user has no stored preference. The manual toggle remains a sticky
// override — once `fenceymd-theme` is set in localStorage, we never
// overwrite it. Safe in browsers without matchMedia (older Safari, SSR,
// tests with the API stubbed): we fall through to 'light'.
//
// Browsers may update the preference live (System Settings → Dark Mode
// toggle). We subscribe to the change and re-apply ONLY when the user
// has not set an explicit override. Once they click the toggle, their
// choice wins and stays.
// Returns 'dark' | 'light' from the OS appearance setting, or 'light' as a
// safe fallback when matchMedia is unavailable/throws (SSR, old Safari, stubs).
function detectOsTheme() {
  if (typeof window === 'undefined' || !window.matchMedia) return 'light';
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } catch { return 'light'; }
}
// True iff the user has an explicit, persisted theme choice. Gate for OS-
// following: while false we mirror the OS; once true the user's choice wins.
function hasStoredTheme() {
  try { return localStorage.getItem('fenceymd-theme') != null; } catch { return false; }
}

// ── Defaults ────────────────────────────────────────────────────────────────
// Single source of truth for "what was the pref before the user touched it".
// `resetAllPrefs()` restores every store to its matching default and clears
// every `fenceymd-*` localStorage key — see below.
//
// `theme` is special: the default reads from the OS at startup, but only
// when the user hasn't stored an explicit choice. The single-quote dance
// keeps `PREFS_DEFAULTS.theme` as a fallback for `resetAllPrefs()` and
// tests that build the object in isolation.
export const PREFS_DEFAULTS = Object.freeze({
  theme: hasStoredTheme() ? lsGet('fenceymd-theme', 'light') : detectOsTheme(),
  fontSize: '',
  contentWidth: 680,
  navCollapsed: false,
  viewMode: 'read',
  codeTheme: 'github',   // 'github' (dual github-light/github-dark) | 'nord'
  fontFamily: 'serif',   // 'serif' | 'sans' | 'mono'
  reopenLast: true,
  // Outline pane (ROADMAP v1.1 #3, owned by the outline-pane task).
  // Disabled by default — shows as inline without proper sidebar CSS.
  outlineVisible: '0',
});

// Persisted prefs. Each store hydrates from `localStorage` (falling back to its
// default) and is mirrored back out by the subscribe block below; consumers
// read/write the store, never the key directly.
export const theme = writable(PREFS_DEFAULTS.theme);
export const fontSize = writable(lsGet('fenceymd-fontsize', PREFS_DEFAULTS.fontSize));
// `parseInt(...) || default` also rejects 0/NaN from a corrupted key, not just
// a missing one — width must be a usable positive number.
export const contentWidth = writable(parseInt(lsGet('fenceymd-content-width', String(PREFS_DEFAULTS.contentWidth)), 10) || PREFS_DEFAULTS.contentWidth);
export const navCollapsed = writable(lsGet('fenceymd-nav-collapsed', '0') === '1');
// Volatile (NOT persisted): transient UI state reset on every launch.
export const navOpen = writable(false); // mobile drawer
export const settingsOpen = writable(false); // settings modal
export const viewMode = writable(lsGet('fenceymd-view-mode', PREFS_DEFAULTS.viewMode));

// ROADMAP v1.1 #16 — onboarding hint. `true` once the user has
// dismissed the first-launch tooltip. Defaults to false so the
// hint shows on the very first session after install. (Owned by
// the reader-qol task — added here so the store has a home.)
export const onboarded = writable(lsGet('fenceymd-onboarded', '') === '1');

// ROADMAP v1.1 #8/#9/#10 — three new preferences hidden behind the Settings
// panel until this commit. All three follow the same shape: writable store +
// localStorage key + side effect on subscribe (data-attr / data-attr / nothing).
export const codeTheme = writable(lsGet('fenceymd-code-theme', PREFS_DEFAULTS.codeTheme));
export const fontFamily = writable(lsGet('fenceymd-font-family', PREFS_DEFAULTS.fontFamily));
export const reopenLast = writable(lsGet('fenceymd-reopen-last', '1') === '1');

// ROADMAP v1.1 #3 — auto-TOC outline pane. Tri-state persistence so the
// user can pin it open/closed or let it follow the viewport. Owned by
// the outline-pane task; we only own the store + helpers.
export const outlineVisible = writable(lsGet('fenceymd-outline-visible', PREFS_DEFAULTS.outlineVisible));
/** Resolve the tri-state outline pref to a concrete boolean.
 *  @param {string} storeValue current `outlineVisible` value: '1' (pinned
 *    open), '0' (pinned closed), or 'auto'/anything else (follow viewport).
 *  @param {number} [viewportWidth] override width in px; defaults to
 *    `window.innerWidth`, or 1024 when there's no window (SSR/tests).
 *  @returns {boolean} whether the outline pane should be shown. */
export function isOutlineVisible(storeValue, viewportWidth) {
  if (storeValue === '1') return true;
  if (storeValue === '0') return false;
  // 'auto' or anything else → resolve from viewport. 1100px is the
  // breakpoint the outline-pane task landed on for "wide enough".
  const w = viewportWidth ?? (typeof window !== 'undefined' ? window.innerWidth : 1024);
  return w >= 1100;
}
/** Flip the outline pane open/closed. From the 'auto' state the first toggle
 *  resolves the viewport default and stores its inverse, so the pane ends up
 *  in the opposite state to what the user currently sees (i.e. the toggle
 *  always visibly does something). Thereafter it's a plain '1'/'0' flip. */
export function toggleOutline() {
  outlineVisible.update((v) => {
    if (v === 'auto') {
      // First explicit toggle: invert the current viewport-resolved default.
      const w = typeof window !== 'undefined' ? window.innerWidth : 1024;
      return w >= 1100 ? '0' : '1';
    }
    return v === '1' ? '0' : '1';
  });
}

// Apply + persist prefs whenever they change.
//
// This is the single owner of pref→DOM and pref→localStorage side effects.
// Subscribing also fires synchronously on registration, so every `data-*`
// attribute and `--css-var` is seeded at module-import time before first
// paint. The `typeof document` guard keeps this no-op (and import-safe) in
// SSR/test contexts that have no DOM.
if (typeof document !== 'undefined') {
  theme.subscribe((v) => {
    document.documentElement.setAttribute('data-theme', v);
    lsSet('fenceymd-theme', v);
    // Re-render shiki code blocks with the new dark/light value so
    // they emit the dark/light inline color pair (instead of
    // sticking with the previously-rendered single color). No-op if
    // there are no shiki blocks on the page yet.
    rethemeForDarkMode(v === 'dark');
  });

  // OS-appearance live tracking (CODE-REVIEW P1.1). We follow the
  // system theme as long as the user hasn't made an explicit choice.
  // Once they click the toggle, the subscribe above writes to
  // localStorage and `hasStoredTheme()` becomes true, so we stop
  // reacting. The user can clear the storage key (via "Reset all
  // prefs") to opt back into OS-following.
  if (typeof window !== 'undefined' && window.matchMedia) {
    try {
      const mq = window.matchMedia('(prefers-color-scheme: dark)');
      const onChange = (e) => {
        if (hasStoredTheme()) return; // explicit user choice wins
        const next = e.matches ? 'dark' : 'light';
        if (get(theme) !== next) theme.set(next);
      };
      // `addEventListener` is the modern API; `addListener` is the
      // Safari < 14 fallback. Both no-op if the browser doesn't
      // support either.
      if (mq.addEventListener) mq.addEventListener('change', onChange);
      else if (mq.addListener) mq.addListener(onChange);
    } catch { /* matchMedia threw — fall back to the value we already wrote */ }
  }
  fontSize.subscribe((v) => {
    document.documentElement.setAttribute('data-font-size', v || '');
    lsSet('fenceymd-fontsize', v || '');
  });
  contentWidth.subscribe((w) => {
    const root = document.documentElement;
    root.style.setProperty('--content-w', w + 'px');
    const base = w / PREFS_DEFAULTS.contentWidth;
    root.style.setProperty('--home-w', Math.round(980 * base) + 'px');
    root.style.setProperty('--landing-w', Math.round(820 * base) + 'px');
    lsSet('fenceymd-content-width', String(w));
  });
  navCollapsed.subscribe((v) => {
    lsSet('fenceymd-nav-collapsed', v ? '1' : '0');
  });
  viewMode.subscribe((v) => {
    lsSet('fenceymd-view-mode', v);
  });
  // ROADMAP v1.1 #16 — onboarding.
  onboarded.subscribe((v) => {
    lsSet('fenceymd-onboarded', v ? '1' : '');
  });
  // New prefs (#8 code theme, #9 font family, #10 reopen-last).
  codeTheme.subscribe((v) => {
    document.documentElement.setAttribute('data-code-theme', v);
    lsSet('fenceymd-code-theme', v);
  });
  fontFamily.subscribe((v) => {
    document.documentElement.setAttribute('data-font-family', v);
    lsSet('fenceymd-font-family', v);
  });
  reopenLast.subscribe((v) => {
    lsSet('fenceymd-reopen-last', v ? '1' : '0');
  });
  // Outline pane (#3).
  outlineVisible.subscribe((v) => {
    lsSet('fenceymd-outline-visible', v);
  });
}

/** Flip between dark and light. Writes through the store, so the subscribe
 *  side effect persists the choice and `hasStoredTheme()` becomes true,
 *  ending OS-following. */
export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}

/** Set the view mode, coercing any non-'slide' input to 'read' so the store
 *  can only ever hold one of the two known values. */
export function setViewMode(mode) {
  viewMode.set(mode === 'slide' ? 'slide' : 'read');
}

/** Toggle between 'read' and 'slide' view modes. */
export function toggleViewMode() {
  viewMode.update((v) => (v === 'slide' ? 'read' : 'slide'));
}

/** Dismiss the first-launch onboarding hint. Idempotent. (Owned by
 *  the reader-qol task — we keep the helper here so the store has
 *  a writer in the same module.) */
export function dismissOnboarding() {
  // Write localStorage directly — the subscribe side-effect runs at module
  // init time and can race against Tauri webview localStorage availability.
  // Bypassing the store here ensures the value actually persists.
  try { localStorage.setItem('fenceymd-onboarded', '1'); } catch {}
  onboarded.set(true);
}

// Display labels for each `fontSize` token. The empty-string key is the
// medium/default size (no `data-font-size` modifier); keep it in sync with
// the `sizes` ladder in `adjustFontSize`.
export const fontSizeLabels = { sm: 'S', '': 'M', lg: 'L', xl: 'XL', xxl: '2X' };

/** Step the font size up (+1) or down (-1) the discrete ladder, clamped to the
 *  ends. `delta` is a step count, not pixels. Unknown current values fall back
 *  to the medium ('') rung via `indexOf`. */
export function adjustFontSize(delta) {
  const sizes = ['sm', '', 'lg', 'xl', 'xxl'];
  fontSize.update((cur) => {
    const i = sizes.indexOf(cur || '');
    return sizes[Math.max(0, Math.min(sizes.length - 1, i + delta))];
  });
}

/** Add `delta` px to the reading column width, clamped to [400, 1200] —
 *  the layout's supported range. */
export function adjustContentWidth(delta) {
  contentWidth.update((w) => Math.max(400, Math.min(1200, w + delta)));
}

// ── Reset all prefs (ROADMAP v1.1 #11) ────────────────────────────────────
//
// Wipes every `fenceymd-*` key from localStorage and restores every store in
// this file to its default. Designed as the "I'm stuck, get me out" button
// the user can hit without understanding which knob they turned.
//
// We intentionally only touch prefs in this file (theme, font, layout, code
// theme, reopen-last, outline, onboarding) — NOT library state, progress, or
// recents. Those have their own delete/clear paths; mixing them here would
// surprise the user.
export function resetAllPrefs() {
  if (typeof localStorage === 'undefined') return;
  // Wipe every fenceymd-* key. Catch any other custom keys that may have
  // been added by future code in this file.
  for (let i = localStorage.length - 1; i >= 0; i -= 1) {
    const k = localStorage.key(i);
    if (k && k.startsWith('fenceymd-')) localStorage.removeItem(k);
  }
  // Restore every store. Svelte 5's `writable.set` skips notification when
  // the new value is `safe_not_equal` to the old one — for primitives (the
  // defaults here are all primitives) that means "if the store was already
  // at the default, no subscribe fires and the localStorage key stays
  // absent". We don't want that: a reset must put every pref key back on
  // disk so a fresh `getItem()` after reset sees the default. So we also
  // write the defaults directly. (Cheap; this runs once per user click.)
  theme.set(PREFS_DEFAULTS.theme);
  fontSize.set(PREFS_DEFAULTS.fontSize);
  contentWidth.set(PREFS_DEFAULTS.contentWidth);
  navCollapsed.set(PREFS_DEFAULTS.navCollapsed);
  viewMode.set(PREFS_DEFAULTS.viewMode);
  codeTheme.set(PREFS_DEFAULTS.codeTheme);
  fontFamily.set(PREFS_DEFAULTS.fontFamily);
  reopenLast.set(PREFS_DEFAULTS.reopenLast);
  outlineVisible.set(PREFS_DEFAULTS.outlineVisible);
  // Belt-and-suspenders: write the canonical string forms the subscribers
  // would write, in case a `set` was a no-op (value already at default).
  lsSet('fenceymd-theme', PREFS_DEFAULTS.theme);
  lsSet('fenceymd-fontsize', PREFS_DEFAULTS.fontSize);
  lsSet('fenceymd-content-width', String(PREFS_DEFAULTS.contentWidth));
  lsSet('fenceymd-nav-collapsed', PREFS_DEFAULTS.navCollapsed ? '1' : '0');
  lsSet('fenceymd-view-mode', PREFS_DEFAULTS.viewMode);
  lsSet('fenceymd-code-theme', PREFS_DEFAULTS.codeTheme);
  lsSet('fenceymd-font-family', PREFS_DEFAULTS.fontFamily);
  lsSet('fenceymd-reopen-last', PREFS_DEFAULTS.reopenLast ? '1' : '0');
  lsSet('fenceymd-outline-visible', PREFS_DEFAULTS.outlineVisible);
}

// Re-export `get` so consumers don't have to import from svelte/store if
// they only need to read prefs synchronously from a click handler.
export { get };
