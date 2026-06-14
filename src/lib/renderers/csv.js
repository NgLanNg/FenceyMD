// CSV fence renderer — parses CSV into a styled editorial <table>.
// No interactive grid, no sort, no filter (the big grid is a v1.1
// follow-up): per the calm-reading brand, tables are for reading, not
// for spreadsheet-style manipulation. The user can copy the source
// back out of the fence if they want to edit.
//
// Papaparse is lazy-loaded so the dep cost is paid only when a
// chapter actually has a ```csv fence. The first row of the CSV
// becomes the table header; subsequent rows become <tbody> rows.
// Quoted fields and embedded newlines are handled by papaparse.
//
// ROADMAP v1.1 #5 — Numeric column alignment: a column is "numeric"
// if ≥80% of its non-empty cells parse as Number. Numeric columns
// get a class so CSS can right-align + add thousands separators.
//
// ROADMAP v1.1 #6 — Row search: a small <input> at the top of each
// CSV block filters tbody rows on the fly (case-insensitive substring
// match against any cell in the row). Empty input = show all rows.
import { register } from '../registry.js';

// Memoized papaparse module — the dynamic import only runs the first
// time a ```csv fence is rendered, then every later call reuses it.
let _papaparse = null;

/**
 * Lazily import + cache the papaparse default export.
 * @returns {Promise<object>} the papaparse module.
 */
async function getPapa() {
  if (!_papaparse) _papaparse = (await import('papaparse')).default;
  return _papaparse;
}

/**
 * Build a `<td>` from a raw cell value. `textContent` (never innerHTML)
 * is the trust boundary here: CSV cells are untrusted markdown content,
 * so they are inserted as text and never parsed as HTML.
 *
 * @param {*} text — raw cell value; null/undefined render as empty.
 * @returns {HTMLTableCellElement}
 */
function cell(text) {
  const td = document.createElement('td');
  td.textContent = text == null ? '' : String(text);
  return td;
}

// Strip a raw cell to a Number if it looks like one. Accepts:
//   - plain ints/floats: 1234, 12.5, -3
//   - thousands-separated: 1,234 / 1.234,5 (EU) — we keep the comma
//     handling loose; "1,234" is accepted as 1234
//   - currency-prefixed: $1,234.50  →  1234.5
//   - percent: 12.5%
// Rejects anything with a non-numeric suffix (so a year like "1995"
// parses as 1995, but a language name "Go" doesn't).
//
// @param {*} raw — raw cell value.
// @returns {number|null} the parsed Number, or null if it isn't numeric.
function parseNumericLoose(raw) {
  if (raw == null) return null;
  const s = String(raw).trim();
  if (!s) return null;
  // Strip leading currency / symbol chars and a trailing % sign
  const cleaned = s.replace(/^[^0-9\-]+/, '').replace(/[^\d.]$/, '');
  if (!cleaned || !/^[-]?\d[\d,]*(\.\d+)?$/.test(cleaned)) return null;
  const n = Number(cleaned.replace(/,/g, ''));
  return Number.isFinite(n) ? n : null;
}

// Format a number with thousand separators. Reuses the raw text if
// parsing failed so we don't garble the cell.
//
// @param {*} raw — raw cell value.
// @returns {string} the en-US grouped number, or the original text
//   unchanged when `raw` doesn't parse as numeric.
function formatNumeric(raw) {
  const n = parseNumericLoose(raw);
  if (n == null) return raw == null ? '' : String(raw);
  // Keep the sign; int vs float choice mirrors the input.
  const isInt = Number.isInteger(n);
  const abs = Math.abs(n);
  const body = isInt ? abs.toLocaleString('en-US') : abs.toLocaleString('en-US', { maximumFractionDigits: 6 });
  return (n < 0 ? '-' : '') + body;
}

// For each column in the data rows, decide if it's "numeric enough"
// (≥80% of non-empty cells parse as Number). Returns Set<number>.
//
// @param {Array<Array<*>>} rows — parsed CSV; row 0 is the header and is
//   excluded from the ratio. Empty cells don't count toward the total.
// @returns {Set<number>} 0-based indices of the numeric columns. Empty
//   when there are fewer than 2 rows (header alone proves nothing).
function detectNumericColumns(rows) {
  if (rows.length < 2) return new Set();
  const headerLen = (rows[0] || []).length;
  const numeric = new Set();
  for (let c = 0; c < headerLen; c++) {
    let total = 0, num = 0;
    for (let r = 1; r < rows.length; r++) {
      const v = (rows[r] || [])[c];
      if (v == null || String(v).trim() === '') continue;
      total++;
      if (parseNumericLoose(v) != null) num++;
    }
    if (total > 0 && num / total >= 0.8) numeric.add(c);
  }
  return numeric;
}

