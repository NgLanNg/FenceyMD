<script>
  // ─────────────────────────────────────────────────────────────────────
  // OutlinePane.svelte — Auto-TOC outline pane (ROADMAP v1.1 #3).
  //
  // RESPONSIBILITY: derive a clickable "On this page" outline from the
  // *live* rendered chapter DOM and keep one entry highlighted as the
  // reader scrolls (read mode) or moves the caret (edit mode). It owns no
  // document state of its own — it is a projection of whatever HTML the
  // Reader currently has mounted.
  //
  // HOW IT WORKS: walks `mdEl` for h1/h2 (and h3 if the chapter has few
  // h1/h2) and renders them as a flat list. Click jumps via smooth-scroll.
  // In read mode an IntersectionObserver tracks the most recently
  // scrolled-past heading; in edit mode a `paragraph-focus` CustomEvent
  // (from Track 1, the Editor) drives the highlight instead.
  //
  // COLLABORATORS:
  //   - Reader.svelte owns visibility + auto-resolve-from-viewport and
  //     passes `mdEl` and `visible` down. We deliberately do NOT read the
  //     UI store directly — keeps this component pure/testable.
  //   - Editor (Track 1) dispatches `paragraph-focus` on `document`; we
  //     listen on `document` so we never need a ref to the editor.
  //
  // INVARIANTS / ASSUMPTIONS A MAINTAINER MUST KNOW:
  //   - `mdEl` is the rendered chapter container; its headings may be
  //     swapped out wholesale by enhance() — hence the re-walk on change.
  //   - Anchors are sourced from `data-md-anchor` when present and
  //     generated locally otherwise (see collectEntries). The generated
  //     scheme is positional, so it is only stable WITHIN a single render
  //     of a single chapter — never persist a generated anchor.
  //   - Reader.svelte strips the leading <h1> chapter title before we see
  //     the HTML, so the first visible h1 here is usually a real section.
  // ─────────────────────────────────────────────────────────────────────
  import { onMount, tick } from 'svelte';

  /**
   * Props (the component's only public surface).
   * @prop {HTMLElement|null} mdEl     Rendered chapter container to walk.
   *                                   Changing it triggers a full re-walk.
   * @prop {boolean}          visible  Bindable. Owned by Reader; gates the
   *                                   mobile overlay and the Escape handler.
   * @prop {() => void}       onclose  Invoked by the close button / Escape.
   */
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
  //
  // $effect HAZARD: this effect WRITES `entries` and `activeAnchor`, both
  // of which other code reads. It is safe from a re-run loop because the
  // only tracked read here is `mdEl` (a prop) — the writes happen inside
  // the async IIFE after `await tick()`, by which point reactive tracking
  // for this effect has already closed, so assigning `entries` does not
  // register this effect as its own dependent. collectEntries/setupObserver
  // read `entries`/`mdEl` imperatively, not reactively, so they likewise
  // can't retrigger us. Do NOT move those reads to the synchronous head of
  // the effect or you'll create a write-then-read self-dependency.
  //
  // The `cancelled` guard handles the race where `mdEl` changes again
  // before the awaited tick resolves: a stale walk must not clobber the
  // entries for the newer chapter.
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
  //
  // @param {HTMLElement|null} root  Chapter container to scan.
  // @returns {Array<{level:1|2|3, anchor:string, text:string}>} entries in
  //   document order; [] for a null root or a chapter with no headings.
  //
  // SIDE EFFECT (intentional): this also MUTATES the DOM — it stamps a
  // `data-md-anchor` and (if missing) an `id` onto each heading so the
  // anchors it returns are actually addressable for hash links and
  // scrollIntoView. Callers must treat collectEntries as the single owner
  // of generated anchors; calling it twice on the same DOM is idempotent
  // because existing `data-md-anchor`/`id` values are reused.
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
  //
  // @param {HTMLElement} el  A candidate heading element.
  // @returns {boolean} true if the heading should appear in the outline.
  // Edge case: relies on `offsetParent === null` to detect hidden h1s,
  // which is a layout read (not text) — accurate only once the element is
  // attached and laid out, which is why the walk runs after `await tick()`.
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
  //
  // Idempotent: disconnects any prior observer first, so it is safe to
  // call on every re-walk. No-ops in non-DOM/test environments (guards on
  // `typeof IntersectionObserver`) and when there are no resolvable anchors.
  // Reads `entries` imperatively — see the $effect hazard note above.
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
  //
  // @param {HTMLElement[]} targets  Heading elements in document order.
  // @returns {string|null} the active heading's id, or null when every
  //   heading is still below the activation line (top of chapter).
  // The early `break` is the load-bearing bit: targets are pre-sorted by
  // document position, so once one is below the line all later ones are
  // too — we stop rather than scanning the whole list each callback.
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

  // Handle a click on an outline entry: prevent the default hash-jump,
  // smooth-scroll to the heading, and optimistically mark it active.
  //
  // @param {Event}  e       The anchor click event.
  // @param {string} anchor  The heading id to scroll to.
  // We set `activeAnchor` here directly (rather than waiting for the
  // observer) so the highlight responds instantly to the click; the
  // observer will reconcile it once the smooth scroll settles.
  function onClickEntry(e, anchor) {
    e.preventDefault();
    const target = document.getElementById(anchor);
    if (!target) return;
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    activeAnchor = anchor;
    // replaceState (not pushState) keeps the deep-link shareable without
    // polluting Back-button history with every outline click.
    if (history && history.replaceState) {
      history.replaceState(null, '', '#' + anchor);
    }
  }

  // When the user scrolls manually, the observer's callback updates
  // activeAnchor. When the user clicks an entry we already set it. So
  // there's no explicit window.scroll listener needed here.

  // ── Outside-click / Escape to close (mobile overlay only) ──
  // Window-level keydown handler. No-ops unless the pane is `visible`, so
  // Escape never steals the key from the reader/editor while the overlay
  // is hidden. Registered/torn down in onMount.
  function onWindowKey(e) {
    if (!visible) return;
    if (e.key === 'Escape') onclose();
  }

  // ROADMAP v1.1 #22 — paragraph tracking in edit mode. Track 1 (Editor)
  // dispatches a `paragraph-focus` CustomEvent on its `editorEl` (the
  // Tiptap wrapper div) with `detail.anchor` set to the `data-md-anchor`
  // string of the enclosing block (e.g. `"para-3"`, `"h2-1"`). We
  // listen on `document` so we don't need a direct ref to editorEl —
  // the event bubbles up. When the editor unmounts (user closes it,
  // navigates away, or saves), we reset the highlight so a stale
  // paragraph doesn't stay "active" in the pane.
  onMount(() => {
    const onFocus = (e) => {
      const anchor = e?.detail?.anchor ?? null;
      activeAnchor = anchor;
    };
    document.addEventListener('paragraph-focus', onFocus);

    // Reset when the editor is gone. .notion-editor-inner is the
    // Tiptap wrapper; its removal coincides with the Editor
    // component's `{#if editing}` teardown.
    const mo = new MutationObserver(() => {
      if (!document.querySelector('.notion-editor-inner')) {
        activeAnchor = null;
      }
    });
    mo.observe(document.body, { childList: true, subtree: true });

    window.addEventListener('keydown', onWindowKey);
    return () => {
      document.removeEventListener('paragraph-focus', onFocus);
      mo.disconnect();
      window.removeEventListener('keydown', onWindowKey);
    };
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
