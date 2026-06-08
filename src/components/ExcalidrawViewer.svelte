<script>
  // React island that mounts @excalidraw/excalidraw into a Svelte-owned div.
  //
  // Two modes:
  //   - inline (default): read-only preview, lazily mounted when visible.
  //     Reflects the *current* scene (after saves / in-progress edits).
  //   - modal: full Excalidraw editor. Edits update `currentScene` live so the
  //     user sees the result as they draw, and Save writes the scene back to
  //     the .md file by block index (re-reads the file each time, so multiple
  //     saves in a row work).
  //
  // React + Excalidraw are dynamic-imported so the bundle only grows when
  // the user actually opens a chapter with an Excalidraw block.
  import { onMount, onDestroy, tick } from 'svelte';
  import { TAURI, updateExcalidrawBlock, saveBytes } from '../lib/tauri.js';
  import { folderRoot } from '../lib/stores.js';
  import { get } from 'svelte/store';

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
          name: 'md-reader-preview',
          width: '100%',
          height: 480,
        })
      );
    });
  }

  // Whenever currentScene changes, refresh the inline preview.
  $effect(() => {
    // Track currentScene + reactTheme so this re-runs on either change.
    void currentScene;
    void reactTheme;
    if (mounted) renderPreview();
  });

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

  function openEditor() {
    saveError = '';
    editorOpen = true;
  }

  // Mount the editor React tree once the host DOM node is in the modal.
  let editorMounted = false;
  $effect(() => {
    if (editorOpen && editorHost && !editorMounted) {
      editorMounted = true;
      mountEditor();
    }
    if (!editorOpen) editorMounted = false;
  });

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
          name: 'md-reader-editor',
          onChange: handleChange,
          UIOptions: { canvasActions: { loadScene: false, saveToActiveFile: false } },
        })
      );
    } catch (e) {
      console.error('[Excalidraw editor]', e);
      saveError = e.message || String(e);
    }
  }

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

  /** Save the current scene back to the .md file. Re-reads the file each
   *  time so a second save after the first works. */
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
