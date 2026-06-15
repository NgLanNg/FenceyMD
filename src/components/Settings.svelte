<script>
  import {
    TAURI, debugLogReveal, debugLogClear,
    agentsDetect, agentsRegister, agentsUnregister, cliStatus, cliInstall,
  } from '../lib/tauri.js';
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

  async function revealDebugLog() {
    if (!TAURI) return;
    await debugLogReveal();
  }
  async function clearDebugLog() {
    if (!TAURI) return;
    await debugLogClear();
  }

  // ── AI agent control (MCP) ──────────────────────────────────────────────
  // The detected agents + their registration state, and the `fenceymd` CLI
  // install status. Loaded from Rust each time the dialog opens so the toggles
  // reflect on-disk truth.
  let agents = $state([]);
  let agentBusy = $state('');   // id currently toggling
  let agentError = $state('');
  let cli = $state({ installed: false });
  let cliBusy = $state(false);

  async function loadAgentControl() {
    if (!TAURI) return;
    agents = await agentsDetect();
    cli = await cliStatus();
  }

  // Re-read on-disk truth whenever the dialog opens.
  $effect(() => {
    if ($settingsOpen) loadAgentControl();
  });

  async function toggleAgent(agent) {
    if (agentBusy) return;
    agentBusy = agent.id;
    agentError = '';
    try {
      if (agent.registered) await agentsUnregister(agent.id);
      else await agentsRegister(agent.id);
      agents = await agentsDetect();
    } catch (e) {
      agentError = e?.message || String(e);
    } finally {
      agentBusy = '';
    }
  }

  async function installCli() {
    if (cliBusy) return;
    cliBusy = true;
    agentError = '';
    try {
      await cliInstall();
      cli = await cliStatus();
    } catch (e) {
      agentError = e?.message || String(e);
    } finally {
      cliBusy = false;
    }
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

      {#if TAURI}
        <div class="settings-section" data-test-section="agents">
          <div class="settings-label">AI agent control</div>
          <span class="settings-row-hint" style="display:block; margin-bottom: var(--space-3);">
            Let an AI coding agent (Claude Code, Gemini, OpenCode, Codex) drive this
            reader over MCP. Toggling writes the <code>fenceymd</code> MCP entry into
            that agent's own config. <strong>Restart the agent to apply.</strong>
          </span>

          <div class="settings-row">
            <span class="settings-row-name">
              Terminal CLI
              <span class="settings-row-hint">
                {#if cli.installed && cli.points_at_current}
                  Installed — <code>{cli.path}</code>
                {:else if cli.installed}
                  At <code>{cli.path}</code>, but pointing elsewhere — re-install to update.
                {:else}
                  Makes <code>fenceymd</code> runnable from a terminal; agent configs then use a clean <code>fenceymd</code> command.
                {/if}
              </span>
            </span>
            <button class="btn-ghost" disabled={cliBusy} data-test="cli-install-btn" onclick={installCli}>
              {cliBusy ? 'Installing…' : (cli.installed && cli.points_at_current ? 'Re-install' : 'Install CLI')}
            </button>
          </div>

          {#each agents as agent (agent.id)}
            <div class="settings-row">
              <span class="settings-row-name">
                {agent.display_name}
                {#if !agent.detected}
                  <span class="settings-row-hint">Not detected on this machine — toggle on to register anyway.</span>
                {/if}
              </span>
              <button
                class="settings-toggle"
                class:on={agent.registered}
                role="switch"
                aria-checked={agent.registered}
                aria-label={`Register FenceyMD with ${agent.display_name}`}
                data-test={`agent-toggle-${agent.id}`}
                disabled={agentBusy === agent.id}
                onclick={() => toggleAgent(agent)}
              >
                <span class="settings-toggle-knob"></span>
              </button>
            </div>
          {/each}
          {#if agentError}
            <div class="settings-row-hint" style="color: var(--tertiary);">{agentError}</div>
          {/if}
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

      {#if TAURI}
        <div class="settings-section" data-test-section="debug">
          <div class="settings-label">Debug</div>
          <div class="settings-row">
            <span class="settings-row-name">
              Activity log
              <span class="settings-row-hint">A file at <code>app_data_dir/debug.log</code> records what the app was doing. Use it when reporting a bug — the log captures folder open, watcher, render, and unhandled errors.</span>
            </span>
            <div class="settings-row-actions">
              <button class="btn-ghost" onclick={revealDebugLog}>Open log folder</button>
              <button class="btn-ghost" onclick={clearDebugLog}>Clear log</button>
            </div>
          </div>
        </div>
      {/if}

      <div class="settings-foot">
        <span class="settings-build">FenceyMD · v1.0.0 · Local-only</span>
        <button class="btn-primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}
