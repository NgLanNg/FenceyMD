// Cross-chapter full-text search.
//
// One MiniSearch index for the open book. The index is rebuilt whenever
// `folderMeta` changes (initial open + `library-changed` watcher event).
// We index three fields — title, stripped body, fence bodies — with
// title boosted so a chapter-title hit ranks above a body hit.
//
// The reader's existing in-chapter search handles the "first match in
// the open chapter" highlight; the panel here only does the cross-chapter
// discovery. When the user picks a result, the panel sets a shared
// "pending search" string that the Reader picks up on mount to populate
// its in-chapter search bar — so the match gets highlighted automatically.
import MiniSearch from 'minisearch';
import { labelFromName } from './index.js';

// Module-singleton index + doc store for the one open book. There is
// at most one book open at a time, so a module-level singleton is the
// simplest correct model; `buildSearchIndex` fully replaces both, and
// `clearSearchIndex` tears them down. Callers must build before search.
let _ms = null;
let _docs = [];

// Fenced code block: ```lang\n…``` . Capture group 1 is the fence body.
// `g` flag means callers MUST reset `.lastIndex` before reuse (see
// `extractFences`) — the regex object is shared across all calls.
const FENCE_RE = /```[a-zA-Z0-9_-]*\n([\s\S]*?)```/g;

/** Strip markdown chrome, keep readable text. Inline code, links, and
 *  emphasis become their inner text. Headings, blockquotes, list bullets
 *  and HRs are dropped. Math delimiters become whitespace (the katex
 *  source is not useful for cross-chapter search). */
function stripMarkdownChrome(text) {
  return text
    // Fences: drop the ```lang / ``` markers but keep the body
    .replace(/```[a-zA-Z0-9_-]*\n/g, '')
    .replace(/```/g, '')
    // Front-matter (--- key: value ---)
    .replace(/^---\n[\s\S]*?\n---\n?/m, '\n')
    // Math display + inline: drop the delimiters
    .replace(/\$\$[\s\S]*?\$\$/g, ' ')
    .replace(/\$[^$\n]+\$/g, ' ')
    // HTML blocks (svg/div/etc. fences) — keep the inner text
    .replace(/<[^>]+>/g, ' ')
    // Headings, blockquotes, list bullets, HR
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/^>\s+/gm, '')
    .replace(/^[-*+]\s+/gm, '')
    .replace(/^\d+\.\s+/gm, '')
    .replace(/^---+\s*$/gm, '')
    // Emphasis + inline code
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/_([^_]+)_/g, '$1')
    // Links + images — keep label/alt
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    // Collapse whitespace
    .replace(/\s+/g, ' ')
    .trim();
}

/** Chapter title for ranking/display: the first `# ` heading if present,
 *  else a human label derived from the filename. Result is trimmed. */
function extractTitle(content, name) {
  const m = content.match(/^#\s+(.+)$/m);
  return (m ? m[1] : labelFromName(name)).trim();
}

/** Concatenate every fenced code block's body (no fence markers) into one
 *  string, blank-line separated. Indexed as its own field so code-only
 *  terms are searchable but don't dominate prose relevance. */
function extractFences(content) {
  const out = [];
  let m;
  // Reset regex state — the global regex is shared via FENCE_RE
  FENCE_RE.lastIndex = 0;
  while ((m = FENCE_RE.exec(content)) !== null) {
    out.push(m[1]);
  }
  return out.join('\n\n');
}

/** Map a `{ path, name, content }` chapter item to the indexed document
 *  shape. `path` doubles as the stable `id` (one doc per chapter file).
 *  `body` and `fenceText` are the search fields; the raw markdown is
 *  intentionally not retained (callers re-derive snippets from `body`). */
function toDoc(item) {
  const title = extractTitle(item.content, item.name);
  const body = stripMarkdownChrome(item.content);
  const fenceText = extractFences(item.content);
  return {
    // The search index needs unique IDs. The renderer's `path` is
    // group-stripped (e.g. two top-level `README.md` files both
    // have `path: 'README.md'`), which collides on MiniSearch's
    // required unique-id constraint. `diskPath` is the full
    // group-prefixed relative path from the scan, which is unique
    // within a folder. We use it for the index id; the renderer
    // keeps using the group-stripped `path` for routing.
    id: item.diskPath || item.path,
    path: item.path,
    name: item.name,
    title,
    body,
    fenceText,
  };
}

/** Build (or rebuild) the index from a list of `{ path, name, content }` items. */
export function buildSearchIndex(items) {
  _docs = items.map(toDoc);
  _ms = new MiniSearch({
    idField: 'id',
    fields: ['title', 'body', 'fenceText'],
    storeFields: ['id', 'path', 'name', 'title', 'body', 'fenceText'],
    searchOptions: {
      prefix: true,
      fuzzy: 0.2,
      boost: { title: 3, body: 1, fenceText: 1 },
      combineWith: 'AND',
    },
  });
  _ms.addAll(_docs);
}

/** Empty out the index (used on app close, if ever). */
export function clearSearchIndex() {
  _ms = null;
  _docs = [];
}

/** Look up the doc for a path. Accepts either the group-stripped
 *  `path` (used by routing) or the full `diskPath` (used as the
 *  search index id). We try `id` first, then `path`, so the
 *  snippet helper works no matter which form the caller passes. */
export function getDoc(path) {
  return _docs.find((d) => d.id === path || d.path === path) || null;
}

/** Run a query, return ranked results. */
export function runSearch(query, limit = 50) {
  if (!_ms) return [];
  const q = (query || '').trim();
  if (!q) return [];
  return _ms.search(q).slice(0, limit);
}

/** Make a snippet around the first occurrence of `query` in the given
 *  text. Returns { text, matchStart, matchEnd } so the caller can
 *  render a <mark> without re-scanning. */
export function makeSnippet(text, query, opts = {}) {
  const { windowChars = 80, maxLength = 220 } = opts;
  if (!text) return { text: '', matchStart: -1, matchEnd: -1 };
  const lower = text.toLowerCase();
  const q = (query || '').trim().toLowerCase();
  if (!q) {
    const t = text.length > maxLength ? text.slice(0, maxLength) + '…' : text;
    return { text: t, matchStart: -1, matchEnd: -1 };
  }
  const idx = lower.indexOf(q);
  if (idx < 0) {
    // No exact substring — fall back to head of the text. MiniSearch may
    // still match via prefix/fuzzy, but the snippet is best-effort.
    const t = text.length > maxLength ? text.slice(0, maxLength) + '…' : text;
    return { text: t, matchStart: -1, matchEnd: -1 };
  }
  const start = Math.max(0, idx - windowChars);
  const end = Math.min(text.length, idx + q.length + windowChars);
  let snippet = text.slice(start, end);
  let matchStart = idx - start;
  let matchEnd = matchStart + q.length;
  if (start > 0) { snippet = '…' + snippet; matchStart += 1; matchEnd += 1; }
  if (end < text.length) snippet += '…';
  return { text: snippet, matchStart, matchEnd };
}

export { stripMarkdownChrome, extractTitle, extractFences };
