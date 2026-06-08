<script>
  import { goChapter } from '../lib/stores.js';
  import { numFromName, shortTitle, labelFromName } from '../lib/index.js';
  import Self from './TreeNode.svelte';

  let { node, depth = 0 } = $props();
  let expanded = $state(true);

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
