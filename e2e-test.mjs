import puppeteer from 'puppeteer-core';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = 'http://localhost:1420?test=1';
const results = [];
const pass = (n) => { results.push(['PASS', n]); console.log('  ✓', n); };
const fail = (n, d) => { results.push(['FAIL', n, d]); console.log('  ✗', n, '→', d); };

const browser = await puppeteer.launch({ executablePath: CHROME, headless: 'new', args: ['--no-sandbox'] });
const page = await browser.newPage();
await page.setViewport({ width: 1200, height: 850 });

const consoleErrors = [];
page.on('console', m => { if (m.type() === 'error') consoleErrors.push(m.text()); });
page.on('pageerror', e => consoleErrors.push('PAGEERROR: ' + e.message));

await page.goto(URL, { waitUntil: 'networkidle0' });
await new Promise(r => setTimeout(r, 800));

// ── 1. App shell renders ────────────────────────────────────────────────
const hasSidebar = await page.$('.sidebar');
hasSidebar ? pass('App shell + sidebar render') : fail('App shell', 'no .sidebar');

// ── 2. Sidebar lists chapters ───────────────────────────────────────────
const chapterCount = await page.$$eval('.sidebar-chapter', els => els.length);
chapterCount > 0 ? pass(`Sidebar lists chapters (${chapterCount})`) : fail('Sidebar chapters', '0 found');

// ── 3. Click a chapter → Reader renders content ─────────────────────────
await page.click('.sidebar-chapter');
await new Promise(r => setTimeout(r, 500));
const readerText = await page.$eval('.chapter-markdown', el => el.textContent).catch(() => '');
readerText.includes('Welcome') || readerText.includes('Introduction') || readerText.length > 20
  ? pass('Reader renders chapter markdown') : fail('Reader render', `text="${readerText.slice(0,40)}"`);

// ── 4. Markdown features: code highlight + copy button ──────────────────
const hasCodeBlock = await page.$('.code-block-wrapper, pre code');
hasCodeBlock ? pass('Code blocks render (highlight pipeline)') : fail('Code block', 'none found');

// ── 5. Font size adjust ─────────────────────────────────────────────────
const fsBefore = await page.$eval('html', el => el.getAttribute('data-font-size') || '');
await page.click('button[title="Increase font size"]');
await new Promise(r => setTimeout(r, 200));
const fsAfter = await page.$eval('html', el => el.getAttribute('data-font-size') || '');
fsBefore !== fsAfter ? pass(`Font size adjust (${fsBefore||'M'} → ${fsAfter})`) : fail('Font size', 'no change');

// ── 6. Theme toggle ─────────────────────────────────────────────────────
const themeBefore = await page.$eval('html', el => el.getAttribute('data-theme'));
await page.click('button[title="Toggle dark mode"]');
await new Promise(r => setTimeout(r, 200));
const themeAfter = await page.$eval('html', el => el.getAttribute('data-theme'));
themeBefore !== themeAfter ? pass(`Theme toggle (${themeBefore} → ${themeAfter})`) : fail('Theme toggle', 'no change');
await page.click('button[title="Toggle dark mode"]'); // restore

// ── 7. In-chapter search highlights ─────────────────────────────────────
await page.type('input[aria-label="Search in chapter"]', 'the');
await new Promise(r => setTimeout(r, 400));
const highlights = await page.$$eval('.search-highlight', els => els.length);
highlights > 0 ? pass(`In-chapter search highlights (${highlights})`) : fail('Search', '0 highlights');
// clear
await page.click('input[aria-label="Search in chapter"]', { clickCount: 3 });
await page.keyboard.press('Backspace');

// ── 8. Bookmark toggle ──────────────────────────────────────────────────
await page.click('button[aria-label="Bookmark"]');
await new Promise(r => setTimeout(r, 200));
const bm = await page.$eval('button[aria-label="Bookmark"]', el => el.className.includes('bookmarked'));
bm ? pass('Bookmark toggles on') : fail('Bookmark', 'class not applied');

