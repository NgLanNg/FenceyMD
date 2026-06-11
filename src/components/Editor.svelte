<script>
  import { tick } from 'svelte';
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { Editor, Extension } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import Placeholder from '@tiptap/extension-placeholder';
  import { Markdown } from 'tiptap-markdown';
  import { saveFile } from '../lib/stores.js';
  import { lastSavedAt } from '../lib/stores/progress.js';
  import { folderRoot } from '../lib/stores/state.js';
  import { saveClipboardImage } from '../lib/tauri.js';
  import { renderMarkdown, enhance } from '../lib/markdown.js';
  // Anchor tracking is doc-state based (see `computeAnchor()` below)
  // because ProseMirror re-renders the editor DOM on every doc change,
  // which would wipe any DOM-stamped `data-md-anchor` attributes.

  let { item, oncancel, onsaved } = $props();

  let editorEl    = $state(null);
  let previewEl   = $state(null);
  let showPreview = $state(false);
  let previewHtml = $state('');
  let active = $state({
    bold: false, italic: false, strike: false,
    h1: false, h2: false, h3: false,
    code: false, bullet: false, ordered: false, quote: false,
  });

  // ── #14 autosave + saved indicator state ──────────────────────────
  // `saving` guards against double-save. `dirty` flips to true on
  // every keystroke and back to false once a save completes. The
  // `lastSavedLocal` mirror is the timestamp the toolbar reads from
  // — we also write the same value to the global `lastSavedAt` store
  // so the Reader can show an unsaved-changes dot in the sidebar.
  let saving       = $state(false);
  let dirty        = $state(false);
  let lastSaved    = $state(0);    // ms since epoch of last successful save
  let nowTick      = $state(Date.now()); // re-render trigger for the "Xs ago" label
  let error        = $state('');

  let _autosaveTimer = null;
  const AUTOSAVE_MS = 5000;

  // ── #15 find / replace state ──────────────────────────────────────
  // We use a custom implementation built on Tiptap commands instead
  // of pulling in `prosemirror-search` — keeps the dep tree flat and
  // matches the rest of the file's "no new packages" style. State
  // shape: matches is the array of `{from,to}` ranges in document
  // order; findIndex is the cursor into that array (-1 = none).
  let findOpen       = $state(false);
  let findText       = $state('');
  let replaceText    = $state('');
  let findMatches    = $state([]);
  let findIndex      = $state(-1);
  let findInputEl    = $state(null);

  let editor;
  let _nowInterval = null;

  // Plain Enter inside a code block should exit to a new paragraph.
  // Tiptap's default (StarterKit) keeps you inside the block — you
  // need Mod-Enter to leave. We register a small Tiptap extension
  // with addKeyboardShortcuts(); Tiptap wires this up as a
  // ProseMirror keymap with higher priority than StarterKit's
  // default so plain Enter behaves as users expect. The function
  // lives outside the template literal so Svelte's $-prefix parser
  // rule doesn't trip on `$from`.
  function onCodeBlockEnter(props) {
    const editorInstance = props && props.editor;
    if (!editorInstance) return false;
    const state = editorInstance.state;
    const fromPos = state.selection.$from;
    if (fromPos && fromPos.parent && fromPos.parent.type && fromPos.parent.type.name === 'codeBlock') {
      return editorInstance.chain().focus().exitCode().run();
    }
    return false;
  }

  const CodeBlockEnterExtension = Extension.create({
    name: 'codeBlockEnterExit',
    addKeyboardShortcuts() {
      return {
        Enter: onCodeBlockEnter,
      };
    },
  });

  // ── #22 paragraph tracking ────────────────────────────────────────
  // We emit a `paragraph-focus` CustomEvent on `editorEl` carrying
  // the `data-md-anchor` value of the cursor's enclosing block.
  // Track 2 (Reader) wires the consumer in a separate commit.
  //
  // Implementation note: the plan's recommended path was to
  // re-stamp the editor DOM via `stampAnchors(editorEl)` and walk
  // the stamped tree, but ProseMirror re-renders the editor DOM on
  // every doc change and wipes any DOM-stamped `data-md-anchor`
  // attributes we set. The doc-state path (`computeAnchor()` below)
  // is the source of truth — it walks `editor.state.doc` in source
  // order using the same kind/index scheme as `stampAnchors`.
  let _lastAnchor = null;

  function updatePreview() {
    if (!editor) return;
    previewHtml = renderMarkdown(editor.storage.markdown.getMarkdown());
    tick().then(() => previewEl && enhance(previewEl));
  }

  // Debounced so rapid typing doesn't re-render markdown + mermaid on every keystroke.
  let _previewTimer = null;
  function schedulePreview() {
    clearTimeout(_previewTimer);
    _previewTimer = setTimeout(updatePreview, 250);
  }

  function syncActive() {
    if (!editor) return;
    active = {
      bold:    editor.isActive('bold'),
      italic:  editor.isActive('italic'),
      strike:  editor.isActive('strike'),
      h1:      editor.isActive('heading', { level: 1 }),
      h2:      editor.isActive('heading', { level: 2 }),
      h3:      editor.isActive('heading', { level: 3 }),
      code:    editor.isActive('codeBlock'),
      bullet:  editor.isActive('bulletList'),
      ordered: editor.isActive('orderedList'),
      quote:   editor.isActive('blockquote'),
    };
    if (showPreview) schedulePreview();
  }

  // Refresh preview immediately whenever it's toggled on.
  $effect(() => { if (showPreview) updatePreview(); });

  // ── #14 autosave helpers ──────────────────────────────────────────
  function scheduleAutosave() {
    if (_autosaveTimer) clearTimeout(_autosaveTimer);
    _autosaveTimer = setTimeout(() => {
      // Don't double-save: skip if a save is already in flight.
      if (!saving && dirty) {
        save({ silent: true });
      }
    }, AUTOSAVE_MS);
  }

  // The toolbar label: "Saving…" / "Unsaved" / "Saved just now" /
  // "Saved Xs ago" / "Saved". `nowTick` is a 1s tick state so the
  // label re-renders as time elapses after a save.
  const savedLabel = $derived.by(() => {
    void nowTick; // dependency
    if (saving) return 'Saving…';
    if (dirty)  return 'Unsaved';
    if (!lastSaved) return 'Saved';
    const elapsed = Math.floor((nowTick - lastSaved) / 1000);
    if (elapsed < 2)  return 'Saved just now';
    if (elapsed < 60) return `Saved ${elapsed}s ago`;
    if (elapsed < 3600) return `Saved ${Math.floor(elapsed / 60)}m ago`;
    return 'Saved';
  });
  const saveIndicatorClass = $derived.by(() => {
    if (saving) return 'is-saving';
    if (dirty)  return 'is-unsaved';
    if (!lastSaved) return 'is-saved';
    const elapsed = Math.floor((nowTick - lastSaved) / 1000);
    if (elapsed < 2) return 'is-just-saved';
    return 'is-saved';
  });

  // ── #15 find / replace helpers ────────────────────────────────────
  function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
  function recomputeMatches() {
    if (!editor || !findText) {
      findMatches = [];
      findIndex = -1;
      return;
    }
    const re = new RegExp(escapeRegex(findText), 'gi');
    const out = [];
    editor.state.doc.descendants((node, pos) => {
      if (!node.isText || !node.text) return;
      const text = node.text;
      let m;
      re.lastIndex = 0;
      while ((m = re.exec(text)) !== null) {
        const start = pos + m.index;
        out.push({ from: start, to: start + m[0].length });
        if (m.index === re.lastIndex) re.lastIndex++; // zero-width safety
      }
    });
    findMatches = out;
    if (out.length) {
      // Try to keep the cursor near its current match (if any).
      const sel = editor.state.selection;
      const cur = sel.from;
      let best = 0, bestDist = Infinity;
      for (let i = 0; i < out.length; i++) {
        const d = Math.abs(out[i].from - cur);
        if (d < bestDist) { bestDist = d; best = i; }
      }
      findIndex = best;
      selectMatch(false);
    } else {
      findIndex = -1;
    }
  }

  function selectMatch(focus = true) {
    if (findIndex < 0 || findIndex >= findMatches.length || !editor) return;
    const m = findMatches[findIndex];
    const chain = editor.chain();
    if (focus) chain.focus();
    chain.setTextSelection({ from: m.from, to: m.to }).run();
    // Scroll the match into view inside the edit pane.
    tick().then(() => {
      try {
        const view = editor.view;
        const coords = view.coordsAtPos(m.from);
        const side = view.dom.closest('.notion-edit-side');
        if (side) {
          const r = side.getBoundingClientRect();
          if (coords.top < r.top + 40 || coords.top > r.bottom - 40) {
            side.scrollTop += coords.top - r.top - 80;
          }
        }
      } catch {}
    });
  }
  function nextMatch() {
    if (!findMatches.length) return;
    findIndex = (findIndex + 1) % findMatches.length;
    selectMatch();
  }
  function prevMatch() {
    if (!findMatches.length) return;
    findIndex = (findIndex - 1 + findMatches.length) % findMatches.length;
    selectMatch();
  }
  function replaceCurrent() {
    if (findIndex < 0 || findIndex >= findMatches.length || !editor) return;
    const m = findMatches[findIndex];
    editor.chain()
      .focus()
      .setTextSelection({ from: m.from, to: m.to })
      .insertContent(replaceText)
      .run();
    // Recompute (positions shift after a replace).
    recomputeMatches();
  }
  function replaceAll() {
    if (!findMatches.length || !editor) return;
    // Walk back-to-front so earlier ranges don't shift under us.
    for (let i = findMatches.length - 1; i >= 0; i--) {
      const m = findMatches[i];
      editor.chain()
        .setTextSelection({ from: m.from, to: m.to })
        .insertContent(replaceText)
        .run();
    }
    recomputeMatches();
  }
  function closeFind() {
    findOpen = false;
    findText = '';
    replaceText = '';
    findMatches = [];
    findIndex = -1;
    editor?.commands.focus();
  }

  // ── #4 image paste ────────────────────────────────────────────────
  // Listen for paste on the editor's contenteditable element. If the
  // clipboard contains an image, get the bytes, write them to
  // `<folderRoot>/images/<hash>-<ts>.png` via the existing
  // `save_clipboard_image` Tauri command, then insert the markdown
  // at the cursor. Non-image pastes fall through to Tiptap's default
  // text handling (we don't preventDefault unless we actually use it).
  function shortHash(bytes) {
    // Tiny non-crypto hash from the first 4 KB — enough to disambiguate
    // two near-simultaneous pastes. Not for security.
    let h = 2166136261;
    const n = Math.min(bytes.length, 4096);
    for (let i = 0; i < n; i++) {
      h ^= bytes[i];
      h = Math.imul(h, 16777619);
    }
    return (h >>> 0).toString(36).slice(0, 6);
  }
  async function handlePaste(e) {
    if (!e.clipboardData) return;
    const items = [...e.clipboardData.items];
    for (const it of items) {
      if (!it.type || !it.type.startsWith('image/')) continue;
      const file = it.getAsFile();
      if (!file) continue;
      e.preventDefault();
      try {
        const buf = await file.arrayBuffer();
        const bytes = new Uint8Array(buf);
        const folder = get(folderRoot);
        if (!folder) {
          error = 'No folder open — cannot paste image';
          return;
        }
        const ts = Date.now();
        const rel = `images/${shortHash(bytes)}-${ts}.png`;
        await saveClipboardImage(folder, rel, bytes);
        // Insert the markdown at the cursor. Tiptap parses the
        // `![](path)` into a proper image node on the way in.
        editor.chain().focus().insertContent(`\n![pasted](./${rel})\n`).run();
        // Anchor tracking is doc-state based; the onUpdate handler
        // will emit the new anchor automatically.
      } catch (err) {
        error = 'Image paste failed: ' + String(err);
      }
      return;
    }
    // No image — let Tiptap's default paste handler run (it inserts
    // the text / HTML / markdown as configured).
  }

  // ── #22 paragraph tracking — emit helper ─────────────────────────
  // Compute the `data-md-anchor` value of the cursor's enclosing
  // block by walking the ProseMirror DOC STATE (not the rendered
  // DOM). ProseMirror re-renders the editor DOM on every doc change
  // and would wipe any DOM-stamped `data-md-anchor` attributes we
  // set, so the doc-state path is the only reliable source of
  // truth for the cursor's enclosing block.
  //
  // The kind/index scheme matches `stampAnchors` in src/lib/anchors.js
  // — same per-kind 1-based counter, walked in document order. We
  // deliberately DON'T include the more exotic kinds (mermaid, svg,
  // html, excalidraw, csv) here because those render as complex
  // sub-trees and the editor's Tiptap doc only carries plain
  // block-level nodes for them. Track 2's OutlinePane can render
  // any of the kind strings, so we fall back to the closest parent
  // block when the current block is one of those.
  function blockKindFromNode(node) {
    if (!node) return null;
    const name = node.type && node.type.name;
    if (name === 'paragraph') return 'para';
    if (name === 'heading')    return `h${node.attrs?.level ?? 0}`;
    if (name === 'codeBlock')  return 'code';
    // Tiptap blockquote / list items aren't anchorable in the
    // reader's walker, so we keep walking up.
    return null;
  }
  function computeAnchor() {
    if (!editor) return null;
    const state = editor.state;
    const sel = state.selection;
    const pos = sel.from;
    const resolved = state.doc.resolve(pos);
    // Find the deepest block node that maps to an anchorable kind.
    let targetNode = null, targetDepth = -1;
    for (let depth = resolved.depth; depth > 0; depth--) {
      const n = resolved.node(depth);
      if (blockKindFromNode(n)) { targetNode = n; targetDepth = depth; break; }
    }
    if (!targetNode) return null;
    const kind = blockKindFromNode(targetNode);
    const start = resolved.start(targetDepth);
    // Walk the doc from the start to count anchorable siblings of
    // the same kind that PRECEDE this one. This matches
    // stampAnchors' preorder walk 1:1 because ProseMirror's doc
    // is already in source order.
    let idx = 0;
    state.doc.descendants((node, nPos) => {
      if (idx) return false; // already found
      if (nPos >= start) return false; // past our block
      if (blockKindFromNode(node) === kind) idx += 1;
    });
    return `${kind}-${idx + 1}`;
  }
  function emitParagraphAnchor() {
    if (!editor || !editorEl) return;
    try {
      const anchor = computeAnchor();
      if (anchor === _lastAnchor) return;
      _lastAnchor = anchor;
      editorEl.dispatchEvent(new CustomEvent('paragraph-focus', {
        detail: { anchor },
        bubbles: true,
      }));
    } catch {}
  }

  onMount(() => {
    try {
      editor = new Editor({
        element: editorEl,
        extensions: [
          StarterKit.configure({
            // Default language for new code blocks (otherwise Tiptap
            // shows them as a bare fence with no language).
            codeBlock: { defaultLanguage: 'plaintext' },
          }),
          Placeholder.configure({ placeholder: 'Start writing…' }),
          Markdown.configure({ html: false, tightLists: true }),
          // Plain Enter inside a code block should exit to a new
          // paragraph below it. Tiptap's default is to add a soft
          // newline within the block (you need Mod-Enter to exit),
          // which surprises prose authors. We add a tiny custom
          // extension that registers a higher-priority keymap
          // plugin via addProseMirrorPlugins.
          CodeBlockEnterExtension,
        ],
        content: item?.content ?? '',
        editorProps: { attributes: { class: 'notion-prose' } },
        onUpdate: () => {
          syncActive();
          // #14: mark dirty and reset the autosave timer on every edit.
          dirty = true;
          scheduleAutosave();
          // #22: emit the current cursor's anchor (computed from
          // the doc state, not the DOM).
          emitParagraphAnchor();
        },
        onSelectionUpdate: () => {
          syncActive();
          emitParagraphAnchor();
        },
        onTransaction: syncActive,
        onCreate: () => {
          // The tiptap-markdown extension fires `onUpdate` during
          // the initial content parse (it converts the markdown
          // string into doc nodes), which would otherwise set
          // `dirty = true` and trip the "Unsaved" indicator on a
          // chapter that was never touched. Reset to a clean state
          // here so the indicator starts at "Saved".
          dirty = false;
          if (_autosaveTimer) { clearTimeout(_autosaveTimer); _autosaveTimer = null; }
          // #22: compute + emit the initial anchor from the doc
          // state. We use the doc-state path (not DOM stamping)
          // because ProseMirror re-renders the editor DOM on every
          // doc change, which would wipe any DOM-stamped
          // `data-md-anchor` attributes.
          emitParagraphAnchor();
        },
      });
      editor.commands.focus();
      // Wire up paste directly on the ProseMirror DOM so we can
      // preventDefault BEFORE Tiptap's handler runs.
      const pmDom = editor.view.dom;
      pmDom.addEventListener('paste', handlePaste);
      // Tick to re-render the save label once a second.
      _nowInterval = setInterval(() => { nowTick = Date.now(); }, 1000);
    } catch (e) {
      error = 'Editor failed to load: ' + String(e);
    }
  });

  onDestroy(() => {
    clearTimeout(_previewTimer);
    clearTimeout(_autosaveTimer);
    if (_nowInterval) clearInterval(_nowInterval);
    if (editor) {
      try { editor.view.dom.removeEventListener('paste', handlePaste); } catch {}
      editor.destroy();
    }
  });

  const c = () => editor?.chain().focus();

  async function save({ silent = false } = {}) {
    if (!editor) return;
    if (saving) return; // belt + suspenders
    saving = true; error = '';
    // Stamp `lastSavedAt` BEFORE saveFile so the Reader's $effect sees
    // the recent save when folderMeta → html changes (otherwise the
    // effect runs with lastSave=0 first, sets editing=false, and only
    // THEN sees the updated lastSave — too late to undo the close).
    lastSaved = Date.now();
    lastSavedAt.set(lastSaved);
    try {
      const md = editor.storage.markdown.getMarkdown();
      await saveFile(item, md);
      dirty = false;
      if (!silent) onsaved?.();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
      // If the user kept typing during the in-flight save, the
      // timer is already armed — let it fire normally.
    }
  }
