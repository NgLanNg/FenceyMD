# CSV

## Vision & DoD (5W1H)

**What.** A ` ```csv ` block renders as a real `<table>`. The first row is the header; the rest is data. The block shows row count and (optionally) numeric alignment for columns where ≥80% of cells parse as numbers.

**Why.** Authors often have small tables — language comparisons, library versions, perf benchmarks. A fenced CSV block is *the* lowest-friction way to embed tabular data in markdown. It also round-trips: copy a CSV, paste it into a chapter, get a styled table.

**Who.** Anyone with structured text data who wants it readable in prose. Common in technical books, research notes, comparison docs.

**When.** A chapter with ` ```csv ` fences opens. CSV rendering is fast (no async work), so it's part of the main render pass.

**Where.** `src/lib/renderers/csv.js` is the renderer. Output has a `.csv-block` class for styling.

**How (acceptance / DoD).**
- A CSV block renders as a `<table>` with the first row as `<th>` and the rest as `<td>`.
- A footer note says how many rows parsed.
- Numeric columns (≥80% cells parse as number) are right-aligned with thousands separators.
- The table scrolls horizontally if it overflows the chapter content width.
- A "row search" filter appears above the table (v1.1 #6) for tables with >5 rows.

---

## How we implemented it

**What.** A renderer that:
1. Splits the fence body on newlines.
2. Splits each line on commas (with quote handling for commas inside quoted fields).
3. Emits an HTML `<table>` with `<thead>` from row 0 and `<tbody>` from rows 1..N.
4. Walks each column, computes the "numeric ratio" (count of cells that parse as a number / total cells), and if ≥80%, applies a `.csv-col-numeric` class.

**Why this shape.** We don't pull in a CSV parsing library for this — the cases we care about (small, hand-written CSVs) don't have edge cases like escaped quotes inside fields. A 30-line parser is enough. The 80% threshold for "numeric column" detection is a heuristic that works well in practice (avoids "yes/no" or "1/2/3/y" being mis-classified as numeric).

**When.** Synchronous in the main render pass.

**Where.**
- `src/lib/renderers/csv.js` — the renderer.
- `src/lib/renderers/manifest.json` — declares `csv` as a known fence.

**How (tech).**
- **Parse**: simple state machine, line-by-line, with a flag for "inside quotes." If we encounter malformed input (e.g. unclosed quote), we fall back to a plain `<pre>` with the original body.
- **Render**: `JSON.stringify` is not used; we build the html string with template literals to keep the output readable in dev tools.
- **Numeric detection**: a per-column scan during render. We use `Number(cell)` and check for `NaN`; if ≥80% are numbers, mark numeric.
- **Row search** (v1.1 #6): a Svelte component that mounts a `<input>` above the table; typing filters rows by substring match.
- **Numeric alignment** (v1.1 #5): CSS `text-align: right` on `.csv-col-numeric td` + `font-variant-numeric: tabular-nums` for column alignment.

**Gotchas.**
- The renderer assumes LF line endings; CR or CRLF breaks it. We strip `\r` upfront.
- A common authoring mistake: putting trailing whitespace at the end of a CSV row, which creates a phantom empty column. We trim each cell.
- The "full data grid" feature (sort, paginate, export) was deferred to a future version — the current table is "for reading in prose," not a spreadsheet.