// Registry manifest entry for ```csv fences. `render` mutates the DOM in
// place per the registry contract — it replaces `block.pre` with the
// editorial `.csv-block` wrapper and returns nothing.
//
// Two paths:
//   - PDF (ctx.isPdf): synchronous source-preserving fallback; papaparse
//     never runs in the Rust print pipeline (see header).
//   - Live reader: `render` returns immediately and the table is built
//     inside the `getPapa().then(...)`. The `pre.replaceWith(wrap)` is
//     therefore deferred until the parse resolves — callers must not
//     assume the DOM is swapped by the time `render` returns.
//
// `block` = { pre, body, ... } (registry block shape); `ctx` supplies
// `isPdf` and the wrapper-class overrides. A failed parse/import is
// caught and degrades to the same raw-source fallback as the PDF path.
register('csv', {
  kind: 'fence',
  load() { return getPapa(); },
  render(block, ctx) {
    const { pre, body } = block;
    if (!pre) return;
    const wrap = document.createElement('div');
    // Same class as the rest of the editorial wrappers so PDF + reader
    // pick up the existing `.csv-block` / `.chapter-markdown table` rules.
    wrap.className = ctx.csvWrapClass || ctx.wrapClassName || 'csv-block';

    // PDF side can't run papaparse, but it doesn't need to — the
    // table styling in build_print_html already targets
    // `.chapter-markdown table` from any source (showdown tables, etc.).
    // Skip the parse in the PDF path; emit a placeholder that the
    // caller can post-process if they want.
    if (ctx.isPdf) {
      // Keep the source as <pre> for now — same fallback as
      // unknown langs. The user can re-export to a more elaborate
      // printable CSV if/when it becomes a real need.
      wrap.className = 'csv-block csv-block-raw';
      const fallback = document.createElement('pre');
      fallback.textContent = body.trim();
      wrap.appendChild(fallback);
      pre.replaceWith(wrap);
      return;
    }

    getPapa().then((Papa) => {
      const parsed = Papa.parse(body.trim(), {
        skipEmptyLines: true,
      });
      const rows = parsed.data;
      if (!rows.length) {
        wrap.classList.add('csv-block-empty');
        wrap.textContent = '(empty CSV)';
        pre.replaceWith(wrap);
        return;
      }
      const numericCols = detectNumericColumns(rows);

      // ── Row search input (v1.1 #6) ──────────────────────────────────
      // A small <input> at the top of the block. Case-insensitive
      // substring match against any cell. Empty = show all rows.
      const filterInput = document.createElement('input');
      filterInput.type = 'text';
      filterInput.className = 'csv-block-filter';
      filterInput.placeholder = 'Filter rows…';
      filterInput.setAttribute('aria-label', 'Filter CSV rows');
      filterInput.spellcheck = false;
      filterInput.autocomplete = 'off';
      // Prevent the reader's chapter-nav key handler from firing
      // while the user is typing into the filter.
      filterInput.addEventListener('keydown', (e) => e.stopPropagation());
      wrap.appendChild(filterInput);

      const table = document.createElement('table');
      const thead = document.createElement('thead');
      const headRow = document.createElement('tr');
      for (let c = 0; c < rows[0].length; c++) {
        const th = document.createElement('th');
        const h = rows[0][c];
        th.textContent = h == null ? '' : String(h);
        if (numericCols.has(c)) th.classList.add('csv-num');
        headRow.appendChild(th);
      }
      thead.appendChild(headRow);
      table.appendChild(thead);

      // Build tbody once; visibility is toggled by the filter input.
      // Keeping the original <tr>s in the DOM means the count note
      // (at the bottom) can always report the true row count, not
      // the visible-after-filter count.
      let tbody = null;
      let dataRows = 0;
      if (rows.length > 1) {
        tbody = document.createElement('tbody');
        for (let i = 1; i < rows.length; i++) {
          const tr = document.createElement('tr');
          // Pre-compute the searchable text once per row, not per keystroke.
          const searchText = rows[i].map((v) => v == null ? '' : String(v).toLowerCase()).join('\u0001');
          tr.dataset.csvSearch = searchText;
          for (let c = 0; c < rows[0].length; c++) {
            const td = cell(rows[i][c]);
            if (numericCols.has(c)) {
              td.classList.add('csv-num');
              td.textContent = formatNumeric(rows[i][c]);
            }
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
        table.appendChild(tbody);
        dataRows = rows.length - 1;
      }
      wrap.appendChild(table);

      // Footer line — number of data rows. Quiet, monospace, no chrome.
      const note = document.createElement('div');
      note.className = 'csv-block-note';
      note.dataset.totalRows = String(dataRows);
      note.textContent = `${dataRows} row${dataRows === 1 ? '' : 's'}`;
      wrap.appendChild(note);

      // Filter handler. Live, no debounce — the dataset per block is
      // small (CSV-as-prose, not the 50k-row data-grid use case).
      const applyFilter = () => {
        if (!tbody) return;
        const q = filterInput.value.trim().toLowerCase();
        let shown = 0;
        for (const tr of tbody.children) {
          const match = !q || (tr.dataset.csvSearch || '').includes(q);
          tr.hidden = !match;
          if (match) shown++;
        }
        note.textContent = q
          ? `${shown} / ${dataRows} row${dataRows === 1 ? '' : 's'}`
          : `${dataRows} row${dataRows === 1 ? '' : 's'}`;
        // Highlight to the user that the table is filtered.
        wrap.classList.toggle('csv-block-filtered', !!q);
      };
      filterInput.addEventListener('input', applyFilter);

      pre.replaceWith(wrap);
    }).catch((e) => {
      // Parse deps failed — leave the source as a pre so the user
      // can still see the data.
      console.warn('[renderer:csv]', e?.message || e);
      wrap.className = 'csv-block csv-block-raw';
      const fallback = document.createElement('pre');
      fallback.textContent = body.trim();
      wrap.appendChild(fallback);
      pre.replaceWith(wrap);
    });
  },
});