</script>

<svelte:window onkeydown={(e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key === 's') { e.preventDefault(); save(); return; }
  if (mod && e.key === 'p') { e.preventDefault(); showPreview = !showPreview; return; }
  // #15: ⌘H opens the editor's find/replace panel. We intentionally
  // do NOT bind ⌘F here — the Reader's in-chapter find owns that
  // shortcut; the editor and the in-chapter search are different
  // contexts so they don't conflict.
  if (mod && (e.key === 'h' || e.key === 'H') && !e.shiftKey && !e.altKey) {
    e.preventDefault();
    if (!findOpen) {
      findOpen = true;
      tick().then(() => findInputEl?.focus());
    } else {
      findInputEl?.focus();
    }
    return;
  }
  if (findOpen) {
    if (e.key === 'Escape') { e.preventDefault(); closeFind(); return; }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) prevMatch(); else nextMatch();
      return;
    }
  }
}} />

<div class="editor-backdrop" aria-hidden="true"></div>
<div class="editor-shell">
  <!-- Toolbar -->
  <div class="editor-bar">
    <div class="editor-toolbar">
      <button class="editor-tool {active.bold   ? 'is-active':''}" onclick={() => c()?.toggleBold().run()}           title="Bold (⌘B)"><strong>B</strong></button>
      <button class="editor-tool {active.italic ? 'is-active':''}" onclick={() => c()?.toggleItalic().run()}         title="Italic (⌘I)"><em>I</em></button>
      <button class="editor-tool {active.strike ? 'is-active':''}" onclick={() => c()?.toggleStrike().run()}         title="Strikethrough"><s>S</s></button>
      <div class="editor-tool-sep"></div>
      <button class="editor-tool editor-tool-sm {active.h1 ? 'is-active':''}" onclick={() => c()?.toggleHeading({ level: 1 }).run()}>H1</button>
      <button class="editor-tool editor-tool-sm {active.h2 ? 'is-active':''}" onclick={() => c()?.toggleHeading({ level: 2 }).run()}>H2</button>
      <button class="editor-tool editor-tool-sm {active.h3 ? 'is-active':''}" onclick={() => c()?.toggleHeading({ level: 3 }).run()}>H3</button>
      <div class="editor-tool-sep"></div>
      <button class="editor-tool editor-tool-sm {active.bullet   ? 'is-active':''}" onclick={() => c()?.toggleBulletList().run()}  title="Bullet list">•—</button>
      <button class="editor-tool editor-tool-sm {active.ordered  ? 'is-active':''}" onclick={() => c()?.toggleOrderedList().run()} title="Numbered">1.</button>
      <button class="editor-tool editor-tool-sm {active.quote    ? 'is-active':''}" onclick={() => c()?.toggleBlockquote().run()}  title="Blockquote">&ldquo;</button>
      <button class="editor-tool editor-tool-sm {active.code     ? 'is-active':''}" onclick={() => c()?.toggleCodeBlock().run()}   title="Code block">&lt;/&gt;</button>
      <div class="editor-tool-sep"></div>
      <button class="editor-tool editor-tool-sm" onclick={() => c()?.undo().run()} title="Undo (⌘Z)">↩</button>
      <button class="editor-tool editor-tool-sm" onclick={() => c()?.redo().run()} title="Redo (⌘⇧Z)">↪</button>
    </div>
    <div class="editor-tool-sep"></div>
    <button class="editor-tool editor-tool-preview {showPreview ? 'is-active':''}"
      onclick={() => { showPreview = !showPreview; }} title="Toggle preview (⌘P)">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px;margin-right:3px"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
      Preview
    </button>
    <span class="folder-name-tag">{item?.name}</span>
    {#if error}<span class="editor-error">{error}</span>{/if}
    <span class="editor-bar-grow"></span>
    <!-- #14: save-state indicator. Shows "Saving…", "Unsaved",
         "Saved just now" (2s flash), "Saved Xs ago", or "Saved". -->
    <span class="editor-save-indicator {saveIndicatorClass}" aria-live="polite">{savedLabel}</span>
    <button class="btn-ghost" onclick={() => oncancel?.()}>Cancel</button>
    <button class="btn-primary" onclick={save} disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
  </div>

  <!-- #15: find / replace panel. Sits between the toolbar and the
       editor body. Same row styles as the existing CSS (pre-shipped
       in 85ecfd0) so we don't churn app.css. -->
  {#if findOpen}
    <div class="editor-find-replace" role="search">
      <div class="editor-find-row">
        <span class="editor-find-label">Find</span>
        <input
          class="editor-find-input"
          type="text"
          placeholder="Find in chapter…"
          aria-label="Find in chapter"
          bind:this={findInputEl}
          bind:value={findText}
          oninput={recomputeMatches}
          onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.shiftKey ? prevMatch() : nextMatch(); } }}
        />
        <span class="editor-find-status">
          {findMatches.length === 0 ? '0 of 0' : `${findIndex + 1} of ${findMatches.length}`}
        </span>
        <button class="editor-tool editor-tool-sm" onclick={prevMatch} title="Previous match (Shift+Enter)">↑</button>
        <button class="editor-tool editor-tool-sm" onclick={nextMatch} title="Next match (Enter)">↓</button>
        <button class="editor-tool editor-tool-sm" onclick={closeFind} title="Close (Esc)" aria-label="Close find">×</button>
      </div>
      <div class="editor-find-row">
        <span class="editor-find-label">Replace</span>
        <input
          class="editor-find-input"
          type="text"
          placeholder="Replace with…"
          aria-label="Replace with"
          bind:value={replaceText}
        />
        <button class="editor-tool editor-tool-sm" onclick={replaceCurrent} title="Replace current match" disabled={findIndex < 0}>Replace</button>
        <button class="editor-tool editor-tool-sm" onclick={replaceAll} title="Replace all matches" disabled={findMatches.length === 0}>All</button>
      </div>
    </div>
  {/if}

  <!-- Body: edit pane (+ optional preview pane) -->
  <div class="notion-split {showPreview ? 'has-preview' : ''}">
    <div class="notion-edit-side">
      <div class="notion-editor-inner" bind:this={editorEl}></div>
    </div>
    {#if showPreview}
      <div class="notion-preview-side">
        <div class="notion-preview-inner chapter-markdown" bind:this={previewEl}>{@html previewHtml}</div>
      </div>
    {/if}
  </div>
</div>

<style>
  /* #14 — save indicator. Lives next to the Save button; shows
     the current state so the user doesn't have to wonder whether
     the last ⌘S actually went through. Scoped to this component
     (Svelte adds the data-attribute hash) so we don't touch the
     pre-shipped app.css. */
  .editor-save-indicator {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-sans);
    font-size: 11px;
    font-weight: 500;
    color: var(--ink-muted);
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-variant);
    transition: background 0.2s, color 0.2s;
    user-select: none;
    white-space: nowrap;
  }
  .editor-save-indicator.is-saved {
    color: var(--ink-muted);
    background: var(--surface-variant);
  }
  .editor-save-indicator.is-just-saved {
    color: #fff;
    background: #2a8b8b; /* tertiary, matches the "saved" link color */
  }
  .editor-save-indicator.is-unsaved {
    color: var(--ink-secondary);
    background: var(--surface-variant);
  }
  .editor-save-indicator.is-saving {
    color: var(--ink-secondary);
    background: var(--surface-variant);
  }
  .editor-save-indicator.is-saving::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: editor-save-pulse 1s ease-in-out infinite;
  }
  @keyframes editor-save-pulse {
    0%, 100% { opacity: 0.3; }
    50% { opacity: 1; }
  }
</style>
