<script>
  import { TAURI } from '../lib/tauri.js';
  import {
    folderName, folderMeta, route, progressMap, theme,
    navCollapsed, navOpen, settingsOpen, goHome, goChapter, toggleTheme,
    getRecents, openFolderPath, pickFolder,
  } from '../lib/stores.js';
  import { labelFromName, buildFolderTree } from '../lib/index.js';
  import SidebarTree from './SidebarTree.svelte';

  let { isMobile = false } = $props();

  // Full nested tree built from each file's *disk* path so top-level group
  // folders AND their subfolders are preserved. `navPath` keeps the
  // group-stripped path the router/Reader navigate by.
  const sidebarTree = $derived(
    buildFolderTree($folderMeta.map((f) => ({ ...f, path: f.diskPath, navPath: f.path })))
  );

  let filter = $state('');
  let menuOpen = $state(false);
  let recents = $state([]);

  const bookmarks = $derived(
    $folderMeta.filter((f) => $progressMap[f.diskPath || f.path]?.bookmarked)
  );

  function isCurrent(item) {
    return $route.name === 'chapter' && $route.path === item.path;
  }

  async function toggleMenu() {
    if (menuOpen) { menuOpen = false; return; }
    recents = (await getRecents()).filter((r) => r.exists);
    menuOpen = true;
  }
  async function chooseRecent(path) { menuOpen = false; await openFolderPath(path); }
  async function chooseOpen() { menuOpen = false; await pickFolder(); }

  function collapseNav() {
    menuOpen = false;
    if (isMobile) navOpen.set(false);
    else navCollapsed.set(true);
  }

  function navTo(path) { menuOpen = false; goChapter(path); }

  function closeMenuOnOutsideClick(e) {
    if (!menuOpen) return;
    if (!(e.target).closest('.folder-menu') && !(e.target).closest('.sidebar-iconbtn')) {
      menuOpen = false;
    }
  }
</script>

<svelte:document onclick={closeMenuOnOutsideClick} />

<aside class="sidebar" id="sidebar">
  <!-- Brand + folder context -->
  <div class="sidebar-brand">
    <div class="sidebar-brand-row">
      <button class="sidebar-brand-btn" onclick={goHome} title="Library overview">
        <span class="sidebar-brand-name">MD Reader</span>
      </button>
      <button class="sidebar-iconbtn" onclick={collapseNav} title="Hide navigation" aria-label="Hide navigation">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"/></svg>
      </button>
    </div>
    <button class="sidebar-folder-sub" onclick={toggleMenu} title="Switch folder">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
      <span>{$folderName}</span>
      <svg class="sidebar-folder-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"/></svg>
    </button>

    {#if menuOpen}
      <div class="folder-menu open">
        {#each recents as r (r.path)}
          <div class="recent-item" onclick={() => chooseRecent(r.path)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); chooseRecent(r.path); } }} role="button" tabindex="0">
            <span style="flex-shrink:0">📁</span><span class="recent-item-name">{r.name}</span>
          </div>
        {/each}
        {#if TAURI}
          <button class="folder-menu-open" onclick={chooseOpen}>＋ Open folder…</button>
        {/if}
      </div>
    {/if}
  </div>

  <input class="sidebar-filter" type="text" placeholder="Filter chapters…" bind:value={filter} />

  <div class="sidebar-scroll">
    <button class="sidebar-nav-item {$route.name !== 'chapter' ? 'active' : ''}" onclick={goHome}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>
      <span>Home</span>
    </button>
    <button class="sidebar-nav-item" onclick={() => settingsOpen.set(true)}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
      <span>Settings</span>
    </button>

    {#if bookmarks.length}
      <div class="sidebar-section-label">Bookmarks</div>
      {#each bookmarks as item (item.path)}
        <button class="sidebar-chapter {isCurrent(item) ? 'active' : ''}" onclick={() => navTo(item.path)} title={labelFromName(item.name)}>
          <span class="sidebar-chapter-title">{labelFromName(item.name)}</span>
          <span class="bm">★</span>
        </button>
      {/each}
    {/if}

    {#if $folderMeta.length}<div class="sidebar-section-label">Chapters</div>{/if}
    <SidebarTree nodes={sidebarTree} depth={0} {filter} />
  </div>

  <div class="sidebar-footer2">
    {#if TAURI}
      <button class="sidebar-open-btn" onclick={chooseOpen}>Open Folder</button>
    {/if}
    <button class="sidebar-darkmode" onclick={toggleTheme} title="Toggle theme">
      <span class="sidebar-darkmode-label">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
        Dark Mode
      </span>
      <span class="sidebar-toggle {$theme === 'dark' ? 'on' : ''}"><span class="sidebar-toggle-knob"></span></span>
    </button>
  </div>
</aside>
