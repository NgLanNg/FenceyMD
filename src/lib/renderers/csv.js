// CSV fence renderer — parses CSV into a styled editorial <table>.
// No interactive grid, no sort, no filter: per the calm-reading brand,
// tables are for reading, not for spreadsheet-style manipulation. The
// user can copy the source back out of the fence if they want to edit.
//
// Papaparse is lazy-loaded so the dep cost is paid only when a
// chapter actually has a ```csv fence. The first row of the CSV
// becomes the table header; subsequent rows become <tbody> rows.
// Quoted fields and embedded newlines are handled by papaparse.
import { register } from '../registry.js';

let _papaparse = null;
async function getPapa() {
  if (!_papaparse) _papaparse = (await import('papaparse')).default;
  return _papaparse;
}

function cell(text) {
  const td = document.createElement('td');
  td.textContent = text == null ? '' : String(text);
  return td;
}

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
      const table = document.createElement('table');
      const thead = document.createElement('thead');
      const headRow = document.createElement('tr');
      for (const h of rows[0]) {
        const th = document.createElement('th');
        th.textContent = h == null ? '' : String(h);
        headRow.appendChild(th);
      }
      thead.appendChild(headRow);
      table.appendChild(thead);

      if (rows.length > 1) {
        const tbody = document.createElement('tbody');
        for (let i = 1; i < rows.length; i++) {
          const tr = document.createElement('tr');
          for (const v of rows[i]) tr.appendChild(cell(v));
          tbody.appendChild(tr);
        }
        table.appendChild(tbody);
      }
      wrap.appendChild(table);

      // Footer line — number of data rows. Quiet, monospace, no chrome.
      const note = document.createElement('div');
      note.className = 'csv-block-note';
      const dataRows = rows.length - 1;
      note.textContent = `${dataRows} row${dataRows === 1 ? '' : 's'}`;
      wrap.appendChild(note);

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
