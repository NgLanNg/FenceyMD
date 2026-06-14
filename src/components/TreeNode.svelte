<script>
  /**
   * TreeNode.svelte — one row of the chapter tree shown in the Library (home)
   * view, recursing into subfolders via <Self>.
   *
   * Single responsibility: render a single file or folder node from a
   * buildFolderTree() result, and (for folders) toggle its own expansion.
   *
   * Relationship to SidebarTree: same recursive shape, different context.
   * SidebarTree's tree is built from disk paths (so it navigates via
   * `node.item.navPath`); the Library builds TreeNode's tree from
   * already-group-stripped paths, so here `node.path` IS the nav path and is
   * passed straight to goChapter. Do not "unify" the two by reaching for
   * node.item here — Library nodes don't carry the same item wrapper.
   *
   * Node shape: a file has `path` (+ `name`); a folder has `children` (+
   * `name`) and no `path`. Presence of `node.path` is the file/folder
   * discriminator.
   */
  import { goChapter } from '../lib/stores.js';
  import { numFromName, shortTitle, labelFromName } from '../lib/index.js';
  import Self from './TreeNode.svelte';

  let { node, depth = 0 } = $props();
  // Folders start expanded; each node owns its own toggle state.
  let expanded = $state(true);

  // Leading chapter number for the file row; 999 sentinel = "no number"
  // (folders, or files without a parseable ch.NN), rendered as blank below.
  const num = $derived(node.path ? numFromName(node.name) : 999);
</script>

{#if node.path}
  <div
    class="chapter-item clickable"
    style="padding-left: {depth * 20}px"
    onclick={() => goChapter(node.path)}
    role="button"
    tabindex="0"
  >
    <span class="chapter-num">{num < 999 ? num : ''}</span>
    <span class="chapter-item-title">{shortTitle(node.name)}</span>
    <span class="chapter-item-arrow">→</span>
  </div>
{:else}
  <div class="folder-item {expanded ? 'expanded' : ''}" onclick={() => (expanded = !expanded)} role="button" tabindex="0">
    <span class="folder-item-arrow">▶</span>
    <span class="folder-item-icon">📁</span>
    <span class="folder-item-name">{labelFromName(node.name)}</span>
  </div>
  <div class="folder-children {expanded ? '' : 'collapsed'}">
    {#each node.children || [] as child (child.path || child.folderPath)}
      <Self node={child} depth={depth + 1} />
    {/each}
  </div>
{/if}
