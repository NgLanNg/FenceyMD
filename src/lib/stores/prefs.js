// Persisted UI preferences: theme, reading font size, content width, nav state.
import { writable, get } from 'svelte/store';
import { rethemeForDarkMode } from '../renderers/shiki.js';

const lsGet = (k, d) => { try { return localStorage.getItem(k) ?? d; } catch { return d; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch {} };

// ── Defaults ────────────────────────────────────────────────────────────────
// Single source of truth for "what was the pref before the user touched it".
// `resetAllPrefs()` restores every store to its matching default and clears
// every `md-reader-*` localStorage key — see below.
export const PREFS_DEFAULTS = Object.freeze({
  theme: 'light',
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

export const theme = writable(lsGet('md-reader-theme', PREFS_DEFAULTS.theme));
export const fontSize = writable(lsGet('md-reader-fontsize', PREFS_DEFAULTS.fontSize));
export const contentWidth = writable(parseInt(lsGet('md-reader-content-width', String(PREFS_DEFAULTS.contentWidth)), 10) || PREFS_DEFAULTS.contentWidth);
export const navCollapsed = writable(lsGet('md-reader-nav-collapsed', '0') === '1');
export const navOpen = writable(false); // mobile drawer
export const settingsOpen = writable(false); // settings modal
export const viewMode = writable(lsGet('md-reader-view-mode', PREFS_DEFAULTS.viewMode));

// ROADMAP v1.1 #16 — onboarding hint. `true` once the user has
// dismissed the first-launch tooltip. Defaults to false so the
// hint shows on the very first session after install. (Owned by
// the reader-qol task — added here so the store has a home.)
export const onboarded = writable(lsGet('md-reader-onboarded', '') === '1');

// ROADMAP v1.1 #8/#9/#10 — three new preferences hidden behind the Settings
// panel until this commit. All three follow the same shape: writable store +
// localStorage key + side effect on subscribe (data-attr / data-attr / nothing).
export const codeTheme = writable(lsGet('md-reader-code-theme', PREFS_DEFAULTS.codeTheme));
export const fontFamily = writable(lsGet('md-reader-font-family', PREFS_DEFAULTS.fontFamily));
export const reopenLast = writable(lsGet('md-reader-reopen-last', '1') === '1');

// ROADMAP v1.1 #3 — auto-TOC outline pane. Tri-state persistence so the
// user can pin it open/closed or let it follow the viewport. Owned by
// the outline-pane task; we only own the store + helpers.
export const outlineVisible = writable(lsGet('md-reader-outline-visible', PREFS_DEFAULTS.outlineVisible));
export function isOutlineVisible(storeValue, viewportWidth) {
  if (storeValue === '1') return true;
  if (storeValue === '0') return false;
  // 'auto' or anything else → resolve from viewport. 1100px is the
  // breakpoint the outline-pane task landed on for "wide enough".
  const w = viewportWidth ?? (typeof window !== 'undefined' ? window.innerWidth : 1024);
  return w >= 1100;
}
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
if (typeof document !== 'undefined') {
  theme.subscribe((v) => {
    document.documentElement.setAttribute('data-theme', v);
    lsSet('md-reader-theme', v);
    // Re-render shiki code blocks with the new dark/light value so
    // they emit the dark/light inline color pair (instead of
    // sticking with the previously-rendered single color). No-op if
    // there are no shiki blocks on the page yet.
    rethemeForDarkMode(v === 'dark');
  });
  fontSize.subscribe((v) => {
    document.documentElement.setAttribute('data-font-size', v || '');
    lsSet('md-reader-fontsize', v || '');
  });
  contentWidth.subscribe((w) => {
    const root = document.documentElement;
    root.style.setProperty('--content-w', w + 'px');
    const base = w / PREFS_DEFAULTS.contentWidth;
    root.style.setProperty('--home-w', Math.round(980 * base) + 'px');
    root.style.setProperty('--landing-w', Math.round(820 * base) + 'px');
    lsSet('md-reader-content-width', String(w));
  });
  navCollapsed.subscribe((v) => {
    lsSet('md-reader-nav-collapsed', v ? '1' : '0');
  });
  viewMode.subscribe((v) => {
    lsSet('md-reader-view-mode', v);
  });
  // ROADMAP v1.1 #16 — onboarding.
  onboarded.subscribe((v) => {
    lsSet('md-reader-onboarded', v ? '1' : '');
  });
  // New prefs (#8 code theme, #9 font family, #10 reopen-last).
  codeTheme.subscribe((v) => {
    document.documentElement.setAttribute('data-code-theme', v);
    lsSet('md-reader-code-theme', v);
  });
  fontFamily.subscribe((v) => {
    document.documentElement.setAttribute('data-font-family', v);
    lsSet('md-reader-font-family', v);
  });
  reopenLast.subscribe((v) => {
    lsSet('md-reader-reopen-last', v ? '1' : '0');
  });
  // Outline pane (#3).
  outlineVisible.subscribe((v) => {
    lsSet('md-reader-outline-visible', v);
  });
}

export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}

export function setViewMode(mode) {
  viewMode.set(mode === 'slide' ? 'slide' : 'read');
}

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
  try { localStorage.setItem('md-reader-onboarded', '1'); } catch {}
  onboarded.set(true);
}

export const fontSizeLabels = { sm: 'S', '': 'M', lg: 'L', xl: 'XL', xxl: '2X' };

export function adjustFontSize(delta) {
  const sizes = ['sm', '', 'lg', 'xl', 'xxl'];
  fontSize.update((cur) => {
    const i = sizes.indexOf(cur || '');
    return sizes[Math.max(0, Math.min(sizes.length - 1, i + delta))];
  });
}

export function adjustContentWidth(delta) {
  contentWidth.update((w) => Math.max(400, Math.min(1200, w + delta)));
}

// ── Reset all prefs (ROADMAP v1.1 #11) ────────────────────────────────────
//
// Wipes every `md-reader-*` key from localStorage and restores every store in
// this file to its default. Designed as the "I'm stuck, get me out" button
// the user can hit without understanding which knob they turned.
//
// We intentionally only touch prefs in this file (theme, font, layout, code
// theme, reopen-last, outline, onboarding) — NOT library state, progress, or
// recents. Those have their own delete/clear paths; mixing them here would
// surprise the user.
export function resetAllPrefs() {
  if (typeof localStorage === 'undefined') return;
  // Wipe every md-reader-* key. Catch any other custom keys that may have
  // been added by future code in this file.
  for (let i = localStorage.length - 1; i >= 0; i -= 1) {
    const k = localStorage.key(i);
    if (k && k.startsWith('md-reader-')) localStorage.removeItem(k);
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
  lsSet('md-reader-theme', PREFS_DEFAULTS.theme);
  lsSet('md-reader-fontsize', PREFS_DEFAULTS.fontSize);
  lsSet('md-reader-content-width', String(PREFS_DEFAULTS.contentWidth));
  lsSet('md-reader-nav-collapsed', PREFS_DEFAULTS.navCollapsed ? '1' : '0');
  lsSet('md-reader-view-mode', PREFS_DEFAULTS.viewMode);
  lsSet('md-reader-code-theme', PREFS_DEFAULTS.codeTheme);
  lsSet('md-reader-font-family', PREFS_DEFAULTS.fontFamily);
  lsSet('md-reader-reopen-last', PREFS_DEFAULTS.reopenLast ? '1' : '0');
  lsSet('md-reader-outline-visible', PREFS_DEFAULTS.outlineVisible);
}

// Re-export `get` so consumers don't have to import from svelte/store if
// they only need to read prefs synchronously from a click handler.
export { get };
