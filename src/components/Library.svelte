<script>
  import { folderName, folderMeta, groupMeta, progressMap, route, goGroup, goHome, goChapter } from '../lib/stores.js';
  import { buildFolderTree, labelFromName } from '../lib/index.js';
  import TreeNode from './TreeNode.svelte';

  const groups = $derived(Object.keys($groupMeta));
  const ungrouped = $derived($folderMeta.filter((f) => !f.grouped));
  const totalFiles = $derived($folderMeta.length);

  const currentGroup = $derived($route.name === 'group' ? $route.group : null);
  const isRootGroup = $derived(currentGroup === '__root');
  const currentTitle = $derived(isRootGroup ? 'Root files' : currentGroup);
  const groupItems = $derived(
    isRootGroup ? ungrouped : currentGroup ? ($groupMeta[currentGroup] || []) : []
  );
  const tree = $derived(currentGroup ? buildFolderTree(groupItems) : []);

  // Bookmarked + recently-read files, for the "Recent Activity" rail.
  const bookmarks = $derived(
    $folderMeta.filter((f) => $progressMap[f.diskPath || f.path]?.bookmarked)
  );
  const inProgress = $derived(
    $folderMeta
      .filter((f) => {
        const s = $progressMap[f.diskPath || f.path]?.scroll || 0;
        return s > 0.02 && s < 0.95;
      })
      .slice(0, 4)
  );
  const activity = $derived((bookmarks.length ? bookmarks : inProgress).slice(0, 4));

  function hasChapters(items) {
    return items.some((i) => /ch\.?(\d+)/i.test(i.name));
  }
  function openGroup(g) {
    const items = $groupMeta[g] || [];
    if (items.length === 1) goChapter(items[0].path);
    else goGroup(g);
  }
  function pct(item) {
    return Math.round(($progressMap[item.diskPath || item.path]?.scroll || 0) * 100);
  }
  // Decorative card variant by index, so the grid has rhythm.
  const variants = ['v-primary', 'v-secondary', 'v-tertiary'];
</script>

{#if currentGroup}
  <!-- ── Group landing ─────────────────────────────────────────── -->
  <div class="lib-page">
    <button class="nav-back-link" onclick={goHome}>← Library</button>
    <div class="lib-eyebrow-row">
      <span class="lib-eyebrow">{isRootGroup ? 'Top level' : 'Group'}</span>
      <span class="lib-folder-chip">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-8z"/></svg>
        {currentTitle}
      </span>
    </div>
    <h1 class="lib-display-title">{currentTitle}</h1>
    <div class="lib-rule"></div>
    <p class="lib-sub">{groupItems.length} file{groupItems.length !== 1 ? 's' : ''}</p>

    <div class="chapter-list">
      {#each tree as node (node.path || node.folderPath)}
        <TreeNode {node} depth={0} />
      {/each}
    </div>
  </div>
{:else}
  <!-- ── Library home ──────────────────────────────────────────── -->
  <div class="lib-page">
    <div class="lib-eyebrow-row">
      <span class="lib-eyebrow">Library</span>
      <span class="lib-folder-chip">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-8z"/></svg>
        Folder: {$folderName}
      </span>
    </div>
    <h1 class="lib-display-title">{$folderName}</h1>
    <div class="lib-rule"></div>
    <p class="lib-sub">
      {totalFiles} Markdown file{totalFiles !== 1 ? 's' : ''} in {groups.length} group{groups.length !== 1 ? 's' : ''}. Click a group to browse.
    </p>

    <div class="lib-grid">
      {#each groups as g, i (g)}
        <div
          class="lib-card {variants[i % 3]}"
          onclick={() => openGroup(g)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openGroup(g); } }}
          role="button"
          tabindex="0"
        >
          <div class="lib-card-blob"></div>
          <div class="lib-card-body">
            <div class="lib-card-thumb">
              <span class="lib-card-thumb-line"></span>
            </div>
            <h3 class="lib-card-title">{g}</h3>
            <p class="lib-card-meta">
              {$groupMeta[g].length} file{$groupMeta[g].length !== 1 ? 's' : ''}{hasChapters($groupMeta[g]) ? ' · sorted by chapter' : ''}
            </p>
            <div class="lib-card-action">
              <span>Open group</span>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/></svg>
            </div>
          </div>
        </div>
      {/each}

      {#if ungrouped.length}
        <div
          class="lib-card v-secondary"
          onclick={() => goGroup('__root')}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); goGroup('__root'); } }}
          role="button"
          tabindex="0"
        >
          <div class="lib-card-blob"></div>
          <div class="lib-card-body">
            <div class="lib-card-thumb lib-card-thumb-doc">
              <span></span><span></span><span></span>
            </div>
            <h3 class="lib-card-title">Root files</h3>
            <p class="lib-card-meta">{ungrouped.length} file{ungrouped.length !== 1 ? 's' : ''} at top level</p>
            <div class="lib-card-action">
              <span>Browse</span>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/></svg>
            </div>
          </div>
        </div>
      {/if}
    </div>

    {#if activity.length}
      <div class="lib-activity">
        <div class="lib-activity-head">
          <h4 class="lib-activity-label">{bookmarks.length ? 'Bookmarked' : 'Continue reading'}</h4>
        </div>
        <div class="lib-activity-list">
          {#each activity as item (item.path)}
            <div
              class="lib-activity-row"
              onclick={() => goChapter(item.path)}
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); goChapter(item.path); } }}
              role="button"
              tabindex="0"
            >
              <span class="lib-activity-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
              </span>
              <div class="lib-activity-text">
                <p class="lib-activity-name">{labelFromName(item.name)}</p>
                <p class="lib-activity-meta">
                  {#if bookmarks.length}Bookmarked{:else}{pct(item)}% read{/if} · {item.path}
                </p>
              </div>
              {#if bookmarks.length}<span class="lib-activity-star">★</span>{/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
{/if}
