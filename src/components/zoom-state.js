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

// `_entry` is the single source of truth: either null (overlay closed) or
// the descriptor of the node currently lifted into the overlay. `_subs`
// holds the subscriber callbacks (in practice just the mounted ZoomOverlay).
let _entry = null;
const _subs = new Set();

// Fan out the current `_entry` to every subscriber. Called after every
// mutation so subscribers never read stale state.
function notify() {
  for (const fn of _subs) fn(_entry);
}

/**
 * Read the current entry without subscribing.
 * @returns {?{node: Node, parent: Node, next: ?Node}} the active entry, or
 *   null when the overlay is closed. Used for initial-state reads on mount.
 */
export function getEntry() { return _entry; }

/**
 * Subscribe to entry changes. `fn` fires immediately on every subsequent
 * mutation with the new entry (or null on close), but NOT synchronously at
 * subscribe time — call getEntry() for the initial value.
 * @param {(entry: ?object) => void} fn subscriber callback.
 * @returns {() => void} unsubscribe; the caller MUST invoke this on unmount
 *   or the closure (and its captured DOM) leaks in the module-level set.
 */
export function subscribe(fn) {
  _subs.add(fn);
  return () => _subs.delete(fn);
}

/**
 * Open the zoom overlay with `node` as the moved content. The node's
 * original parent + next-sibling are remembered so closeZoom() can
 * restore it.
 *
 * No-ops when `node` is missing or detached (no parentNode): a detached
 * node has nowhere to be restored to, so opening it would strand it.
 * @param {Node} node the live DOM element to lift into the overlay.
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
