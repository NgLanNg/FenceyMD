<script>
  // Fullscreen zoom overlay for any zoomable block (images, tables, diagrams,
  // Excalidraw scenes). State is owned by zoom-state.js; this component is
  // a thin Svelte wrapper that:
  //   1. Subscribes to the entry and renders the overlay when set
  //   2. On open, moves the target's DOM into .zoom-host via a microtask
  //   3. On close, lets zoom-state.js restore the node (it queues the
  //      move-back on a microtask after the entry is cleared)
  //   4. Closes on Esc and on backdrop click
  //
  // Mounted once in Reader.svelte.
  import { onMount } from 'svelte';
  import { getEntry, subscribe, closeZoom } from './zoom-state.js';

  let entry = $state(getEntry());
  let host = $state(null);

  // Subscribe to the module-level state. Svelte's $effect tracks the
  // returned cleanup function.
  //
  // Hazard note: this effect has NO reactive deps — it reads/writes `entry`
  // only inside the subscribe callback, not in the effect body — so it runs
  // exactly once (mount) and tears down on unmount. That's deliberate: the
  // subscription, not Svelte reactivity, is what drives `entry`. The
  // `wasOpen` snapshot is taken before the assignment so we can detect the
  // closed→open edge and move the node in only on that transition.
  $effect(() => {
    return subscribe((next) => {
      const wasOpen = !!entry;
      entry = next;
      const isOpen = !!entry;
      // After the Svelte DOM update, the {#if} block has rendered
      // (or cleared) the .zoom-host. Move the node in or let
      // zoom-state.js move it back.
      if (isOpen && !wasOpen) {
        // Open: move the target node into the freshly-rendered host.
        queueMicrotask(() => {
          if (host && entry?.node && entry.node.parentNode !== host) {
            host.appendChild(entry.node);
          }
        });
      }
    });
  });

  /** Close only when the click lands on the backdrop itself, not on the
   *  zoomed content bubbling up — hence the target === currentTarget guard. */
  function onBackdropClick(e) {
    if (e.target === e.currentTarget) closeZoom();
  }
  /** Esc closes the overlay. Gated on `entry` so the global listener is a
   *  no-op while nothing is zoomed. */
  function onKeydown(e) {
    if (e.key === 'Escape' && entry) closeZoom();
  }
  // Esc is bound at the window level (not on the overlay) so it works
  // regardless of focus; cleanup removes it on unmount.
  onMount(() => {
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });
</script>

{#if entry}
  <div
    class="zoom-overlay"
    role="dialog"
    aria-modal="true"
    aria-label="Zoomed view"
    onclick={onBackdropClick}
  >
    <div class="zoom-surface">
      <button class="zoom-close" onclick={closeZoom} aria-label="Close zoom" title="Close (Esc)">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18"/>
          <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
      <!-- The target node is moved here imperatively. -->
      <div class="zoom-host" bind:this={host} data-zoom-host></div>
    </div>
  </div>
{/if}
