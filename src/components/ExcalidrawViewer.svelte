<script>
  // ExcalidrawViewer — React island that mounts @excalidraw/excalidraw into a
  // Svelte-owned div.
  //
  // Single responsibility: render one ```excalidraw fenced block from a chapter
  // as a read-only inline preview, and provide an in-app editor to modify it.
  // It owns the React lifecycle (createRoot / unmount) for the foreign tree;
  // Svelte never touches the DOM inside `host` / `editorHost` once React owns it.
  //
  // Collaborators:
  //   - lib/tauri.js: updateExcalidrawBlock() writes a scene back into the .md
  //     file by block index; saveBytes() exports a standalone .excalidraw file.
  //   - lib/stores.js folderRoot: the currently-open book folder, the root the
  //     relPath is resolved against on save.
  //   - the markdown renderer that emits this component with json/relPath/
  //     blockIndex props, one instance per excalidraw block.
  //
  // Two modes:
  //   - inline (default): read-only preview, lazily mounted when visible.
  //     Reflects the *current* scene (after saves / in-progress edits).
  //   - modal: full Excalidraw editor. Edits update `currentScene` live so the
  //     user sees the result as they draw, and Save writes the scene back to
  //     the .md file by block index (re-reads the file each time, so multiple
  //     saves in a row work).
  //
  // Key invariants / assumptions a maintainer must know:
  //   - `json` is parsed exactly once (onMount). `currentScene` is the live
  //     source of truth thereafter; the editor's internal state is authoritative
  //     while the modal is open and flows back via onChange.
  //   - blockIndex is the scene's position among excalidraw blocks in the file;
  //     it MUST stay stable for save to target the right block. The Rust side
  //     re-reads the file per save, so concurrent edits to the same chapter from
  //     elsewhere can clobber.
  //   - relPath is untrusted file input but is resolved under folderRoot by the
  //     Rust command, which enforces the path-traversal boundary.
  //
  // React + Excalidraw are dynamic-imported so the bundle only grows when
  // the user actually opens a chapter with an Excalidraw block.
  import { onMount, onDestroy, tick } from 'svelte';
  import { TAURI, updateExcalidrawBlock, saveBytes } from '../lib/tauri.js';
  import { folderRoot } from '../lib/stores.js';
  import { get } from 'svelte/store';

  // Props:
  //   json       — raw scene as the fenced-block text (JSON string) or an object.
  //   dark       — true in dark theme; selects the React theme and the default
  //                scene background when none is stored.
  //   relPath    — chapter path relative to folderRoot; '' disables save-to-file
  //                (e.g. browser mode with no Tauri backend).
  //   label      — optional caption shown above the preview and in the print note.
  //   blockIndex — 0-based index of this scene among the file's excalidraw blocks;
  //                the save target. See header invariant.
  let {
    json = '',
    dark = false,
    relPath = '',
    label = '',
    blockIndex = 0,
  } = $props();

  let host;                       // Svelte-owned DOM node (inline preview)
  let root;                       // React root handle for the inline preview
  let ExcalidrawComp;             // lazy-loaded React component
  let loading = $state(true);
  let error = $state('');
  let mounted = $state(false);
  let reactTheme = $derived(dark ? 'dark' : 'light');

  // The scene that drives the inline preview. Starts as the prop, gets
  // updated by the editor on every change, and gets updated explicitly
  // when the user saves. The inline preview re-renders off this state.
  let currentScene = $state(null);

  // Modal state.
  let editorOpen = $state(false);
  let editorHost = $state(null);
  let editorRoot = null;
  let editorExcalidrawRef = null;
  let saveBusy = $state(false);
  let saveError = $state('');

  /** Normalize raw block text/object into a complete Excalidraw scene.
   *  Accepts a JSON string or an already-parsed object and fills in the
   *  fields Excalidraw requires, so a sparse block (just `elements`) still
   *  renders. Throws on non-object input or a missing `elements` array — the
   *  caller (onMount) turns the throw into the inline error state.
   *  @param {string|object} raw  block text or parsed scene
   *  @returns {object} a scene with type/version/source/elements/appState/files */
  function parseScene(raw) {
    const parsed = typeof raw === 'string' ? JSON.parse(raw) : raw;
    if (!parsed || typeof parsed !== 'object') throw new Error('not a JSON object');
    if (!Array.isArray(parsed.elements)) throw new Error('missing `elements` array');
    return {
      type: parsed.type || 'excalidraw',
      version: parsed.version || 2,
      source: parsed.source || 'https://excalidraw.com',
      elements: parsed.elements,
      appState: parsed.appState || { gridSize: null, viewBackgroundColor: dark ? '#1c1c1e' : '#ffffff' },
      files: parsed.files || {},
    };
  }

  // ---------------------------------------------------------------------------
  // Inline preview
  // ---------------------------------------------------------------------------

  /** Lazily import React + Excalidraw and mount the read-only preview into
   *  `host`. Idempotent via the `mounted` guard and a no-op until the scene
   *  has parsed, so the IntersectionObserver may fire it speculatively.
   *  Failure is surfaced in the inline `error` state, not thrown. */
  async function mountInline() {
    if (!host || mounted || !currentScene) return;
    try {
      const [{ Excalidraw }, { createElement }, { createRoot }] = await Promise.all([
        import('@excalidraw/excalidraw'),
        import('react'),
        import('react-dom/client'),
      ]);
      await import('@excalidraw/excalidraw/index.css');
      ExcalidrawComp = Excalidraw;

      root = createRoot(host);
      renderPreview();
      mounted = true;
    } catch (e) {
      console.error('[Excalidraw inline]', e);
      error = e.message || String(e);
    }
  }

  /** Render (or re-render) the inline preview from `currentScene`. Safe to
   *  call repeatedly; React reconciles the same tree against new initialData. */
  function renderPreview() {
    if (!root || !ExcalidrawComp) return;
    // Re-render the inline preview off the current scene. Cheap — same
    // React tree, just new initialData (Excalidraw memoizes internally).
    // Importing here to avoid pulling React into the module top-level.
    import('react').then(({ createElement }) => {
      if (!root) return;
      root.render(
        createElement(ExcalidrawComp, {
          initialData: currentScene,
          viewModeEnabled: true,
          zenModeEnabled: true,
          gridModeEnabled: false,
          theme: reactTheme,
          name: 'fenceymd-preview',
          width: '100%',
          height: 480,
        })
      );
    });
  }

  // Whenever currentScene changes, refresh the inline preview.
  $effect(() => {
    // Explicitly read currentScene + reactTheme so Svelte tracks them as deps
    // and re-runs on either change; renderPreview() reads them indirectly
    // (through React props) where the tracker wouldn't otherwise see them.
    // This effect only *reads* these and writes nothing reactive, so it can't
    // self-trigger a loop.
    void currentScene;
    void reactTheme;
    if (mounted) renderPreview();
  });

  /** Svelte action: mount the inline preview the first time `node` scrolls into
   *  view (then stop observing), keeping React/Excalidraw out of the bundle path
   *  and off the main thread until needed. Falls back to an immediate microtask
   *  mount where IntersectionObserver is unavailable. */
  function lazyMount(node) {
    if (typeof IntersectionObserver === 'undefined') {
      queueMicrotask(mountInline);
      return { destroy: () => {} };
    }
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) { mountInline(); io.disconnect(); break; }
      }
    });
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }

  // ---------------------------------------------------------------------------
  // Modal editor
  // ---------------------------------------------------------------------------

  /** Open the editor modal. Actual React mount is deferred to the $effect
   *  below, which waits for the modal's `editorHost` node to exist. */
  function openEditor() {
    saveError = '';
    editorOpen = true;
  }

  // Mount the editor React tree once the host DOM node is in the modal.
  // editorMounted is a plain (non-reactive) latch, not $state: it guards the
  // one-time mount but must NOT itself be a tracked dependency, or setting it
  // inside the effect would re-trigger the effect. The effect reads editorOpen
  // and editorHost (reactive) and writes only the latch + calls mountEditor.
  let editorMounted = false;
  $effect(() => {
    if (editorOpen && editorHost && !editorMounted) {
      editorMounted = true;
      mountEditor();
    }
    if (!editorOpen) editorMounted = false;
  });

  /** Lazily import React + Excalidraw and mount the full editor into
   *  `editorHost`. Wires onChange so live edits flow back into currentScene
   *  (driving the inline preview underneath). loadScene/saveToActiveFile are
   *  disabled because persistence is owned by this component, not Excalidraw's
   *  own file actions. Failure surfaces in `saveError`, not thrown. */
  async function mountEditor() {
    if (!editorHost || !currentScene) return;
    try {
      const [{ Excalidraw }, { createElement }, { createRoot }] = await Promise.all([
        import('@excalidraw/excalidraw'),
        import('react'),
        import('react-dom/client'),
      ]);
      await import('@excalidraw/excalidraw/index.css');
      ExcalidrawComp = Excalidraw;
      const ref = { current: null };
      editorExcalidrawRef = ref;
      editorRoot = createRoot(editorHost);

      // onChange updates our local state immediately. The editor's
      // internal state is the source of truth while the modal is open.
      const handleChange = (elements, appState) => {
        currentScene = {
          ...currentScene,
          elements,
          appState: { ...(appState || currentScene.appState) },
        };
      };

      editorRoot.render(
        createElement(ExcalidrawComp, {
          excalidrawRef: ref,
          initialData: currentScene,
          theme: reactTheme,
          name: 'fenceymd-editor',
          onChange: handleChange,
          UIOptions: { canvasActions: { loadScene: false, saveToActiveFile: false } },
        })
      );
    } catch (e) {
      console.error('[Excalidraw editor]', e);
      saveError = e.message || String(e);
    }
  }

  /** Tear down the editor: unmount React, drop refs, and revert any unsaved
   *  in-progress edits. Used both for Cancel and as the final step of a
   *  successful save (saveToChapter has already advanced lastSavedScene by then,
   *  so the revert is a no-op there). */
  function closeEditor() {
    // Revert currentScene to the original (last-saved) scene so the inline
    // preview doesn't show the user's in-progress edits after they cancel.
    if (lastSavedScene) currentScene = lastSavedScene;
    try { editorRoot?.unmount(); } catch (_) {}
    editorRoot = null;
    editorExcalidrawRef = null;
    editorOpen = false;
    saveError = '';
  }

  // The last successfully-saved scene. Used to revert on cancel.
  let lastSavedScene = $state(null);

  /** Build the canonical Excalidraw JSON payload from the current scene.
   *  We only keep `viewBackgroundColor` from the editor's full appState —
   *  everything else (scroll position, hover, tool selection, runtime
   *  stats) is editor UI state, not part of the scene. Persisting it
   *  bloats the file and churns diffs. */
  function buildPayload() {
    return {
      type: 'excalidraw',
      version: 2,
      source: 'https://excalidraw.com',
      elements: currentScene?.elements || [],
      appState: {
        gridSize: null,
        viewBackgroundColor: currentScene?.appState?.viewBackgroundColor
          || (dark ? '#1c1c1e' : '#ffffff'),
      },
      files: currentScene?.files || {},
    };
  }

  /** Save the current scene back into the chapter's .md file. The chapter
   *  updates and the next time the user opens it, the new scene is what
   *  they see. */
  async function saveToChapter() {
    saveError = '';
    if (!currentScene) { saveError = 'editor not ready'; return; }
    if (!relPath) { saveError = 'no file path; cannot save (browser mode?)'; return; }
    const folder = get(folderRoot);
    if (!folder) { saveError = 'no folder open'; return; }

    saveBusy = true;
    try {
      const payload = buildPayload();
      const newInner = JSON.stringify(payload, null, 2);
      await updateExcalidrawBlock(folder, relPath, blockIndex, newInner);
      lastSavedScene = { ...payload };
      currentScene = { ...payload };
      closeEditor();
    } catch (e) {
      console.error('[Excalidraw save]', e);
      saveError = e.message || String(e);
    } finally {
      saveBusy = false;
    }
  }

  /** Save the current scene as a standalone .excalidraw file. Opens the
   *  native save dialog so the user picks where to put it. The chapter
   *  is not touched — this is a portable export. */
  async function saveAsFile() {
    saveError = '';
    if (!currentScene) { saveError = 'editor not ready'; return; }

    saveBusy = true;
    try {
      const payload = buildPayload();
      const json = JSON.stringify(payload, null, 2);
      // Derive a default filename: <chapter>-scene-<N>.excalidraw
      const chapterBase = (relPath || 'scene')
        .split('/').pop()               // drop folders
        .replace(/\.md$/i, '');         // drop .md extension
      const defaultName = `${chapterBase}-scene-${blockIndex + 1}.excalidraw`;
      const encoder = new TextEncoder();
      const saved = await saveBytes(defaultName, encoder.encode(json));
      if (!saved) {
        // User cancelled the dialog — no error, just leave the modal open.
        return;
      }
      closeEditor();
    } catch (e) {
      console.error('[Excalidraw save-as]', e);
      saveError = e.message || String(e);
    } finally {
      saveBusy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  // Parse the prop exactly once and seed both the live scene and the
  // revert baseline. A parse failure here is the only path to the inline
  // `error` state; mounting is gated on a valid currentScene.
  onMount(() => {
    try {
      currentScene = parseScene(json);
      lastSavedScene = { ...currentScene };
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  });

  // Unmount both React roots so they don't outlive the Svelte component and
  // leak. Svelte won't tear down a foreign React tree for us; guards swallow
  // the "already unmounted" case (e.g. editor closed before destroy).
  onDestroy(() => {
    try { root?.unmount(); } catch (_) {}
    try { editorRoot?.unmount(); } catch (_) {}
    root = null;
    editorRoot = null;
  });
</script>

<div class="excalidraw-wrap">
  {#if label}
    <div class="excalidraw-label">Excalidraw {label}</div>
  {/if}
  <div class="excalidraw-print-note">Excalidraw diagram{label ? ` — ${label}` : ''} (open in app to view)</div>
  <div class="excalidraw-host" bind:this={host} use:lazyMount>
    {#if loading}
      <div class="excalidraw-placeholder">Preparing Excalidraw…</div>
    {:else if error}
      <div class="excalidraw-error">Invalid Excalidraw JSON: {error}</div>
    {:else if !mounted}
      <div class="excalidraw-placeholder">Scroll into view to load Excalidraw preview</div>
    {/if}
  </div>
  <button class="excalidraw-edit-btn" type="button" onclick={openEditor} title="Edit this Excalidraw scene in the app">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
      <path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4z"/>
    </svg>
    Edit
  </button>
</div>

{#if editorOpen}
  <div class="excalidraw-modal-backdrop" onclick={closeEditor} role="presentation"></div>
  <div class="excalidraw-modal" role="dialog" aria-modal="true" aria-label="Edit Excalidraw scene">
    <div class="excalidraw-modal-header">
      <h3>Edit Excalidraw scene</h3>
      <div class="excalidraw-modal-actions">
        <button type="button" class="excalidraw-btn-secondary" onclick={closeEditor} disabled={saveBusy}>Cancel</button>
        <button type="button" class="excalidraw-btn-tertiary" onclick={saveAsFile} disabled={saveBusy} title="Export the current scene as a standalone .excalidraw file">
          Save as file
        </button>
        <button type="button" class="excalidraw-btn-primary" onclick={saveToChapter} disabled={saveBusy}>
          {saveBusy ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
    {#if saveError}
      <div class="excalidraw-error" style="margin: 0.5rem 0.75rem;">{saveError}</div>
    {/if}
    <div class="excalidraw-modal-body" bind:this={editorHost}></div>
  </div>
{/if}

<style>
  .excalidraw-wrap { position: relative; margin: var(--space-4, 1rem) 0; }
  .excalidraw-wrap:hover .excalidraw-edit-btn { opacity: 1; }
  .excalidraw-print-note { display: none; }

  .excalidraw-host {
    width: 100%;
    height: 480px;
    background: var(--surface-container-lowest);
    border: 1px solid var(--surface-variant);
    border-radius: var(--radius-md, 4px);
    overflow: hidden;
  }
  .excalidraw-host :global(.excalidraw) {
    width: 100% !important;
    height: 100% !important;
  }
  .excalidraw-placeholder {
    padding: 2rem;
    text-align: center;
    color: var(--ink-muted);
    font-family: var(--font-sans);
    font-size: 0.9rem;
  }
  .excalidraw-error {
    padding: 1rem;
    color: #b00020;
    font-family: monospace;
    font-size: 0.85rem;
    background: var(--tertiary-dim);
    border-left: 3px solid var(--tertiary);
  }
  .excalidraw-label {
    display: inline-block;
    margin-bottom: 0.4rem;
    padding: 2px 8px;
    font-family: var(--font-sans);
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ink-secondary);
    background: var(--tertiary-dim);
    border: 1px solid var(--tertiary);
    border-radius: var(--radius-sm, 2px);
  }
  .excalidraw-edit-btn {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.6rem;
    font-family: var(--font-sans);
    font-size: 0.75rem;
    font-weight: 500;
    background: var(--surface);
    color: var(--ink-secondary);
    border: 1px solid var(--surface-variant);
    border-radius: var(--radius-sm, 2px);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .excalidraw-edit-btn:hover {
    background: var(--tertiary-dim);
    color: var(--tertiary);
    border-color: var(--tertiary);
  }

  .excalidraw-modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 200;
  }
  .excalidraw-modal {
    position: fixed;
    top: 3vh;
    left: 3vw;
    right: 3vw;
    bottom: 3vh;
    background: var(--surface);
    border-radius: var(--radius-lg, 8px);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.25);
    z-index: 201;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .excalidraw-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--surface-variant);
  }
  .excalidraw-modal-header h3 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 1rem;
    font-weight: 600;
  }
  .excalidraw-modal-actions {
    display: flex;
    gap: 0.5rem;
  }
  .excalidraw-btn-secondary,
  .excalidraw-btn-primary {
    padding: 0.4rem 0.8rem;
    font-family: var(--font-sans);
    font-size: 0.85rem;
    border-radius: var(--radius-sm, 2px);
    cursor: pointer;
    border: 1px solid var(--surface-variant);
  }
  .excalidraw-btn-secondary {
    background: var(--surface);
    color: var(--ink-secondary);
  }
  .excalidraw-btn-tertiary {
    background: var(--surface);
    color: var(--ink);
    border-color: var(--ink-muted);
  }
  .excalidraw-btn-tertiary:hover {
    background: var(--tertiary-dim);
    border-color: var(--tertiary);
    color: var(--tertiary);
  }
  .excalidraw-btn-primary {
    background: var(--tertiary);
    color: white;
    border-color: var(--tertiary);
  }
  .excalidraw-btn-secondary:disabled,
  .excalidraw-btn-tertiary:disabled,
  .excalidraw-btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Hide the Excalidraw library sidebar. The in-app editor doesn't need
     it (we don't load .excalidraw files), and it overflows the modal
     when expanded, getting clipped on the right. */
  .excalidraw-modal-body :global(.sidebar-trigger),
  .excalidraw-modal-body :global(.library-sidebar),
  .excalidraw-modal-body :global(.library-unit) {
    display: none !important;
  }
  .excalidraw-modal-body {
    flex: 1;
    overflow: hidden;
  }
</style>
