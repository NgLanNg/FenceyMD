// Module-level state for the ZoomOverlay component. Svelte 5 components
// can't export named functions reliably across rollup's CJS/ESM boundaries,
// so the state lives here in a plain JS module and the ZoomOverlay
// component subscribes to changes.
//
// The Svelte component owns the DOM (the overlay element, the .zoom-host
// div) and the lifecycle (mount/unmount). This module owns the data:
// what's currently being shown.
//
// Pattern: tiny pub/sub. The Svelte component subscribes on mount and
// updates `entry` state when the value changes. `openZoom(node)` is the
// only mutator.

let _entry = null;
const _subs = new Set();

function notify() {
  for (const fn of _subs) fn(_entry);
}

export function getEntry() { return _entry; }
export function subscribe(fn) {
  _subs.add(fn);
  return () => _subs.delete(fn);
}

/**
 * Open the zoom overlay with `node` as the moved content. The node's
 * original parent + next-sibling are remembered so closeZoom() can
 * restore it.
 */
export function openZoom(node) {
  if (!node || !node.parentNode) return;
  _entry = {
    node,
    parent: node.parentNode,
    next: node.nextSibling,
  };
  notify();
}

/** Close the overlay and restore the moved node to its original spot. */
export function closeZoom() {
  if (!_entry) return;
  const e = _entry;
  _entry = null;
  notify();
  // Restore happens on a microtask so the {#if entry} Svelte block
  // clears the .zoom-host first; the node then goes back to its
  // original parent.
  queueMicrotask(() => {
    if (!e.node) return;
    if (e.next) e.parent.insertBefore(e.node, e.next);
    else e.parent.appendChild(e.node);
  });
}
