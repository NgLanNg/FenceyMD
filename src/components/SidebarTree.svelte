<script>
  import { route, progressMap, navOpen, goChapter } from '../lib/stores.js';
  import { labelFromName, shortTitle } from '../lib/index.js';
  import Self from './SidebarTree.svelte';

  let { nodes = [], depth = 0, filter = '' } = $props();

  let collapsed = $state(new Set());
  function toggle(p) {
    const s = new Set(collapsed);
    s.has(p) ? s.delete(p) : s.add(p);
    collapsed = s;
  }

  const q = $derived(filter.trim().toLowerCase());

  function fileMatches(node) {
    return !q || labelFromName(node.name).toLowerCase().includes(q) || node.name.toLowerCase().includes(q);
  }
  function nodeVisible(node) {
    return node.path ? fileMatches(node) : (node.children || []).some(nodeVisible);
  }
  function countFiles(node) {
    let n = 0;
    for (const c of node.children || []) n += c.path ? 1 : countFiles(c);
    return n;
  }
  function isCurrent(node) {
    return $route.name === 'chapter' && $route.path === node.item.navPath;
  }
  function status(node) {
    const pr = $progressMap[node.item.diskPath] || {};
    return { done: (pr.scroll || 0) >= 0.95, bookmarked: !!pr.bookmarked };
  }
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
      {#if !collapsed.has(node.folderPath) || q}
        <Self nodes={node.children} depth={depth + 1} {filter} />
      {/if}
    {/if}
  {/if}
{/each}