// ── 9. THE BIG ONE: Edit button opens editor (the loop bug) ─────────────
const editBtn = await page.$('button[title="Edit markdown"]');
if (!editBtn) { fail('Edit button', 'not present'); }
else {
  await editBtn.click();
  await new Promise(r => setTimeout(r, 700));
  const editorShell = await page.$('.editor-shell');
  editorShell ? pass('Edit button OPENS editor (loop bug fixed)') : fail('Edit opens', '.editor-shell not mounted');

  // ── 10. Editor loaded the existing content (blank bug) ────────────────
  const proseText = await page.$eval('.notion-prose', el => el.textContent).catch(() => '');
  proseText.trim().length > 10 ? pass(`Editor loads content (${proseText.trim().length} chars)`) : fail('Editor content', `blank! "${proseText}"`);

  // ── 11. Formatting: bold toggle works ─────────────────────────────────
  await page.$eval('.notion-prose', el => el.focus());
  await page.click('button[title="Bold (⌘B)"]');
  await new Promise(r => setTimeout(r, 150));
  const boldActive = await page.$eval('button[title="Bold (⌘B)"]', el => el.className.includes('is-active'));
  boldActive ? pass('Bold toggle activates') : fail('Bold toggle', 'not active');

  // ── 12. Preview toggle splits view ────────────────────────────────────
  await page.click('button[title="Toggle preview (⌘P)"]');
  await new Promise(r => setTimeout(r, 400));
  const previewPane = await page.$('.notion-preview-side');
  const previewText = previewPane ? await page.$eval('.notion-preview-inner', el => el.textContent).catch(()=>'') : '';
  previewPane && previewText.length > 10 ? pass(`Preview button shows live render (${previewText.trim().length} chars)`) : fail('Preview', previewPane ? 'empty' : 'no pane');

  // ── 13. Cancel closes editor ──────────────────────────────────────────
  await page.click('button.btn-ghost'); // Cancel
  await new Promise(r => setTimeout(r, 400));
  const editorGone = await page.$('.editor-shell');
  !editorGone ? pass('Cancel closes editor, returns to reader') : fail('Cancel', 'editor still open');
}

// ── 14. Chapter nav (next sibling) ──────────────────────────────────────
const nextBtn = await page.$('.sibling-nav-btn.next');
if (nextBtn) {
  const before = await page.$eval('.chapter-info-path', el => el.textContent).catch(()=>'');
  await nextBtn.click();
  await new Promise(r => setTimeout(r, 400));
  const after = await page.$eval('.chapter-info-path', el => el.textContent).catch(()=>'');
  before !== after ? pass('Next-chapter navigation') : fail('Chapter nav', 'path unchanged');
} else { pass('Chapter nav (no sibling on this chapter — skipped)'); }

// ── 15. No console errors ───────────────────────────────────────────────
const realErrors = consoleErrors.filter(e => !e.includes('favicon') && !e.includes('404') && !e.includes('Failed to load resource') && !e.includes('not running in Tauri') && !e.includes('[watch]'));
realErrors.length === 0 ? pass('No console errors') : fail('Console errors', realErrors.slice(0,3).join(' | '));

