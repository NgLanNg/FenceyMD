<script>
  import { TAURI } from '../lib/tauri.js';
  import {
    theme, fontSize, contentWidth, fontSizeLabels, settingsOpen,
    adjustFontSize, adjustContentWidth, pickFolder,
  } from '../lib/stores.js';

  function close() { settingsOpen.set(false); }
  async function openFolder() { close(); await pickFolder(); }
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

      <div class="settings-section">
        <div class="settings-label">Appearance</div>
        <div class="settings-row">
          <span class="settings-row-name">Theme</span>
          <div class="settings-seg">
            <button class:active={$theme === 'light'} onclick={() => theme.set('light')}>Light</button>
            <button class:active={$theme === 'dark'} onclick={() => theme.set('dark')}>Dark</button>
          </div>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-label">Reading</div>
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

      <div class="settings-foot">
        <span class="settings-build">MD Reader · v1.0.0 · Local-only</span>
        <button class="btn-primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}
