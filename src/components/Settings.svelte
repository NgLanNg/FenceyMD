<script>
  import { TAURI } from '../lib/tauri.js';
  import {
    theme, fontSize, contentWidth, fontSizeLabels, settingsOpen,
    adjustFontSize, adjustContentWidth, pickFolder,
    codeTheme, fontFamily, reopenLast, resetAllPrefs,
  } from '../lib/stores.js';
  import { setCodeTheme } from '../lib/renderers/shiki.js';

  function close() { settingsOpen.set(false); }
  async function openFolder() { close(); await pickFolder(); }

  // When the user picks a new code theme, push it to the shiki renderer
  // so any chapter currently on screen re-themes in place. Persisting +
  // updating the html data-attr is handled by the codeTheme.subscribe
  // hook in prefs.js.
  function pickCodeTheme(next) {
    if (next === $codeTheme) return;
    codeTheme.set(next);
    setCodeTheme(next);
  }
</script>

<svelte:window onkeydown={(e) => { if ($settingsOpen && e.key === 'Escape') close(); }} />

{#if $settingsOpen}
  <div class="settings-overlay" onclick={(e) => { if (e.target === e.currentTarget) close(); }} role="presentation">
    <div class="settings-dialog" role="dialog" aria-modal="true" aria-label="Settings">
      <div class="settings-head">
        <h2 class="settings-title">Settings</h2>
        <button class="settings-close" onclick={close} title="Close" aria-label="Close settings">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>

      <div class="settings-section" data-test-section="appearance">
        <div class="settings-label">Appearance</div>
        <div class="settings-row">
          <span class="settings-row-name">Theme</span>
          <div class="settings-seg" data-test="theme-control">
            <button class:active={$theme === 'light'} onclick={() => theme.set('light')}>Light</button>
            <button class:active={$theme === 'dark'} onclick={() => theme.set('dark')}>Dark</button>
          </div>
        </div>
        <div class="settings-row">
          <span class="settings-row-name">Code theme</span>
          <div class="settings-seg" data-test="code-theme-control">
            <button class:active={$codeTheme === 'github'} onclick={() => pickCodeTheme('github')}>GitHub</button>
            <button class:active={$codeTheme === 'nord'} onclick={() => pickCodeTheme('nord')}>Nord</button>
          </div>
        </div>
      </div>

      <div class="settings-section" data-test-section="reading">
        <div class="settings-label">Reading</div>
        <div class="settings-row">
          <span class="settings-row-name">Font family</span>
          <div class="settings-seg" data-test="font-family-control">
            <button class:active={$fontFamily === 'serif'} onclick={() => fontFamily.set('serif')}>Serif</button>
            <button class:active={$fontFamily === 'sans'} onclick={() => fontFamily.set('sans')}>Sans</button>
            <button class:active={$fontFamily === 'mono'} onclick={() => fontFamily.set('mono')}>Mono</button>
          </div>
        </div>
        <div class="settings-row">
          <span class="settings-row-name">Font size</span>
          <div class="settings-stepper">
            <button onclick={() => adjustFontSize(-1)} aria-label="Decrease font size">A−</button>
            <span class="settings-val">{fontSizeLabels[$fontSize] ?? 'M'}</span>
            <button onclick={() => adjustFontSize(1)} aria-label="Increase font size">A+</button>
          </div>
        </div>
        <div class="settings-row">
          <span class="settings-row-name">Reading width</span>
          <div class="settings-stepper">
            <button onclick={() => adjustContentWidth(-40)} aria-label="Narrower">−</button>
            <span class="settings-val">{$contentWidth}px</span>
            <button onclick={() => adjustContentWidth(40)} aria-label="Wider">+</button>
          </div>
        </div>
        <div class="settings-row">
          <span class="settings-row-name">Reopen last folder on launch</span>
          <button
            class="settings-toggle"
            class:on={$reopenLast}
            role="switch"
            aria-checked={$reopenLast}
            aria-label="Reopen last folder on launch"
            data-test="reopen-last-toggle"
            onclick={() => reopenLast.set(!$reopenLast)}
          >
            <span class="settings-toggle-knob"></span>
          </button>
        </div>
      </div>

      {#if TAURI}
        <div class="settings-section">
          <div class="settings-label">Library</div>
          <div class="settings-row">
            <span class="settings-row-name">Folder</span>
            <button class="btn-ghost" onclick={openFolder}>Open another folder…</button>
          </div>
        </div>
      {/if}

      <div class="settings-section" data-test-section="reset">
        <div class="settings-row">
          <span class="settings-row-name">
            Reset all preferences
            <span class="settings-row-hint">Restores theme, font, code theme, and reopen-last to their defaults.</span>
          </span>
          <button
            class="btn-ghost"
            data-test="reset-prefs-btn"
            onclick={resetAllPrefs}
          >Reset all prefs</button>
        </div>
      </div>

      <div class="settings-foot">
        <span class="settings-build">MD Reader · v1.0.0 · Local-only</span>
        <button class="btn-primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}
