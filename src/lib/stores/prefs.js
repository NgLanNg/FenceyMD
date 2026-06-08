// Persisted UI preferences: theme, reading font size, content width, nav state.
import { writable } from 'svelte/store';

const lsGet = (k, d) => { try { return localStorage.getItem(k) ?? d; } catch { return d; } };

export const theme = writable(lsGet('md-reader-theme', 'light'));
export const fontSize = writable(lsGet('md-reader-fontsize', '')); // '', sm, lg, xl, xxl
export const contentWidth = writable(parseInt(lsGet('md-reader-content-width', '680'), 10) || 680);
export const navCollapsed = writable(lsGet('md-reader-nav-collapsed', '0') === '1');
export const navOpen = writable(false); // mobile drawer
export const settingsOpen = writable(false); // settings modal
export const viewMode = writable(lsGet('md-reader-view-mode', 'read')); // 'read' | 'slide'

// Apply + persist prefs whenever they change.
if (typeof document !== 'undefined') {
  theme.subscribe((v) => {
    document.documentElement.setAttribute('data-theme', v);
    try { localStorage.setItem('md-reader-theme', v); } catch {}
  });
  fontSize.subscribe((v) => {
    document.documentElement.setAttribute('data-font-size', v || '');
    try { localStorage.setItem('md-reader-fontsize', v || ''); } catch {}
  });
  contentWidth.subscribe((w) => {
    const root = document.documentElement;
    root.style.setProperty('--content-w', w + 'px');
    const base = w / 680;
    root.style.setProperty('--home-w', Math.round(980 * base) + 'px');
    root.style.setProperty('--landing-w', Math.round(820 * base) + 'px');
    try { localStorage.setItem('md-reader-content-width', String(w)); } catch {}
  });
  navCollapsed.subscribe((v) => {
    try { localStorage.setItem('md-reader-nav-collapsed', v ? '1' : '0'); } catch {}
  });
  viewMode.subscribe((v) => {
    try { localStorage.setItem('md-reader-view-mode', v); } catch {}
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
