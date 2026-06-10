<script>
  import { tick } from 'svelte';
  import { runSearch, makeSnippet, getDoc } from '../lib/cross-search.js';
  import { goChapter } from '../lib/stores.js';
  import { crossSearchOpen, crossSearchQuery, pendingInChapterSearch, ready } from '../lib/stores/state.js';

  let inputEl;
  let cursor = $state(0);
  let results = $state([]);
  let trimmed = $derived($crossSearchQuery.trim());

  // Re-run the search whenever the query changes.
  $effect(() => {
    const q = $crossSearchQuery;
    results = runSearch(q);
    cursor = 0;
  });

  // When the panel opens, focus the input + select-all so the user can
  // either type a new query or just hit Enter to jump the first hit.
  $effect(() => {
    if ($crossSearchOpen) {
      tick().then(() => {
        if (inputEl) { inputEl.focus(); inputEl.select(); }
      });
    }
  });

  function close() {
    crossSearchOpen.set(false);
    crossSearchQuery.set('');
  }

  function jumpToResult(idx) {
    const r = results[idx];
    if (!r) return;
    const doc = getDoc(r.id);
    // Route to the chapter + ask the Reader to populate its in-chapter
    // search bar with the same query so the match highlights on mount.
    goChapter(doc.path);
    pendingInChapterSearch.set($crossSearchQuery);
    close();
  }

  function onKey(e) {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      // Shift+Enter = previous, plain Enter = next
      if (results.length === 0) return;
      const next = e.shiftKey
        ? (cursor - 1 + results.length) % results.length
        : (cursor + 1) % results.length;
      cursor = next;
      jumpToResult(cursor);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cursor = (cursor + 1) % Math.max(1, results.length);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = (cursor - 1 + results.length) % Math.max(1, results.length);
      return;
    }
  }

  // (Global ⌘⇧F / Ctrl+Shift+F toggle is owned by App.svelte so the
  // shortcut works from any view and doesn't race with the panel's
  // own close-on-Escape logic. See App.svelte `onAppKey`.)
</script>

<!-- ROADMAP v1.1 #2 — the global ⌘⇧F / Ctrl+⇧F toggle lives on
     App.svelte so the shortcut works from any view. We do NOT
     re-toggle here, otherwise the App's "open" and the panel's
     "close" race on the same keypress and the panel never shows.
     The panel only listens for Escape (handled in `onKey` above). -->

{#if $crossSearchOpen && $ready}
  <div
    class="xsearch-backdrop"
    onclick={close}
    onkeydown={(e) => { if (e.key === 'Escape') close(); }}
    role="presentation"
  ></div>
  <div
    class="xsearch-panel"
    role="dialog"
    aria-label="Cross-chapter search"
    aria-modal="true"
  >
    <div class="xsearch-inputrow">
      <svg class="xsearch-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
      </svg>
      <input
        bind:this={inputEl}
        bind:value={$crossSearchQuery}
        type="text"
        class="xsearch-input"
        placeholder="Search across all chapters…"
        aria-label="Cross-chapter search query"
        spellcheck="false"
        autocomplete="off"
        onkeydown={onKey}
      />
      <span class="xsearch-shortcut">Esc to close</span>
    </div>

    <div class="xsearch-results" role="listbox" aria-label="Search results">
      {#if !trimmed}
        <div class="xsearch-empty">Type to search across all chapters in this book.</div>
      {:else if results.length === 0}
        <div class="xsearch-empty">No matches for <em>{trimmed}</em>.</div>
      {:else}
        {#each results as r, i (r.id)}
          {@const doc = getDoc(r.id)}
          {@const body = doc?.body || ''}
          {@const fence = doc?.fenceText || ''}
          {@const fromBody = body.toLowerCase().includes(trimmed.toLowerCase())}
          {@const snip = fromBody
            ? makeSnippet(body, $crossSearchQuery)
            : makeSnippet(fence || body, $crossSearchQuery)}
          <button
            class="xsearch-result"
            class:active={i === cursor}
            role="option"
            aria-selected={i === cursor}
            onmouseenter={() => (cursor = i)}
            onclick={() => jumpToResult(i)}
          >
            <div class="xsearch-result-title">
              <span class="xsearch-result-name">{doc?.title || doc?.name || r.id}</span>
              <span class="xsearch-result-path">{doc?.path || r.id}</span>
            </div>
            {#if snip.text}
              <div class="xsearch-result-snippet">
                {#if snip.matchStart >= 0}
                  {snip.text.slice(0, snip.matchStart)}<mark>{snip.text.slice(snip.matchStart, snip.matchEnd)}</mark>{snip.text.slice(snip.matchEnd)}
                {:else}
                  {snip.text}
                {/if}
              </div>
            {/if}
          </button>
        {/each}
      {/if}
    </div>

    <div class="xsearch-foot">
      <span class="xsearch-foot-meta">
        {#if trimmed}{results.length} result{results.length === 1 ? '' : 's'}{:else}Ready{/if}
      </span>
      <span class="xsearch-foot-hint">
        <kbd>↑</kbd><kbd>↓</kbd> navigate · <kbd>Enter</kbd> jump · <kbd>Shift</kbd>+<kbd>Enter</kbd> prev · <kbd>Esc</kbd> close
      </span>
    </div>
  </div>
{/if}
