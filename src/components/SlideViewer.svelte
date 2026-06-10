<script>
  // Slide deck view powered by Marp (https://marp.app).
  //
  // Markdown semantics: a `---` horizontal rule splits slides. Marp
  // directives at the top of the file (between two `---` lines) are
  // honored, e.g.:
  //
  //   ---
  //   theme: gaia
  //   _class: lead
  //   ---
  //
  //   # Welcome
  //
  //   ---
  //
  //   ## Slide 2
  //
  // `marp-core` is dynamic-imported on first slide open so the bundle
  // only grows when the user actually opens a chapter with slides.
  import { onMount, onDestroy, tick } from 'svelte';
  import { splitIntoSlides } from '../lib/slides.js';
  // Phase 2: the registry is the single source of truth for fence
  // dispatch. The reader + slides + PDF all go through it, so adding
  // a new fence type means dropping a file in `renderers/` and adding
  // an entry to `renderers/manifest.json` — no SlideViewer changes.
  import { dispatch } from '../lib/registry.js';
  // Register the core renderers with the registry at module init
  // (svg, html, mermaid, excalidraw, math, shiki).
  import '../lib/renderers/index.js';
  import { addDiagramTools } from '../lib/diagram-export.js';
  // ROADMAP v1.1 #23 — stamp stable `data-md-anchor="slide-N"` on each
  // Marpit slide so cross-chapter link / AI-edit primitives can address
  // a specific slide by a stable address.
  import { stampSlides } from '../lib/anchors.js';

  let { markdown = '', onExit } = $props();

  const slides = $derived(splitIntoSlides(markdown));
  let current = $state(0);
  let stageEl;
  let trackEl;
  let resizeObserver;
  let marpReady = $state(false);
  let marpError = $state('');
  let marpStyleEl;          // <style> with Marp's CSS, injected once
  let marpHostEl;           // host div for the rendered SVG slides

  // Cached Marp instance + the rendered HTML for each slide. We
  // render all slides up-front (one Marp call), then use CSS
  // transform to show only the current one. This makes slide
  // navigation instant — no re-render on arrow keys.
  let slideSvgs = $state([]); // array of { html, width, height }
  let slideEls = [];        // array of DOM nodes, one per slide (filled on mount)
  let rendered = $derived(slideSvgs.length > 0);

  // Marp's default slide is 1280×720 (16:9). Fit-to-stage scales
  // the entire deck uniformly.
  const SLIDE_W = 1280;
  const SLIDE_H = 720;

  // Re-fit whenever the stage resizes or the slide changes.
  $effect(() => {
    void current;
    if (rendered) {
      tick().then(() => requestAnimationFrame(fitToStage));
    }
  });

  onMount(async () => {
    if (typeof ResizeObserver !== 'undefined' && stageEl) {
      resizeObserver = new ResizeObserver(() => fitToStage());
      resizeObserver.observe(stageEl);
    }
    window.addEventListener('resize', fitToStage);

    // Lazy-load Marp and render all slides up-front.
    try {
      const { Marp } = await import('@marp-team/marp-core');
      const marp = new Marp();
      const src = joinSlidesForMarp(slides);
      const { html, css } = marp.render(src);

      // Katex + shiki CSS is loaded once by main.js for the whole app;
      // slides inherit the same stylesheet. No slide-specific injection.

      // Inject Marp's CSS into the document once. The CSS is global
      // (it uses class selectors on `div.marpit`) so a single <style>
      // tag is fine.
      if (!document.getElementById('marp-core-css')) {
        marpStyleEl = document.createElement('style');
        marpStyleEl.id = 'marp-core-css';
        marpStyleEl.textContent = css;
        document.head.appendChild(marpStyleEl);
      }

      // Render Marp's full output into a hidden host. Marp emits a
      // <div class="marpit"> with one <svg> per slide — we clone each
      // SVG into our track. Cloning (rather than re-wrapping) keeps
      // Marp's `div.marpit > svg > foreignObject > section` CSS
      // selectors intact, so theme styles apply correctly.
      marpHostEl = document.createElement('div');
      marpHostEl.id = 'marp-host';
      marpHostEl.style.cssText = 'position:absolute;left:-99999px;top:-99999px;width:1280px;';
      marpHostEl.innerHTML = html;
      document.body.appendChild(marpHostEl);
      const marpitDiv = marpHostEl.querySelector('div.marpit');
      if (marpitDiv) {
        // Post-process fenced blocks through the registry. The reader
        // does the same via `enhance()`; slides use `dispatch` directly
        // because Marp's output wraps the whole deck in a single
        // `<div class="marpit">` and we don't want the per-area math
        // walker or copy-button pass — only the fence renderers.
        const dark = typeof document !== 'undefined'
          && document.documentElement.getAttribute('data-theme') === 'dark';
        const ctx = {
          area: marpitDiv,
          isPdf: false,
          dark,
          meta: {},
          // Slides wrap svg/html blocks with slide-prefixed classes so
          // the fixed 16:9 viewport can size the graphic appropriately.
          // The registry's svg/html renderers honor ctx.wrapClassName.
          svgWrapClass: 'slide-svg-block',
          htmlWrapClass: 'slide-html-block',
          diagramTools: (container, name, opts) => addDiagramTools(container, name, opts),
        };
        // Walk every fence in document order and dispatch to the
        // registry. Unknown langs go through the shiki renderer.
        // We match any <pre><code> whose class list contains a
        // `language-X` token — the previous `class^="language-"`
        // selector failed when the class attribute string started
        // with a different token (e.g. "js language-js").
        const codes = marpitDiv.querySelectorAll('pre code');
        let i = 0;
        for (const codeEl of codes) {
          const pre = codeEl.parentElement;
          if (!pre) continue;
          const cls = [...codeEl.classList].find((c) => c.startsWith('language-'));
          if (!cls) continue;
          const lang = cls.slice('language-'.length).toLowerCase();
          await dispatch(
            { lang, body: codeEl.textContent, codeEl, pre, index: i },
            ctx,
          );
          i += 1;
        }
        // Math is text-node based; the registry's `math` renderer
        // handles it via dispatch(lang='math', area, ...).
        if (marpitDiv.textContent && /\$/.test(marpitDiv.textContent)) {
          await dispatch(
            { lang: 'math', area: marpitDiv, body: '', codeEl: null, pre: null, index: -1 },
            ctx,
          );
        }
        // ROADMAP v1.1 #23 — stamp stable slide anchors before we
        // extract the per-slide SVGs.
        stampSlides(marpitDiv);
        const svgs = Array.from(marpitDiv.querySelectorAll(':scope > svg'));
        // Mark each as a slide for fitToStage() to find.
        svgs.forEach((svg, i) => {
          svg.setAttribute('data-slide-index', String(i));
          // Ensure each svg sizes to its native 1280x720 viewBox.
          svg.setAttribute('width', '1280');
          svg.setAttribute('height', '720');
        });
        slideSvgs = svgs.map((svg) => svg.outerHTML);
      } else {
        // Fallback: parse the HTML and split by <section> as before.
        slideSvgs = splitRenderedIntoSlides(html);
      }
      marpReady = true;
      await tick();
      requestAnimationFrame(fitToStage);
    } catch (e) {
      console.error('[Marp]', e);
      marpError = e.message || String(e);
    }
  });

  onDestroy(() => {
    resizeObserver?.disconnect();
    window.removeEventListener('resize', fitToStage);
    // Note: we leave Marp's <style> in the head — it's small and
    // re-entering slide view is fast if it's already there. If the
    // user wants it gone they can refresh; the cost of removing and
    // re-adding it on each mount is higher than just leaving it.
  });

  /** Re-stitch the per-slide markdown back into a single Marp
   *  document. Marp expects the whole deck in one render call so
   *  it can apply per-slide theme/class directives. */
  function joinSlidesForMarp(slideArr) {
    if (!slideArr.length) return '';
    return slideArr.join('\n\n---\n\n');
  }

  /** Marp emits a single HTML string with all slides chained as
   *  `<svg><foreignObject><section>…</section></foreignObject></svg>`
   *  blocks. The exact boundary layout can vary (e.g. the polyfill
   *  script is appended at the end, sometimes the first slide
   *  shares its wrapper with the marpit `<div>`), so the safest
   *  approach is to extract every `<section>…</section>` block
   *  and re-wrap each in a fresh `<svg><foreignObject>…</foreignObject></svg>`
   *  envelope. The result is a uniform array of self-contained
   *  slide fragments we can render into separate DOM nodes. */
  function splitRenderedIntoSlides(rendered) {
    const SECTION_RE = /<section\b[\s\S]*?<\/section>/g;
    const matches = rendered.match(SECTION_RE);
    if (!matches) return [];
    return matches.map((sectionHtml) => (
      `<svg data-marpit-svg="" viewBox="0 0 1280 720">` +
      `<foreignObject width="1280" height="720">` +
      sectionHtml +
      `</foreignObject></svg>`
    ));
  }

  function fitToStage() {
    if (!stageEl || !trackEl) return;
    const cw = stageEl.clientWidth;
    const ch = stageEl.clientHeight;
    if (cw <= 0 || ch <= 0) return;
    // Fit a 16:9 slide into (cw, ch) by whichever dimension is tighter.
    // The slide stays at native 16:9 — no JS scaling, just sizing.
    let sw, sh;
    if (ch * 16 / 9 <= cw) { sh = ch; sw = sh * 16 / 9; }
    else                     { sw = cw; sh = sw * 9 / 16; }
    const gap = 24;
    trackEl.style.setProperty('--slide-w', `${sw}px`);
    trackEl.style.setProperty('--slide-h', `${sh}px`);
    trackEl.style.setProperty('--slide-gap', `${gap}px`);
    // Center the active slide on the stage's horizontal centerline. The
    // track has zero intrinsic height (absolute children), so we set top
    // explicitly and skip the translateY(-50%) trick.
    trackEl.style.setProperty('--marp-active-x', `${(cw - sw) / 2}px`);
    trackEl.style.top = `${(ch - sh) / 2}px`;
    for (let i = 0; i < slideEls.length; i++) {
      const el = slideEls[i];
      if (!el) continue;
      el.style.left = `${i * (sw + gap)}px`;
      el.style.width = `${sw}px`;
      el.style.height = `${sh}px`;
    }
  }

  function next() {
    if (current < slides.length - 1) current++;
  }
  function prev() {
    if (current > 0) current--;
  }
  function first() { current = 0; }
  function last() { current = slides.length - 1; }
  function exit() { onExit?.(); }

  function onKey(e) {
    // Don't intercept while user is typing in a search/editor field.
    const t = e.target;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
    if (e.key === 'ArrowRight' || e.key === ' ' || e.key === 'PageDown') {
      e.preventDefault(); next();
    } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
      e.preventDefault(); prev();
    } else if (e.key === 'Home') {
      e.preventDefault(); first();
    } else if (e.key === 'End') {
      e.preventDefault(); last();
    } else if (e.key === 'Escape') {
      e.preventDefault(); exit();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="slide-stage" bind:this={stageEl}>
  <div class="slide-track" bind:this={trackEl}
       style="top: var(--marp-track-top, 0px); transform: translateX(calc(var(--marp-active-x, 0px) - {current} * (var(--slide-w, 1280px) + var(--slide-gap, 24px))));">
    {#if marpError}
      <div class="slide-error">Couldn't render slides: {marpError}</div>
    {:else if !rendered}
      <div class="slide-loading">Loading slide deck…</div>
    {:else}
      {#each slideSvgs as svg, i}
        <div class="slide-svg"
             class:active={i === current}
             bind:this={slideEls[i]}>
          <div class="marpit">
            {@html svg}
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<div class="slide-bar">
  <button class="slide-nav" onclick={prev} disabled={current === 0} title="Previous slide" aria-label="Previous slide">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"/></svg>
  </button>
  <span class="slide-counter">{current + 1} <em>/</em> {slides.length}</span>
  <div class="slide-dots" role="tablist" aria-label="Jump to slide">
    {#each slides as _, i}
      <button
        class="slide-dot"
        class:active={i === current}
        onclick={() => (current = i)}
        title={`Go to slide ${i + 1}`}
        aria-label={`Go to slide ${i + 1}`}
        aria-selected={i === current}
        role="tab"
      ></button>
    {/each}
  </div>
  <button class="slide-nav" onclick={next} disabled={current === slides.length - 1} title="Next slide" aria-label="Next slide">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"/></svg>
  </button>
  <button class="slide-exit" onclick={exit} title="Exit slide view (Esc)" aria-label="Exit slide view">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
  </button>
</div>

<style>
  .slide-stage {
    position: absolute;
    inset: 0;
    bottom: 60px;
    overflow: hidden;
    background: var(--surface-container-lowest);
    /* Override the legacy global .slide-stage rules in app.css
       (width: 100%; max-width: 1100px; aspect-ratio: 16/9;
       max-height: 62vh) which would otherwise clamp the stage to
       a 16:9 box and stop it from filling the reader area. */
    width: auto;
    max-width: none;
    aspect-ratio: auto;
    max-height: none;
  }
  .slide-track {
    /* Track's vertical position is set from JS (--marp-track-top)
       so the active slide sits at the stage's vertical center.
       translateX is set inline; no translateY here. */
    position: absolute;
    left: 0;
  }
  .slide-svg {
    position: absolute;
    top: 0;
    left: 0;
    background: white;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
    border-radius: 6px;
    overflow: hidden;
  }
  .slide-svg :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }
  .slide-svg.active {
    /* Active slide is fully opaque; siblings are slightly dimmed so the
       user can tell which one they're on. */
    opacity: 1;
  }
  .slide-svg:not(.active) {
    opacity: 0.4;
  }
  .slide-loading, .slide-error {
    width: 1280px;
    height: 720px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-muted);
    font-family: var(--font-sans);
    font-size: 1rem;
  }
  .slide-error {
    color: #b00020;
  }

  .slide-bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 60px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
    background: var(--surface);
    border-top: 1px solid var(--surface-variant);
  }
  .slide-nav, .slide-exit {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm, 2px);
    background: transparent;
    color: var(--ink-secondary);
    border: 1px solid var(--surface-variant);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .slide-nav svg, .slide-exit svg { width: 18px; height: 18px; }
  .slide-nav:hover:not(:disabled),
  .slide-exit:hover { background: var(--tertiary-dim); color: var(--tertiary); border-color: var(--tertiary); }
  .slide-nav:disabled { opacity: 0.35; cursor: not-allowed; }
  .slide-counter {
    font-family: var(--font-sans);
    font-size: 0.85rem;
    color: var(--ink-secondary);
    min-width: 48px;
    text-align: center;
  }
  .slide-counter em { color: var(--ink-muted); font-style: normal; margin: 0 4px; }
  .slide-dots {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .slide-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--surface-variant);
    border: none;
    padding: 0;
    cursor: pointer;
    transition: background 0.15s ease, transform 0.15s ease;
  }
  .slide-dot.active {
    background: var(--tertiary);
    transform: scale(1.4);
  }
  .slide-dot:hover { background: var(--ink-muted); }
</style>
