<script>
  // Auto-TOC outline pane (ROADMAP v1.1 #3).
  //
  // Walks the live chapter DOM for h1/h2 (and h3 if the chapter has few
  // h1/h2) and renders them as a clickable outline. Click jumps via
  // smooth-scroll. The most recently scrolled-past heading is the
  // "active" one and gets a highlight via IntersectionObserver.
  //
  // Visibility + auto-resolve-from-viewport is owned by the parent
  // (Reader), which passes `visible` here. We don't read the store
  // directly — keeps the component testable and pure.
  import { onMount, tick } from 'svelte';

  let { mdEl = null, visible = $bindable(false), onclose = () => {} } = $props();

  // Reactive list of outline entries. Each entry: { level, text, anchor }
  let entries = $state([]);

  // Anchor of the currently-active heading (the most recently scrolled
  // past). null until the user scrolls.
  let activeAnchor = $state(null);

  // The pane element (used by IntersectionObserver and click-scroll).
  let paneEl = $state(null);
  let listEl = $state(null);

  // Re-walk the chapter DOM whenever the markdown element changes (i.e.
  // when the user navigates to a new chapter) or after `enhance()` swaps
  // the innerHTML. We do this in a $effect so Svelte tracks mdEl.
  $effect(() => {
    if (!mdEl) { entries = []; return; }
    // Re-collect after the next tick so freshly-rendered headings are
    // in the DOM.
    let cancelled = false;
    (async () => {
      await tick();
      if (cancelled) return;
      entries = collectEntries(mdEl);
      activeAnchor = null;
      setupObserver();
    })();
    return () => { cancelled = true; };
  });

  // Walk mdEl and build a flat list of outline entries.
  // Optional h3 inclusion: if the chapter has < 4 h1+h2 entries total,
  // pull in the h3s as well so the pane doesn't look empty in short
  // chapters.
  function collectEntries(root) {
    if (!root) return [];
    const h1h2 = [...root.querySelectorAll('h1, h2')].filter(isVisibleHeading);
    const useH3 = h1h2.length < 4;
    const sel = useH3 ? 'h1, h2, h3' : 'h1, h2';
    const heads = [...root.querySelectorAll(sel)].filter(isVisibleHeading);

    // Assign stable anchors if not already present. The anchor-infra
    // task (ROADMAP #23) will hand us `data-md-anchor`; until then we
    // generate them ourselves so the pane works today.
    let h1n = 0, h2n = 0, h3n = 0;
    const out = [];
    for (const el of heads) {
      const level = el.tagName === 'H1' ? 1 : el.tagName === 'H2' ? 2 : 3;
      const existing = el.getAttribute('data-md-anchor');
      let anchor;
      if (existing) {
        anchor = existing;
      } else {
        if (level === 1) { h1n += 1; anchor = 'h1-' + h1n; }
        else if (level === 2) { h2n += 1; anchor = 'h2-' + h2n; }
        else { h3n += 1; anchor = 'h3-' + h3n; }
        el.setAttribute('data-md-anchor', anchor);
      }
      // Make anchor addressable for hash links + scrollIntoView.
      if (!el.id) el.id = anchor;
      out.push({
        level,
        anchor,
        text: (el.textContent || '').trim().replace(/\s+/g, ' '),
      });
    }
    return out;
  }

  // Showdown sometimes produces an empty h1/h2 placeholder inside the
  // rendered HTML — filter those out so the outline doesn't show blank
  // rows. Also skip the very first h1 (the chapter title) since the
  // reader already shows it in its own editorial header.
  function isVisibleHeading(el) {
    const text = (el.textContent || '').trim();
    if (!text) return false;
    if (el.offsetParent === null && el.tagName === 'H1') {
      // Hidden h1s are common when the markdown starts with a # title
      // that the reader's editorial header also shows. Reader.svelte
      // strips the leading <h1> from `bodyHtml`, so by the time we get
      // here those are already gone; this is a defensive check.
      return false;
    }
    return true;
  }

  // IntersectionObserver: pick the heading whose top is closest to
  // (but not below) the top of the viewport. This is more reliable
  // than "the last entry that crossed the threshold" because of how
  // sticky toolbars offset the visible region.
  let observer = null;
  function setupObserver() {
    if (observer) { observer.disconnect(); observer = null; }
    if (typeof IntersectionObserver === 'undefined' || !mdEl) return;

    const targets = entries
      .map((e) => document.getElementById(e.anchor))
      .filter(Boolean);
    if (!targets.length) return;

    // The "activation line" sits just below the sticky toolbar. We use
    // rootMargin to push the viewport top down ~80px so a heading
    // counts as "active" only once it has scrolled under the toolbar.
    observer = new IntersectionObserver(() => {
      activeAnchor = pickActive(targets);
    }, {
      // -80px top: heading must clear the toolbar before activating.
      // -50% bottom: heading must be at least halfway into the lower
      // half of the viewport.
      rootMargin: '-80px 0px -50% 0px',
      threshold: [0, 0.25, 0.5, 1],
    });
    for (const t of targets) observer.observe(t);
  }

  // Walk the targets in document order and return the anchor of the
  // last heading whose top is above the activation line. If the user
  // is scrolled past every heading, the last one is still active (it
  // represents the "section the reader is in").
  function pickActive(targets) {
    const line = 80; // matches rootMargin top
    let chosen = null;
    for (const t of targets) {
      const rect = t.getBoundingClientRect();
      if (rect.top <= line) chosen = t.id;
      else break;
    }
    return chosen;
  }

  function onClickEntry(e, anchor) {
    e.preventDefault();
    const target = document.getElementById(anchor);
    if (!target) return;
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    activeAnchor = anchor;
    // Update the URL hash without triggering a scroll jump.
    if (history && history.replaceState) {
      history.replaceState(null, '', '#' + anchor);
    }
  }

  // When the user scrolls manually, the observer's callback updates
  // activeAnchor. When the user clicks an entry we already set it. So
  // there's no explicit window.scroll listener needed here.

  // ── Outside-click / Escape to close (mobile overlay only) ──
  function onWindowKey(e) {
    if (!visible) return;
    if (e.key === 'Escape') onclose();
  }
  onMount(() => {
    window.addEventListener('keydown', onWindowKey);
    return () => window.removeEventListener('keydown', onWindowKey);
  });
</script>

<aside
  class="outline-pane"
  class:visible
  bind:this={paneEl}
  aria-label="Chapter outline"
  aria-hidden={!visible}
>
  <div class="outline-pane-header">
    <span class="outline-pane-title">On this page</span>
    <button
      class="outline-pane-close"
      type="button"
      onclick={onclose}
      aria-label="Hide outline"
      title="Hide outline (⌘\)"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="18" y1="6" x2="6" y2="18"/>
        <line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
    </button>
  </div>
  {#if entries.length === 0}
    <div class="outline-pane-empty">This chapter has no headings.</div>
  {:else}
    <ul class="outline-pane-list" bind:this={listEl}>
      {#each entries as e (e.anchor)}
        <li class="outline-pane-item level-{e.level}" class:active={activeAnchor === e.anchor}>
          <a
            href={'#' + e.anchor}
            onclick={(ev) => onClickEntry(ev, e.anchor)}
            title={e.text}
          >{e.text}</a>
        </li>
      {/each}
    </ul>
  {/if}
</aside>
