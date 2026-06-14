<script>
  // Picker.svelte — the no-folder-open landing screen.
  //
  // Single responsibility: let the user choose a book folder to open, either
  // via the native folder dialog or by clicking a remembered recent. It owns
  // only its local view state (the recents list + an error string); the
  // actual open/persist logic lives in the stores. Once a folder opens the
  // `ready` store flips and App.svelte unmounts this in favour of the shell.
  //
  // Mounted by App.svelte while `!ready`.
  import { onMount } from 'svelte';
  import { TAURI } from '../lib/tauri.js';
  import { pickFolder, openFolderPath, getRecents, removeRecent } from '../lib/stores.js';

  let recents = $state([]);
  let error = $state('');

  /** Reload the recents list from the store. Each entry carries an `exists`
   *  flag so the UI can show stale (deleted) folders as dimmed + unclickable
   *  rather than hiding them silently. */
  async function refresh() { recents = await getRecents(); }
  onMount(refresh);

  /** Open the native folder picker. pickFolder both prompts and opens the
   *  chosen folder; a thrown error (e.g. unreadable folder) is surfaced
   *  inline rather than crashing the screen. */
  async function onOpen() {
    error = '';
    try { await pickFolder(); }
    catch (e) { error = 'Could not read folder: ' + e; }
  }
  /** Re-open a remembered folder. No-op for entries the store already marked
   *  missing. If the folder vanished between scan and click, openFolderPath
   *  returns false — show the error and refresh so the row flips to missing. */
  async function onRecent(path, exists) {
    if (!exists) return;
    error = '';
    const ok = await openFolderPath(path);
    if (!ok) { error = 'Folder no longer exists.'; await refresh(); }
  }
  /** Drop a folder from recents. stopPropagation keeps the parent row's
   *  click (which would try to OPEN the folder) from firing on the ✕. */
  async function onRemove(path, ev) {
    ev.stopPropagation();
    await removeRecent(path);
    await refresh();
  }
</script>

<div class="picker-screen">
  <div class="picker-hero">
    <h1 class="picker-title">FenceyMD</h1>
    <p class="picker-tagline">A calm space for your local Markdown library.</p>
  </div>

  <div class="picker-card">
    <button class="picker-open-btn" onclick={onOpen}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
      Open Folder
    </button>

    {#if error}<p class="picker-error">{error}</p>{/if}

    {#if recents.length}
      <div class="picker-divider"></div>
      <div class="picker-recents-label">Recent Folders</div>
      <div class="picker-recents">
        {#each recents as r (r.path)}
          <div
            class="picker-recent {r.exists ? '' : 'missing'}"
            onclick={() => onRecent(r.path, r.exists)}
            onkeydown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && r.exists) { e.preventDefault(); onRecent(r.path, r.exists); } }}
            role="button"
            tabindex="0"
          >
            <span class="picker-recent-icon">
              {#if r.exists}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="2" y1="2" x2="22" y2="22"/></svg>
              {/if}
            </span>
            <div class="picker-recent-text">
              <div class="picker-recent-name">
                {r.name}{#if !r.exists}<span class="picker-recent-missing"> (missing)</span>{/if}
              </div>
              <div class="picker-recent-path">{r.path}</div>
            </div>
            <button class="picker-recent-x" title="Remove" aria-label="Remove from recents" onclick={(e) => onRemove(r.path, e)}>✕</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#if !TAURI}
    <p class="picker-hint">ⓘ Folder access requires the FenceyMD desktop application for secure file system hooks.</p>
  {/if}
</div>