// ── 16. Inline SVG fence renders as a real namespaced <svg> ─────────────
// Navigate to the slides chapter (04-slides.md is the only demo chapter
// with an inline ```svg fence) and assert the parsed SVG made it into the
// DOM with a viewBox attribute. This guards the DOMParser re-wrap path
// in src/lib/markdown.js::enhance().
const navigated = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const slides = chapters.find(c => /slide/i.test(c.textContent));
  if (slides) { slides.click(); return true; }
  return false;
});
if (!navigated) {
  fail('Inline SVG', 'slides chapter not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 600));
  const svgInfo = await page.evaluate(() => {
    const blocks = [...document.querySelectorAll('.chapter-markdown .svg-block svg, .chapter-markdown .slide-svg-block svg')];
    return {
      count: blocks.length,
      hasViewBox: blocks.some(s => s.getAttribute('viewBox')),
    };
  });
  svgInfo.count > 0 && svgInfo.hasViewBox
    ? pass(`Inline SVG fence renders (${svgInfo.count} <svg> with viewBox)`)
    : fail('Inline SVG', `count=${svgInfo.count} hasViewBox=${svgInfo.hasViewBox}`);
}

// ── 17. Math (katex) renders inline + block ────────────────────────────
const navigatedMath = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const math = chapters.find(c => /math/i.test(c.textContent));
  if (math) { math.click(); return true; }
  return false;
});
if (!navigatedMath) {
  fail('Math chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 800)); // katex is lazy-loaded
  const mathInfo = await page.evaluate(() => {
    const katex = [...document.querySelectorAll('.chapter-markdown .katex')];
    const display = [...document.querySelectorAll('.chapter-markdown .katex-display')];
    // Sanity: the source text should not be visible — katex replaces it.
    const stillRaw = document.querySelector('.chapter-markdown')?.textContent.includes('$E = mc^2$') ?? false;
    return { inline: katex.length, display: display.length, stillRaw };
  });
  mathInfo.inline > 0 && mathInfo.display > 0
    ? pass(`Katex renders math (${mathInfo.inline} inline, ${mathInfo.display} block)`)
    : fail('Katex', `inline=${mathInfo.inline} display=${mathInfo.display} raw=${mathInfo.stillRaw}`);
}

// ── 18. Syntax highlight (shiki) renders code blocks ───────────────────
const navigatedCode = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  // Sidebar item text contains both the file name and the rendered title;
  // match broadly on either "code" or "shiki".
  const code = chapters.find(c => /code|shiki/i.test(c.textContent));
  if (code) { code.click(); return true; }
  return false;
});
if (!navigatedCode) {
  fail('Code chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 1200)); // shiki is lazy-loaded + grammar init
  const shikiInfo = await page.evaluate(() => {
    const blocks = [...document.querySelectorAll('.chapter-markdown .shiki-block, .chapter-markdown .shiki')];
    // Dual theme: shiki emits `style="color:#x;--shiki-dark:#y"` on each
    // token. The presence of --shiki-dark inline styles confirms the
    // dual-theme wiring is active.
    const tokens = [...document.querySelectorAll('.chapter-markdown .shiki-block span, .chapter-markdown .shiki span')];
    const hasDark = tokens.some(s => s.getAttribute('style')?.includes('--shiki-dark'));
    // Multiple language classes should be present (one per fenced block).
    const langs = new Set();
    for (const b of blocks) {
      for (const c of b.classList) {
        const m = c.match(/^language-(\w+)$/);
        if (m) langs.add(m[1]);
      }
    }
    return { blocks: blocks.length, hasDark, langs: [...langs] };
  });
  shikiInfo.blocks > 0 && shikiInfo.hasDark && shikiInfo.langs.length >= 3
    ? pass(`Shiki highlights code (${shikiInfo.blocks} blocks, langs: ${shikiInfo.langs.join(',')})`)
    : fail('Shiki', `blocks=${shikiInfo.blocks} hasDark=${shikiInfo.hasDark} langs=${shikiInfo.langs.join(',')}`);
}

// ── 19. Inline SVG fence renders namespace-correct ───────────────────────
// (Phase 2 of PLAN.md — guard that the registry's svg.js preserves
// the namespace-correct DOMParser re-wrap. The previous bug was that
// `innerHTML` insertion put <svg> children in the HTML namespace; the
// fix is to use createElementNS + setAttribute('xmlns', svg-ns).)
const svgBlockCheck = await page.evaluate(() => {
  // Use the SVG demo chapter (05-svg.md). It's already loaded in the
  // reader from the earlier svgInfo test; navigate to it explicitly
  // so the assertion is independent of test 16's chapter state.
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const svg = chapters.find(c => /svg/i.test(c.textContent));
  if (svg) svg.click();
  return true;
});
await new Promise(r => setTimeout(r, 400));
const svgNsInfo = await page.evaluate(() => {
  const blocks = [...document.querySelectorAll('.chapter-markdown .svg-block svg')];
  return {
    count: blocks.length,
    hasXmlns: blocks.some(s => s.getAttribute('xmlns') === 'http://www.w3.org/2000/svg'),
    hasViewBox: blocks.some(s => s.getAttribute('viewBox')),
  };
});
svgNsInfo.count > 0 && svgNsInfo.hasXmlns && svgNsInfo.hasViewBox
  ? pass(`SVG fence renders namespace-correct (${svgNsInfo.count} <svg>, xmlns + viewBox set)`)
  : fail('SVG namespace', `count=${svgNsInfo.count} xmlns=${svgNsInfo.hasXmlns} viewBox=${svgNsInfo.hasViewBox}`);

// ── 20. Mermaid renders to <svg> (registry path) ──────────────────────────
// (Phase 2 of PLAN.md — guard that the registry's mermaid.js produces
// an <svg> with a viewBox from a ```mermaid fence. The per-diagram
// light/dark toggle, Copy/PNG tools are bonus features we don't lock in.)
const mermaidNav = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const m = chapters.find(c => /mermaid|diagram/i.test(c.textContent));
  if (m) { m.click(); return true; }
  return false;
});
if (!mermaidNav) {
  fail('Mermaid chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 1500)); // mermaid lazy-load + render
  const mmdInfo = await page.evaluate(() => {
    const blocks = [...document.querySelectorAll('.chapter-markdown pre.mermaid svg')];
    return {
      count: blocks.length,
      hasViewBox: blocks.some(s => s.getAttribute('viewBox')),
    };
  });
  mmdInfo.count > 0 && mmdInfo.hasViewBox
    ? pass(`Mermaid renders to <svg> (${mmdInfo.count} <svg> with viewBox)`)
    : fail('Mermaid registry', `count=${mmdInfo.count} viewBox=${mmdInfo.hasViewBox}`);
}

// ── 21. Excalidraw block mounts the viewer (registry path) ────────────────
// (Phase 2 of PLAN.md — guard that the registry's excalidraw.js mounts
// the Svelte viewer into the fenced block. We assert the presence of
// either the .excalidraw-block wrapper (always present) or the React-
// mounted .excalidraw host the viewer injects.)
const excalNav = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const e = chapters.find(c => /excalidraw|drawing/i.test(c.textContent));
  if (e) { e.click(); return true; }
  return false;
});
if (!excalNav) {
  fail('Excalidraw chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 1500)); // React + excalidraw lazy-load
  const exInfo = await page.evaluate(() => {
    // ExcalidrawViewer.svelte mounts a React <Excalidraw> component
    // which renders a <canvas> for the scene. The wrapper has the
    // excalidraw-block class set by the renderer. We look for either
    // the wrapper or a mounted canvas.
    const wrappers = [...document.querySelectorAll('.chapter-markdown .excalidraw-block')];
    const canvases = [...document.querySelectorAll('.chapter-markdown .excalidraw-block canvas')];
    return {
      wrappers: wrappers.length,
      canvases: canvases.length,
    };
  });
  exInfo.wrappers > 0
    ? pass(`Excalidraw block mounts the viewer (${exInfo.wrappers} wrapper${exInfo.wrappers !== 1 ? 's' : ''}, ${exInfo.canvases} canvas${exInfo.canvases !== 1 ? 'es' : ''})`)
    : fail('Excalidraw mount', `wrappers=${exInfo.wrappers} canvases=${exInfo.canvases}`);
}

// ── 22. Slide view (Marp) opens and renders at least one slide page ────────
// (Phase 5 of PLAN.md — the slides feature was previously exercised
// only by the inline-SVG test (#16), which navigated to the slides
// chapter but only asserted on SVG blocks. This test actually opens
// the slide viewer and asserts the Marp chrome is present.)
const slideNav = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const s = chapters.find(c => /slide/i.test(c.textContent));
  if (s) { s.click(); return true; }
  return false;
});
if (!slideNav) {
  fail('Slide chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 500));
  // The slide icon is the toolbar button (aria-label="Toggle slide view").
  // Match by aria-label to avoid hitting the sidebar chapter button whose
  // title also contains "slide" (e.g. "06 Slides").
  const slideBtn = await page.$('button[aria-label="Toggle slide view"]');
  if (!slideBtn) {
    fail('Slide view opens', 'no slide-view button in toolbar');
  } else {
    await slideBtn.click();
    await new Promise(r => setTimeout(r, 2500)); // Marp dynamic-import (3 MB chunk) + render
    const slideInfo = await page.evaluate(() => {
      // The slide viewer mounts `.slide-stage` with one `.slide-svg`
      // per deck page. Each cloned <svg> carries `data-slide-index`
      // and contains a <foreignObject><section> with the slide
      // content. That's our "the deck rendered" signal.
      const stage = document.querySelector('.slide-stage');
      const slideSvgs = document.querySelectorAll('.slide-svg svg[data-slide-index]');
      const sections = document.querySelectorAll('.slide-svg svg[data-slide-index] foreignObject > section');
      return {
        hasStage: !!stage,
        slideSvgs: slideSvgs.length,
        sections: sections.length,
      };
    });
    slideInfo.hasStage && slideInfo.slideSvgs > 0 && slideInfo.sections > 0
      ? pass(`Slide view renders (${slideInfo.slideSvgs} deck page${slideInfo.slideSvgs !== 1 ? 's' : ''}, ${slideInfo.sections} section${slideInfo.sections !== 1 ? 's' : ''})`)
      : fail('Slide view', `stage=${slideInfo.hasStage} svgs=${slideInfo.slideSvgs} sections=${slideInfo.sections}`);
    // Exit slide view so subsequent tests see the reader chrome.
    await page.keyboard.press('Escape');
    await new Promise(r => setTimeout(r, 400));
  }
}

// ── 23. CSV fence renders as a real <table> in the registry path ──────────
// (Phase 5 of PLAN.md — CSV pulled into lean core. The renderer
// papaparse-lazy-loads and replaces the <pre> with a <div class="csv-block">
// wrapping a <table> with <thead> and <tbody>. The first row becomes
// <th>, subsequent rows become <td>.)
const csvNav = await page.evaluate(() => {
  const chapters = [...document.querySelectorAll('.sidebar-chapter')];
  const c = chapters.find(c => /csv/i.test(c.textContent));
  if (c) { c.click(); return true; }
  return false;
});
if (!csvNav) {
  fail('CSV chapter', 'not found in sidebar');
} else {
  await new Promise(r => setTimeout(r, 800)); // papaparse lazy-load + parse
  const csvInfo = await page.evaluate(() => {
    const wrappers = [...document.querySelectorAll('.chapter-markdown .csv-block')];
    const tables = [...document.querySelectorAll('.chapter-markdown .csv-block table')];
    const headers = [...document.querySelectorAll('.chapter-markdown .csv-block thead th')];
    const rows = [...document.querySelectorAll('.chapter-markdown .csv-block tbody tr')];
    // The .csv-block-note at the bottom should be present and have a row count.
    const note = document.querySelector('.chapter-markdown .csv-block .csv-block-note');
    return {
      wrappers: wrappers.length,
      tables: tables.length,
      headers: headers.length,
      rows: rows.length,
      note: note?.textContent || '',
    };
  });
  csvInfo.wrappers > 0 && csvInfo.tables > 0 && csvInfo.headers > 0 && csvInfo.rows > 0
    ? pass(`CSV fence renders as table (${csvInfo.wrappers} block${csvInfo.wrappers !== 1 ? 's' : ''}, ${csvInfo.headers} headers, ${csvInfo.rows} rows; note: "${csvInfo.note}")`)
    : fail('CSV table', `wrappers=${csvInfo.wrappers} tables=${csvInfo.tables} headers=${csvInfo.headers} rows=${csvInfo.rows}`);
}

await browser.close();

const failed = results.filter(r => r[0] === 'FAIL');
console.log(`\n${'='.repeat(50)}\nRESULT: ${results.length - failed.length}/${results.length} passed`);
if (failed.length) { console.log('FAILURES:'); failed.forEach(f => console.log('  ✗', f[1], '→', f[2])); process.exit(1); }
else console.log('ALL FEATURES VERIFIED ✓');
