// Stable block-anchor walker — ROADMAP v1.1 #23.
//
// After all renderers have mutated the DOM (shiki → `.shiki`, mermaid →
// `<pre class="mermaid"><svg>…`, excalidraw → `.excalidraw-block`,
// math → `<span class="katex">` / `.katex-display`, csv → `.csv-block`,
// svg → `.svg-block`, html → `.html-block`, …), this walker walks the
// area in document order and assigns each anchorable block a stable
// `data-md-anchor` attribute, scoped per kind and 1-based.
//
// Format (per ROADMAP #23 spec):
//   - Paragraph:   data-md-anchor="para-12"
//   - Heading:     data-md-anchor="h2-3"   (level + index, per-level counter)
//   - Code block:  data-md-anchor="code-7"  (shiki-rendered or plain <pre>)
//   - Math inline: data-md-anchor="eq-2"
//   - Math block:  data-md-anchor="eq-block-1"
//   - Mermaid:     data-md-anchor="mermaid-3"
//   - SVG:         data-md-anchor="svg-2"
//   - HTML fence:  data-md-anchor="html-1"
//   - Excalidraw:  data-md-anchor="excalidraw-1"
//   - CSV:         data-md-anchor="csv-1"
//   - Slide:       data-md-anchor="slide-3" (Marpit — see `stampSlides`)
//
// Why this exists:
//   1. ROADMAP #20 (link-to-md nav) and #22 (paragraph tracking) need a
//      stable address for every block the user might want to reference.
//   2. The v2 AI vision (ROADMAP #26) is anchor-shaped: an agent says
//      "edit anchor mermaid-3:nodeA", and the editor applies a diff. We
//      can't ship that surface without stable anchors first.
//   3. The `id` attribute is already used by the auto-TOC outline pane
//      for h1/h2 scroll-jump; we don't want to overload it. `data-md-anchor`
//      is a distinct attribute, side-by-side, and never breaks hash links.
//
// Idempotency: if `data-md-anchor` is already set on an element we skip
// its subtree entirely. The walker re-runs cleanly on the same area, and
// counters are local to one `stampAnchors()` call (i.e. per chapter area),
// so the same chapter rendered twice gets the same anchors.
//
// Stability: counters are incremented in document order. As long as the
// rendered DOM order is stable (it is — showdown emits in source order
// and renderers don't reorder siblings), the anchors are stable too.

// "Top-level" means: a child of the area that isn't inside a list,
// blockquote, table, or any other non-flow container. Paragraphs
// nested in those are still flow text and don't get their own anchor.
// The walker stops descending at these containers; if one of them
// happens to be a kind (e.g. <pre> for code), classify() picks it up
// at the container level before the stop check fires.
const STOP_SELECTOR = [
  'ul', 'ol', 'li', 'blockquote', 'table', 'thead', 'tbody',
  'tr', 'td', 'th', 'details', 'summary', 'nav', 'header', 'footer', 'aside',
  '.katex', '.katex-display', '.math-render', // math is a leaf
].join(', ');

// Match a class selector for each kind, in priority order (most specific
// first). The walker picks the FIRST selector that matches the element;
// this matters when an element could plausibly match more than one
// (e.g. .slide-svg-block and .html-block are mutually exclusive, but
// the order here is what the unioned selector tries to enforce).
//
// NOTE: we deliberately put `.katex` AFTER `.katex-display` so a
// display katex is captured as a block (and its child inline `.katex`
// is never visited, because we stop descending on the first match).
const KIND_SELECTORS = [
  { kind: 'eq-block',    sel: '.katex-display' },
  { kind: 'eq',          sel: '.katex' },
  { kind: 'mermaid',     sel: '.mermaid' },
  { kind: 'svg',         sel: '.svg-block, .slide-svg-block' },
  { kind: 'html',        sel: '.html-block, .slide-html-block' },
  { kind: 'excalidraw',  sel: '.excalidraw-block' },
  { kind: 'csv',         sel: '.csv-block' },
  // Shiki highlights land in a <pre class="shiki-block"> or
  // <pre class="shiki"> (defensive — both classes are emitted by
  // the renderer depending on which shiki path ran).
  { kind: 'code',        sel: '.shiki-block, .shiki' },
  // Plain code block — <pre><code class="language-X">…</code></pre>
  // that didn't get picked up by shiki. The shiki path always
  // replaces these with a .shiki/.shiki-block pre, so any <pre><code>
  // remaining in the DOM after `enhance()` is a renderer-miss case.
  { kind: 'code',        sel: 'pre:has(> code)' },
  { kind: 'h1',          sel: 'h1' },
  { kind: 'h2',          sel: 'h2' },
  { kind: 'h3',          sel: 'h3' },
  { kind: 'para',        sel: 'p' },
];

