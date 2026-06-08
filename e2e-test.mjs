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

await browser.close();

const failed = results.filter(r => r[0] === 'FAIL');
console.log(`\n${'='.repeat(50)}\nRESULT: ${results.length - failed.length}/${results.length} passed`);
if (failed.length) { console.log('FAILURES:'); failed.forEach(f => console.log('  ✗', f[1], '→', f[2])); process.exit(1); }
else console.log('ALL FEATURES VERIFIED ✓');
