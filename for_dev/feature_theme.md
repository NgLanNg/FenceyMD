# Theme + OS auto-detect

## Vision & DoD (5W1H)

**What.** A light/dark theme with three states: explicit light, explicit dark, follow OS (`prefers-color-scheme`). The theme is reflected in the CSS via the `data-theme` attribute on `<html>`. The user's choice persists across restarts; if they picked "follow OS," the theme tracks the OS as it changes.

**Why.** Long reading sessions are easier on the eyes in a theme that matches the room. Users who work in dark mode everywhere expect their reader to follow. Users who prefer light should be able to lock it.

**Who.** Any user. The "follow OS" default is the right call for new users; power users override.

**When.**
- On first launch: the default is "follow OS" (we read `matchMedia('(prefers-color-scheme: dark)')`).
- On every launch: we re-read the OS preference and apply it (unless the user has explicitly overridden).
- On the user's manual toggle: we lock the choice (so future OS changes don't override).

**Where.** `src/lib/stores/prefs.js#theme`. The store is the source of truth. The CSS uses `[data-theme="dark"]` selectors. A live `change` listener on the media query handles OS changes.

**How (acceptance / DoD).**
- The user can pick light, dark, or follow OS.
- An explicit choice (light or dark) persists across restarts.
- A "follow OS" choice follows the system as the user toggles macOS dark mode.
- The CSS theme is applied via `data-theme` on `<html>`.
- Mermaid, Shiki, and KaTeX all re-render on theme flip.
- The theme toggle in the toolbar is a one-click switch between light and dark; it sets an explicit choice.

---

## How we implemented it

**What.** A `theme` writable in the prefs store, with three possible values: `'light' | 'dark' | 'os'`. A `$effect` applies the theme to `<html>` and listens for OS changes.

**Why this shape.** The "explicit vs OS-following" duality is a common UX pattern. Encoding it as `'light' | 'dark' | 'os'` makes the store the single source of truth and the effect the only place that talks to the DOM.

**When.** On every theme value change. On every `prefers-color-scheme` change (when in 'os' mode).

**Where.**
- `src/lib/stores/prefs.js` — the store, the effect, the OS listener.
- `src/lib/anchors.js` — the theme effect for the OS listener.
- `src/components/Settings.svelte` — the theme picker (3-way).
- `src/components/Reader.svelte` — the theme toggle button (1-click light ↔ dark).

**How (tech).**
- **OS detection**: `window.matchMedia('(prefers-color-scheme: dark)').matches` returns the current preference.
- **OS listener**: `mediaQuery.addEventListener('change', () => { if (theme === 'os') applyTheme() })`. The listener is only active when the user is in 'os' mode.
- **Apply**: `document.documentElement.setAttribute('data-theme', effectiveTheme)` where `effectiveTheme` is the resolved value (`'dark'` if OS is dark and theme is 'os', else the explicit value).
- **Renderer re-render**: mermaid is re-initialized with the new theme; Shiki's dual-theme spans are swapped via CSS-variable cascade (no JS work needed); KaTeX has no theme.
- **CSS variables**: every color in the app is a CSS variable; the `[data-theme]` selector swaps the values. There are no hard-coded colors in components.
- **localStorage**: the explicit choice persists under `fenceymd.theme`. The 'os' value persists as well (we want the user's preference to survive restarts).

**Gotchas.**
- `matchMedia` is the standard but not available in all WebViews. In Tauri 2's WKWebView it's fine; in some Linux WebKitGTK versions it's spotty. We fall back to light if `matchMedia` is undefined.
- The v1.0 default was 'light', which ignored the OS preference. v1.1 made 'os' the default — but we needed a `change` listener to actually track OS changes after launch.
- The toolbar's theme toggle button is a 1-click switch (light ↔ dark), not a 3-way. Clicking it sets an explicit value, breaking out of 'os' mode.
- "Reset all preferences" returns the theme to 'os' default.
