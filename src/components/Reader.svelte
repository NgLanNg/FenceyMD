<script>
  import { tick } from 'svelte';
  import { TAURI, saveBytes, printPDF } from '../lib/tauri.js';
  import { renderMarkdown, enhance } from '../lib/markdown.js';
  import { labelFromName } from '../lib/index.js';
  import {
    folderMeta, progressMap, fontSize, theme, findItem, siblingsOf, goChapter, goGroup, goHome,
    saveProgress, progressFor, toggleBookmark, adjustFontSize, adjustContentWidth, toggleTheme,
    fontSizeLabels, renameFile,
    navCollapsed, navOpen, viewMode, toggleViewMode,
    onboarded, dismissOnboarding,
  } from '../lib/stores.js';
  import { pendingInChapterSearch } from '../lib/stores/state.js';
  import { chapterScrollFrac, lastSavedAt } from '../lib/stores/progress.js';
  import { hasMultipleSlides } from '../lib/slides.js';
  import { resolveChapterLink } from '../lib/link-resolver.js';
  import Editor from './Editor.svelte';
  import SlideViewer from './SlideViewer.svelte';
  import { outlineVisible, isOutlineVisible } from '../lib/stores/prefs.js';

  let { path, isMobile = false } = $props();

  // Editing requires the Tauri backend to save. In dev/browser ?test=1 mode we
  // still expose the editor so the WYSIWYG flow can be exercised (save is a no-op).
  const canEdit = TAURI || (typeof location !== 'undefined' && new URLSearchParams(location.search).get('test') === '1');

  let mdEl = $state(null);
  let searchQuery = $state('');
  let editing = $state(false);
  let pdfBusy = $state(false);

  // ── Rename ──
  let renaming = $state(false);
  let renameValue = $state('');
  let renameError = $state('');
  let renameBusy = $state(false);

  function focusOnMount(node) {
    node.focus();
    node.select();
  }

  function showNav() {
    if (isMobile) navOpen.set(true);
    else navCollapsed.set(false);
  }
  function startRename() {
    renameValue = (item?.name || '').replace(/\.md$/i, '');
    renameError = '';
    renaming = true;
  }
  async function confirmRename() {
    const name = renameValue.trim();
    if (!name || renameBusy) return;
    renameBusy = true; renameError = '';
    try {
      const { newPath } = await renameFile(item, name);
      renaming = false;
      goChapter(newPath); // follow the file to its new path
    } catch (e) {
      renameError = String(e).replace(/^Error:\s*/, '');
    } finally {
      renameBusy = false;
    }
  }

  const item = $derived($folderMeta.find((f) => f.path === path));
  const text = $derived(
    item?.content != null
      ? item.content
      : `# ${labelFromName(path)}\n\n> *Content not available.*`
  );
  const html = $derived(renderMarkdown(text));
  // Strip a single leading <h1> so the centered editorial header (mdTitle) isn't duplicated.
  const bodyHtml = $derived(html.replace(/^\s*<h1[^>]*>[\s\S]*?<\/h1>/i, ''));
  const sib = $derived(siblingsOf(path));
  const bookmarked = $derived(!!$progressMap[item?.diskPath || path]?.bookmarked);
  const slideCapable = $derived(hasMultipleSlides(text));
  const inSlideMode = $derived($viewMode === 'slide' && slideCapable);
  function exitSlide() { viewMode.set('read'); }
  const wordCount = $derived(text.trim().split(/\s+/).length);

  // ROADMAP v1.1 #12 — reading time + word count in chapter header.
  // We strip markdown chrome (fences, headings, link syntax, code
  // spans) before counting, so a chapter with 200 lines of ```js
  // doesn't inflate the "words" number. Then `ceil(words / 220)`
  // gives a Vim-style "5 min" reading time.
  function stripMarkdown(s) {
    return s
      .replace(/```[\s\S]*?```/g, ' ')
      .replace(/^\s{0,3}#{1,6}\s+/gm, '')
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\[[^\]]*\]/g, '$1')
      .replace(/(\*\*|__|\*|_|~~|`)/g, '')
      .replace(/^\s{0,3}>\s?/gm, '')
      .replace(/^\s{0,3}[-*+]\s+/gm, '')
      .replace(/^\s{0,3}\d+\.\s+/gm, '')
      .replace(/^[-*_]{3,}\s*$/gm, ' ');
  }
  const proseText = $derived(stripMarkdown(text));
  const proseWordCount = $derived(proseText.trim() ? proseText.trim().split(/\s+/).length : 0);
  const readingMinutes = $derived(proseWordCount > 0 ? Math.max(1, Math.ceil(proseWordCount / 220)) : 0);
  const wordCountLabel = $derived(
    proseWordCount >= 1000
      ? (proseWordCount / 1000).toFixed(proseWordCount >= 10000 ? 0 : 1).replace(/\.0$/, '') + 'k'
      : String(proseWordCount)
  );
  const readingTimeLabel = $derived(
    readingMinutes > 0 ? `${readingMinutes} min · ${wordCountLabel} words` : `${wordCountLabel} words`
  );
  const mdTitle = $derived((text.match(/^#\s+(.+)$/m)?.[1] || labelFromName(item?.name || path)).trim());
  const backLabel = $derived(sib.group || 'Library');

  let progressBar = $state(0);

  // ROADMAP v1.1 #3 — auto-TOC outline pane visibility.
  // The store keeps a tri-state ('1' / '0' / 'auto'); we resolve 'auto'
  // against the current viewport on every read. On viewport changes
  // (resize, mobile <-> desktop) we re-resolve, so a chapter on a
  // desktop stays expanded, then auto-collapses if the window is
  // narrowed. The first explicit user toggle pins the value to '1'
  // or '0' so we stop auto-resolving.
  let viewportW = $state(typeof window !== 'undefined' ? window.innerWidth : 1200);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const onResize = () => { viewportW = window.innerWidth; };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });
  const outlineOpen = $derived(isOutlineVisible($outlineVisible, viewportW));
  // Wrapper class for the reader — adds an `outline-on` hook so CSS can
  // shift toolbar right-padding when the pane is showing, keeping the
  // article centred in the remaining space.
  const readerCls = $derived('reader2' + (outlineOpen ? ' outline-on' : ''));

  // Re-enhance + restore scroll whenever the chapter (or its html) changes.
  // Close the editor whenever the chapter's rendered HTML changes from an
  // EXTERNAL source (file watcher, etc.). Autosave's own save() also updates
  // folderMeta → html, but the editor should stay open in that case. We
  // detect "save vs external" by checking $lastSavedAt: if a save happened
  // in the last few hundred ms, this html change is from our own save
  // (and the editor's `save({ silent: true })` path already handled the
  // close-or-not decision). Otherwise, it's an external change and we close
  // the editor so the user sees the fresh content.
  //
  // IMPORTANT: must NOT read `editing` here — that would make Svelte track it, causing
  // the effect to re-run when editing=true and immediately reset it to false.
  $effect(() => {
    html; // track only html — do not read mdEl or editing here
    const lastSave = $lastSavedAt;
    if (lastSave && (Date.now() - lastSave) < 500) return; // our own autosave; editor stays open
    editing = false;
  });

  // Enhance + restore scroll whenever the chapter element is available (first mount,
  // content change while reading, or returning from the editor).
  $effect(() => {
    html; // track content changes
    const el = mdEl; // track element availability
    if (!el) return;
    let cancelled = false;
    (async () => {
      await tick();
      if (cancelled || !mdEl) return;
      enhance(mdEl, { relPath: item?.diskPath || path });
      await tick();
      if (cancelled) return;
      const pr = progressFor(item?.diskPath || path);
      const docH = document.documentElement.scrollHeight - window.innerHeight;
      window.scrollTo(0, pr.scroll > 0 && docH > 0 ? Math.round(pr.scroll * docH) : 0);
      updateProgress();
    })();
    return () => { cancelled = true; };
  });

  // Save reading position on scroll (debounced inside saveProgress).
  $effect(() => {
    const onScroll = () => updateProgress();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  });

  function updateProgress() {
    const docH = document.documentElement.scrollHeight - window.innerHeight;
    const frac = docH > 0 ? window.scrollY / docH : 0;
    progressBar = frac * 100;
    // ROADMAP v1.1 #13 — publish the scroll fraction to the sidebar
    // store so the progress dot in the chapter list can render in
    // real time. Capped to [0, 1] in case of rounding overshoot.
    chapterScrollFrac.set(Math.max(0, Math.min(1, frac)));
    if (!editing && item) {
      const pr = progressFor(item.diskPath || item.path);
      saveProgress(item.diskPath || item.path, frac, pr.bookmarked);
    }
  }

  function back() { sib.group ? goGroup(sib.group) : goHome(); }

  // In-chapter search (highlight + scroll to first match).
  function runSearch() {
    if (!mdEl) return;
    mdEl.querySelectorAll('.search-highlight').forEach((el) => {
      const p = el.parentNode;
      while (el.firstChild) p.insertBefore(el.firstChild, el);
      p.removeChild(el);
      p.normalize();
    });
    const q = searchQuery.trim();
    if (!q) return;
    const regex = new RegExp('(' + q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
    const walker = document.createTreeWalker(mdEl, NodeFilter.SHOW_TEXT, null, false);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    nodes.forEach((node) => {
      if (node.parentElement.closest('.search-highlight')) return;
      if (['SCRIPT', 'STYLE', 'TEXTAREA', 'INPUT'].includes(node.parentElement.tagName)) return;
      if (regex.test(node.textContent)) {
        const span = document.createElement('span');
        span.innerHTML = node.textContent.replace(regex, '<mark class="search-highlight">$1</mark>');
        node.parentNode.replaceChild(span, node);
      }
    });
    const first = mdEl.querySelector('.search-highlight');
    if (first) first.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }
  $effect(() => { searchQuery; runSearch(); });

  // Pick up a pending in-chapter search string set by the cross-chapter
  // search panel when the user picks a result. We clear the pending value
  // immediately so a re-mount on the same chapter (e.g. via watcher reload)
  // doesn't get the same query re-applied, then let the runSearch effect
  // above re-highlight based on the updated searchQuery.
  $effect(() => {
    const pending = $pendingInChapterSearch;
    if (pending) {
      searchQuery = pending;
      pendingInChapterSearch.set('');
    }
  });

  // ROADMAP v1.1 #20 — markdown link-to-md navigation. When the user
  // clicks an <a href="…"> inside the rendered chapter, intercept it
  // and route to the target chapter instead of letting the WebView
  // navigate away (which reloads the app in dev, and is broken in
  // Tauri). The resolver short-circuits external / anchor / non-md
  // links, returning null, and we let the browser handle those.
  function onChapterClick(e) {
    // Find the closest <a> in case the user clicked a child element
    // (e.g. <code> or <em> inside a markdown link).
    const a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
    if (!a) return;
    const href = a.getAttribute('href');
    if (!href) return;
    // Use `diskPath` (the full path including the group prefix) so the
    // resolver's directory math has the right base. `path` is group-
    // stripped and would break `../` resolution.
    const currentPath = item?.diskPath || path;
    const resolved = resolveChapterLink(currentPath, href, $folderMeta);
    if (!resolved) return; // browser handles external / anchor / non-md
    e.preventDefault();
    e.stopPropagation();
    goChapter(resolved);
  }

  function copyLink() {
    const url = location.origin + location.pathname + '?load=' + encodeURIComponent(path);
    navigator.clipboard.writeText(url);
  }

  // Export the current chapter as a PDF.
  //   • Desktop (Tauri): render with html2pdf.js (lazy-loaded), hand the bytes
  //     to the Rust `save_export` command, which pops a native Save dialog.
  //     Works regardless of the host OS's print-to-PDF support.
  // Export the current chapter as a PDF via the OS print dialog.
  //
  // We show a visible toast so the user knows the print panel is
  // opening (Tauri WKWebView can hide it behind the app window).
  // The user picks "Save as PDF" in the print panel to save.
  let pdfToast;
  function showPdfToast(msg) {
    if (pdfToast) pdfToast.remove();
    pdfToast = document.createElement('div');
    pdfToast.className = 'pdf-toast';
    pdfToast.textContent = msg;
    document.body.appendChild(pdfToast);
  }
  function hidePdfToast() {
    if (pdfToast) { pdfToast.remove(); pdfToast = null; }
  }

  async function exportPDF() {
    if (pdfBusy || !TAURI) return;
    pdfBusy = true;
    showPdfToast('Generating PDF…');

    try {
      // Wait for mermaid SVGs to be in the DOM (enhance() is async).
      // Poll up to ~3s for any pending `.mermaid` block to gain an <svg>.
      await tick();
      if (mdEl) {
        for (let i = 0; i < 30; i++) {
          let allReady = true;
          for (const pre of mdEl.querySelectorAll('.mermaid')) {
            if (!pre.querySelector('svg')) { allReady = false; break; }
          }
          if (allReady) break;
          await new Promise((r) => setTimeout(r, 100));
        }
      }

      // Convert any Excalidraw blocks to inline SVG so the PDF pipeline
      // (which just inlines mdEl.innerHTML into the headless Chrome page)
      // can render them. exportToSvg runs entirely in the browser — no DOM
      // needed, just the scene data.
      if (mdEl && mdEl.querySelector('.excalidraw-block')) {
        showPdfToast('Rendering Excalidraw…');
        const exMod = await import('@excalidraw/excalidraw');
        const blocks = mdEl.querySelectorAll('.excalidraw-block');
        for (const block of blocks) {
          // The viewer stashes the original scene JSON on the element so
          // we don't have to re-parse the source. If it's missing (older
          // viewer), fall back to the inner React-rendered canvas.
          const src = block.dataset.excalidrawJson;
          if (!src) continue;
          let parsed;
          try { parsed = JSON.parse(src); } catch { continue; }
          try {
            const svg = await exMod.exportToSvg({
              type: 'excalidraw',
              version: 2,
              source: 'https://excalidraw.com',
              elements: parsed.elements || [],
              appState: { ...(parsed.appState || {}), viewBackgroundColor: $theme === 'dark' ? '#1c1c1e' : '#ffffff' },
              files: parsed.files || {},
            });
            // The exported SVG is an SVGSVGElement. Style the wrapper so it
            // looks like the in-app card.
            const wrapper = document.createElement('div');
            wrapper.className = 'excalidraw-block excalidraw-static';
            wrapper.appendChild(svg);
            block.replaceWith(wrapper);
          } catch (e) {
            console.warn('[Excalidraw→SVG]', e);
          }
        }
      }

      // Pull the live rendered HTML (mermaid SVGs + Excalidraw SVGs now in place).
      const liveHtml = mdEl ? mdEl.innerHTML : bodyHtml;

      // Snapshot the computed CSS variables the chapter relies on (theme + fonts).
      // This way the PDF matches the on-screen rendering exactly, no matter the
      // theme the user picked.
      const cs = getComputedStyle(document.documentElement);
      const vars = {};
      for (const name of [
        '--surface', '--surface-variant', '--surface-container-low',
        '--surface-container-lowest', '--surface-container', '--surface-container-high',
        '--ink', '--ink-secondary', '--ink-muted',
        '--tertiary', '--tertiary-dim',
        '--font-serif', '--font-sans',
        '--space-2', '--space-3', '--space-4', '--space-5', '--space-6',
        '--space-8', '--space-10', '--space-12', '--space-16',
        '--radius-sm', '--radius-md', '--radius-lg',
      ]) {
        vars[name] = cs.getPropertyValue(name).trim();
      }

      const pdfBytes = await printPDF(mdTitle, liveHtml, $theme === 'dark', vars);
      if (pdfBytes) {
        const safeName = (item?.name || 'chapter').replace(/\.md$/i, '') + '.pdf';
        const saved = await saveBytes(safeName, new Uint8Array(pdfBytes));
        if (saved) showPdfToast('PDF saved.');
        else showPdfToast('PDF cancelled.');
      }
    } catch (err) {
      console.error('[PDF]', err);
      showPdfToast('PDF failed: ' + String(err));
    } finally {
      pdfBusy = false;
      setTimeout(hidePdfToast, 2500);
    }
  }

  // Keyboard: ← / → move between chapters when not typing.
  // ROADMAP v1.1 #13 — g g (two g's within 500ms) jumps to top, G to bottom.
  // ROADMAP v1.1 #17 — ? opens the cheatsheet modal. The cheatsheet
  // modal is intentionally NOT a focus trap (it's a hint, not a
  // workflow) — Esc/click-outside both close it.
  //
  // When slide view is active, SlideViewer.svelte owns the keyboard
  // (arrows / Space / PageUp / PageDown / Home / End / Esc for
  // slide nav). If we let the Reader's arrow handler fire too,
  // pressing → in slide mode would BOTH advance the slide AND
  // navigate to the next chapter — the chapter change unmounts the
  // SlideViewer, so the user sees the slide jump followed by an
  // unexpected chapter jump and slide view exiting. Bail early here
  // so the SlideViewer's handler is the only one that fires.
  let showCheatsheet = $state(false);
  let lastGAt = 0;
  function onKey(e) {
    const t = e.target;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
    if (editing) return;
    if (inSlideMode) return;
    // ? opens the cheatsheet. The Shift+/ chord comes out as e.key === '?' on US layouts.
    if (e.key === '?' && showCheatsheet) {
      showCheatsheet = false;
      e.preventDefault();
      return;
    }
    if (e.key === '?' && !showCheatsheet) {
      showCheatsheet = true;
      e.preventDefault();
      return;
    }
    if (e.key === 'Escape' && showCheatsheet) {
      showCheatsheet = false;
      e.preventDefault();
      return;
    }
    if (e.key === 'ArrowLeft' && sib.prev) goChapter(sib.prev.path);
    else if (e.key === 'ArrowRight' && sib.next) goChapter(sib.next.path);
    else if (e.key === 'g' || e.key === 'G') {
      const now = performance.now();
      if (e.key === 'G') {
        e.preventDefault();
        const docH = document.documentElement.scrollHeight - window.innerHeight;
        window.scrollTo({ top: docH, behavior: 'smooth' });
        lastGAt = 0;
      } else if (now - lastGAt < 500) {
        e.preventDefault();
        window.scrollTo({ top: 0, behavior: 'smooth' });
        lastGAt = 0;
      } else {
        lastGAt = now;
      }
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if editing}
  <Editor {item} oncancel={() => (editing = false)} onsaved={() => (editing = false)} />
{:else}
<div class={readerCls}>
  <!-- Reading progress, fixed to viewport top -->
  <div class="reader2-progress" aria-hidden="true"><div class="reader2-progress-fill" style="width:{progressBar}%"></div></div>

  <!-- Sticky toolbar -->
  <div class="reader2-toolbar">
    <div class="reader2-toolbar-inner">
      <div class="reader2-tools-left">
        <button class="reader2-back" onclick={back}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"/></svg>
          {backLabel}
        </button>
        <div class="reader2-find">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
          <input
            type="text" placeholder="Find in chapter…" aria-label="Search in chapter"
            bind:value={searchQuery}
            onkeydown={(e) => { if (e.key === 'Escape') searchQuery = ''; }}
          />
        </div>
      </div>
      <div class="reader2-tools-right">
        <div class="reader2-tool-group">
          <button class="tool-btn" onclick={() => adjustFontSize(-1)} title="Decrease font size">A−</button>
          <span class="font-size-indicator">{fontSizeLabels[$fontSize] ?? 'M'}</span>
          <button class="tool-btn" onclick={() => adjustFontSize(1)} title="Increase font size">A+</button>
        </div>
        <div class="reader2-tool-group">
          <button class="tool-btn" onclick={() => adjustContentWidth(-50)} title="Narrower">W−</button>
          <button class="tool-btn" onclick={() => adjustContentWidth(50)} title="Wider">W+</button>
        </div>
        <div class="reader2-tool-group">
          <button class="tool-btn" onclick={toggleTheme} title="Toggle dark mode" aria-label="Toggle dark mode">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
          </button>
          <button
            class="tool-btn"
            class:active={$viewMode === 'slide'}
            onclick={toggleViewMode}
            disabled={!slideCapable}
            title={slideCapable ? ($viewMode === 'slide' ? 'Exit slide view' : 'View as slides') : 'No slide breaks found (add `---` between sections)'}
            aria-label="Toggle slide view"
            aria-pressed={$viewMode === 'slide'}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="13" rx="2"/><polygon points="10,8 10,13 15,10.5" fill="currentColor" stroke="none"/></svg>
          </button>
          <button class="tool-btn" onclick={copyLink} title="Copy link" aria-label="Copy link">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
          </button>
          <button class="tool-btn" onclick={exportPDF} disabled={pdfBusy} title="Export as PDF" aria-label="Export as PDF">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="12" y1="18" x2="12" y2="12"/><line x1="9" y1="15" x2="12" y2="12"/><line x1="15" y1="15" x2="12" y2="12"/></svg>
          </button>
          <button class="tool-btn {bookmarked ? 'bookmarked' : ''}" onclick={() => toggleBookmark(item)} title="Bookmark this chapter" aria-label="Bookmark">
            <svg viewBox="0 0 24 24" fill={bookmarked ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2"><path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"/></svg>
          </button>
          {#if canEdit}
            <button class="tool-btn" onclick={startRename} title="Rename file" aria-label="Rename file">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2"/><line x1="12" y1="4" x2="12" y2="20"/><line x1="9" y1="20" x2="15" y2="20"/></svg>
            </button>
            <button class="tool-btn" onclick={() => (editing = true)} title="Edit markdown" aria-label="Edit markdown">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4z"/></svg>
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>

  <!-- Slide deck view -->
  {#if inSlideMode}
    <section class="reader2-slides">
      <SlideViewer markdown={text} onExit={exitSlide} />
    </section>
  {:else}
    <!-- Editorial article -->
    <article class="reader2-article">
    <header class="reader2-header">
      <h1 class="reader2-title">{mdTitle}</h1>
      <div class="reader2-meta">{path} · {readingTimeLabel}</div>
    </header>

    {#key $theme}
      <div class="chapter-markdown reader2-body" bind:this={mdEl} data-title={mdTitle} onclick={onChapterClick}>{@html bodyHtml}</div>
    {/key}

    {#if sib.prev || sib.next}
      <footer class="reader2-siblings">
        {#if sib.prev}
          <button class="reader2-sib prev" onclick={() => goChapter(sib.prev.path)}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
            <span class="reader2-sib-text"><em>Previous</em>{labelFromName(sib.prev.name)}</span>
          </button>
        {:else}<span class="reader2-sib-spacer"></span>{/if}

        <span class="reader2-sib-counter">{sib.idx + 1} / {sib.list.length}</span>

        {#if sib.next}
          <button class="reader2-sib next" onclick={() => goChapter(sib.next.path)}>
            <span class="reader2-sib-text"><em>Next</em>{labelFromName(sib.next.name)}</span>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/></svg>
          </button>
        {:else}<span class="reader2-sib-spacer"></span>{/if}
      </footer>
    {/if}
    </article>
  {/if}

  {#if canEdit}
    <button class="reader2-fab" onclick={() => (editing = true)} title="Edit chapter" aria-label="Edit chapter">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4z"/></svg>
    </button>
  {/if}

  {#if renaming}
    <div class="rename-overlay" onclick={(e) => { if (e.target === e.currentTarget) renaming = false; }} role="presentation">
      <div class="rename-dialog" role="dialog" aria-modal="true" aria-label="Rename file">
        <div class="rename-title">Rename file</div>
        <input
          class="rename-input" type="text" bind:value={renameValue}
          spellcheck="false" autocomplete="off"
          onkeydown={(e) => { if (e.key === 'Enter') confirmRename(); else if (e.key === 'Escape') renaming = false; }}
          use:focusOnMount
        />
        <div class="rename-hint">.md is added automatically. The file is renamed on disk.</div>
        {#if renameError}<div class="rename-error">{renameError}</div>{/if}
        <div class="rename-actions">
          <button class="btn-ghost" onclick={() => (renaming = false)}>Cancel</button>
          <button class="btn-primary" onclick={confirmRename} disabled={renameBusy || !renameValue.trim()}>
            {renameBusy ? 'Renaming…' : 'Rename'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- ROADMAP v1.1 #16 — first-launch onboarding hint. Sits as a
       small pill just inside the reader, pointing at the sidebar
       with a chevron. Dismiss persists in localStorage; once gone,
       it never comes back. -->
  {#if !$onboarded}
    <div class="onboard-hint" role="status" aria-live="polite">
      <div class="onboard-hint-arrow" aria-hidden="true"></div>
      <div class="onboard-hint-body">
        <span class="onboard-hint-text">Pick a folder to start — Markdown, HTML, even Excalidraw scenes render in place.</span>
        <button class="onboard-hint-dismiss" onclick={dismissOnboarding} aria-label="Dismiss onboarding hint">Got it</button>
      </div>
    </div>
  {/if}

  <!-- ROADMAP v1.1 #17 — keyboard cheatsheet. Tiny modal listing
       every shortcut the reader ships with. NOT a focus trap (per
       spec): Esc and click-outside both close. -->
  {#if showCheatsheet}
    <div
      class="cheatsheet-backdrop"
      onclick={(e) => { if (e.target === e.currentTarget) showCheatsheet = false; }}
      role="presentation"
    >
      <div class="cheatsheet" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
        <div class="cheatsheet-head">
          <span class="cheatsheet-title">Keyboard shortcuts</span>
          <button class="cheatsheet-close" onclick={() => (showCheatsheet = false)} aria-label="Close cheatsheet">×</button>
        </div>
        <div class="cheatsheet-list">
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>←</kbd> <kbd>→</kbd></span><span>Previous / next chapter</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>⌘</kbd><kbd>F</kbd></span><span>Find in chapter</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>⌘</kbd><kbd>⇧</kbd><kbd>F</kbd></span><span>Search across chapters</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>⌘</kbd><kbd>P</kbd></span><span>Export chapter as PDF</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>⌘</kbd><kbd>S</kbd></span><span>Save (in edit mode)</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>e</kbd></span><span>Edit current chapter</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>Esc</kbd></span><span>Clear search / close</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>g</kbd><kbd>g</kbd></span><span>Jump to top of chapter</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>G</kbd></span><span>Jump to bottom of chapter</span></div>
          <div class="cheatsheet-row"><span class="cheatsheet-keys"><kbd>?</kbd></span><span>Show this cheatsheet</span></div>
        </div>
      </div>
    </div>
  {/if}

</div>
{/if}
