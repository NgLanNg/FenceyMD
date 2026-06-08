<script>
  import { tick } from 'svelte';
  import { onMount, onDestroy } from 'svelte';
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import Placeholder from '@tiptap/extension-placeholder';
  import { Markdown } from 'tiptap-markdown';
  import { saveFile } from '../lib/stores.js';
  import { renderMarkdown, enhance } from '../lib/markdown.js';

  let { item, oncancel, onsaved } = $props();

  let editorEl    = $state(null);
  let previewEl   = $state(null);
  let saving      = $state(false);
  let error       = $state('');
  let showPreview = $state(false);
  let previewHtml = $state('');
  let active = $state({
    bold: false, italic: false, strike: false,
    h1: false, h2: false, h3: false,
    code: false, bullet: false, ordered: false, quote: false,
  });

  let editor;

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

  onMount(() => {
    try {
      editor = new Editor({
        element: editorEl,
        extensions: [
          StarterKit,
          Placeholder.configure({ placeholder: 'Start writing…' }),
          Markdown.configure({ html: false, tightLists: true }),
        ],
        content: item?.content ?? '',
        editorProps: { attributes: { class: 'notion-prose' } },
        onUpdate:          syncActive,
        onSelectionUpdate: syncActive,
        // transaction also fires on stored-mark changes (e.g. toggling bold with a
        // collapsed cursor), so the toolbar highlight stays in sync.
        onTransaction:     syncActive,
      });
      editor.commands.focus();
    } catch (e) {
      error = 'Editor failed to load: ' + String(e);
    }
  });

  onDestroy(() => { clearTimeout(_previewTimer); editor?.destroy(); });

  const c = () => editor?.chain().focus();

  async function save() {
    if (!editor) return;
    saving = true; error = '';
    try {
      const md = editor.storage.markdown.getMarkdown();
      await saveFile(item, md);
      onsaved?.();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); save(); }
  if ((e.metaKey || e.ctrlKey) && e.key === 'p') { e.preventDefault(); showPreview = !showPreview; }
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
    <button class="btn-ghost" onclick={() => oncancel?.()}>Cancel</button>
    <button class="btn-primary" onclick={save} disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
  </div>

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
