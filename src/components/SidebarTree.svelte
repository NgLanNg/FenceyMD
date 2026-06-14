<script>
  /**
   * SidebarTree.svelte — the recursive chapter tree inside <Sidebar>.
   *
   * Single responsibility: render one level of the nested folder/chapter tree
   * and recurse (via <Self>) into subfolders. Owns per-level collapse state and
   * the live filter match, nothing else.
   *
   * Node shape (produced by buildFolderTree in index.js): a node is EITHER
   *   - a file: has `path`, plus `item` carrying { navPath, diskPath, name }; or
   *   - a folder: has `folderPath` and `children`, no `path`.
   * The presence/absence of `node.path` is the discriminator used throughout.
   *
   * Path duality (critical): the tree fed here is built from *disk* paths, so
   * navigation must use `node.item.navPath` (the group-stripped path the router
   * understands) — NOT `node.path` (the disk path). Reading progress, however,
   * is keyed by `node.item.diskPath`. Mixing these up silently breaks either
   * navigation or the done/bookmark indicators.
   *
   * Collaborators: stores.js ($route/$progressMap for highlight+status,
   * navOpen+goChapter for navigation); index.js (labelFromName/shortTitle).
   */
  import { route, progressMap, navOpen, goChapter } from '../lib/stores.js';
  import { labelFromName, shortTitle } from '../lib/index.js';
  import Self from './SidebarTree.svelte';

  let { nodes = [], depth = 0, filter = '' } = $props();

  // Set of collapsed folderPaths for THIS subtree instance. Local per level so
  // collapse state lives with the node that owns the folder rows.
  let collapsed = $state(new Set());
  /**
   * Toggle a folder's collapsed state by its folderPath.
   * Reassigns a fresh Set rather than mutating in place — Svelte 5 reactivity
   * tracks the binding identity, so an in-place add/delete would not re-render.
   */
  function toggle(p) {
    const s = new Set(collapsed);
    s.has(p) ? s.delete(p) : s.add(p);
    collapsed = s;
  }

  // Normalised filter query; '' means "no filter" (everything visible).
  const q = $derived(filter.trim().toLowerCase());

  // A file matches when there's no query, or the query is a substring of
  // either its display label or its raw filename (so users can search either).
  function fileMatches(node) {
    return !q || labelFromName(node.name).toLowerCase().includes(q) || node.name.toLowerCase().includes(q);
  }
  // A folder is visible iff any descendant file matches — recurses so a deep
  // match keeps its whole ancestor chain on screen.
  function nodeVisible(node) {
    return node.path ? fileMatches(node) : (node.children || []).some(nodeVisible);
  }
  // Count of leaf files under a folder (recursive), shown as the row's badge.
  function countFiles(node) {
    let n = 0;
    for (const c of node.children || []) n += c.path ? 1 : countFiles(c);
    return n;
  }
  // Active-row test: compare the router path against the file's NAV path.
  function isCurrent(node) {
    return $route.name === 'chapter' && $route.path === node.item.navPath;
  }
  // Read/bookmark status for a file row, keyed by DISK path. A chapter counts
  // as "done" once scrolled past 95% (the same threshold the reader records).
  function status(node) {
    const pr = $progressMap[node.item.diskPath] || {};
    return { done: (pr.scroll || 0) >= 0.95, bookmarked: !!pr.bookmarked };
  }
  // Navigate to a chapter file. navOpen.set(false) closes the mobile drawer on
  // tap; goChapter routes by the nav path (not the disk path).
  function go(node) {
    navOpen.set(false);
    goChapter(node.item.navPath);
  }
</script>

{#each nodes as node (node.path || node.folderPath)}
  {#if nodeVisible(node)}
    {#if node.path}
      <button
        class="sidebar-chapter {isCurrent(node) ? 'active' : ''}"
        style="padding-left: {depth * 14 + 12}px"
        onclick={() => go(node)}
        title={labelFromName(node.name)}
      >
        <span class="sidebar-chapter-title">{shortTitle(node.name)}</span>
        {#if status(node).bookmarked}<span class="bm">★</span>{:else if status(node).done}<span class="done">✓</span>{/if}
      </button>
    {:else}
      <button
        class="sidebar-folder-row"
        style="padding-left: {depth * 14 + 8}px"
        onclick={() => toggle(node.folderPath)}
        title={labelFromName(node.name)}
      >
        <svg class="sidebar-folder-caret2 {collapsed.has(node.folderPath) && !q ? 'collapsed' : ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"/></svg>
        <span class="sidebar-folder-row-name">{labelFromName(node.name)}</span>
        <span class="sidebar-folder-row-count">{countFiles(node)}</span>
      </button>
      <!-- An active filter (`q`) force-expands every folder so matches deep in
           a collapsed branch stay reachable; collapse state is preserved and
           restored once the filter is cleared. -->
      {#if !collapsed.has(node.folderPath) || q}
        <Self nodes={node.children} depth={depth + 1} {filter} />
      {/if}
    {/if}
  {/if}
{/each}