/**
 * Walk `area` in document order and assign `data-md-anchor` to every
 * anchorable block. Counters are local to this call (one per chapter
 * area) and 1-based. Idempotent: re-calling on the same DOM is a no-op.
 *
 * The walker is depth-first preorder. When it finds a kind match it
 * stamps the element and does NOT recurse into the subtree — that
 * means a `.csv-block` containing a `<table>` is stamped once as
 * `csv-N`, and the table cells don't get separate anchors.
 */
export function stampAnchors(area) {
  if (!area) return;
  const counts = Object.create(null);
  const bump = (k) => (counts[k] = (counts[k] || 0) + 1);
  visit(area);
  // Depth-first preorder visit. Returns nothing; side-effect is stamping.
  // Three exits in priority order: already-stamped subtree (idempotency),
  // a non-flow container (classify it, then stop descending), or a kind
  // match (stamp + claim the subtree). Otherwise recurse into children.
  function visit(el) {
    if (!el || el.nodeType !== 1) return;
    // Idempotency: a stamped element claims its entire subtree.
    if (el.hasAttribute && el.hasAttribute('data-md-anchor')) return;
    // Stop descending into non-flow containers, but allow them to
    // be classified themselves (e.g. a <pre> inside a <blockquote>
    // is still anchorable as code).
    if (el !== area && el.matches && el.matches(STOP_SELECTOR)) {
      classifyAndStamp(el);
      return;
    }
    // The kind match wins over the stop-check: e.g. a <pre> with a
    // <code> child matches 'code' (kind) AND nothing in STOP_SELECTOR,
    // so it falls through to the normal recurse path below — but
    // classifyAndStamp will short-circuit because we stamp + return.
    if (classifyAndStamp(el)) return;
    // No kind match — recurse into children.
    for (const child of el.children) visit(child);
  }
  // Stamp `el` with the first matching kind's anchor and return true; return
  // false if no kind matches. KIND_SELECTORS order is the tiebreak when an
  // element could match several kinds (most specific first — see its NOTE).
  function classifyAndStamp(el) {
    if (!el.matches) return false;
    for (const { kind, sel } of KIND_SELECTORS) {
      if (el.matches(sel)) {
        el.setAttribute('data-md-anchor', `${kind}-${bump(kind)}`);
        return true;
      }
    }
    return false;
  }
}

/**
 * Stamp `slide-N` on every top-level `<svg>` of a Marpit `marpit` div.
 * Marp emits one <svg><foreignObject><section>…</section></foreignObject></svg>
 * per slide as a direct child of the wrapping <div class="marpit">. The
 * SlideViewer extracts the SVGs via `querySelectorAll(':scope > svg')`
 * and clones them into the track; we stamp the anchors BEFORE extraction
 * so the data-md-anchor attribute survives the outerHTML round-trip.
 *
 * Idempotent: skips SVGs that already carry an anchor.
 */
export function stampSlides(marpitDiv) {
  if (!marpitDiv) return;
  const svgs = marpitDiv.querySelectorAll(':scope > svg');
  let i = 0;
  for (const svg of svgs) {
    i += 1;
    if (svg.hasAttribute('data-md-anchor')) continue;
    svg.setAttribute('data-md-anchor', `slide-${i}`);
  }
}
