// Pure helpers ported from the original reader: filename parsing, sorting,
// and building the chapter index/tree. No DOM, no framework — easy to test.
//
// Responsibility: turn the flat list of markdown file records that Rust hands
// back (`{ path, name, content }`) into the ordered, human-labelled structures
// the Svelte UI binds to — the flat `folderMeta`, the grouped `groupMeta`, and
// the nested tree from `buildFolderTree`. All ordering decisions (chapter
// numbers, group/folder sort) live here so the UI never re-sorts.
//
// Key conventions a maintainer must know:
// - Filenames are the source of truth for both ordering and display. Chapter
//   numbers are parsed heuristically from names like `ch01-foo.md` or `1. foo`;
//   files with no recognizable number sort last (sentinel 999, see numFromName).
// - "Group" = the top-level path segment when a file lives in a subfolder
//   (`partA/partB.md` -> group "partA"). Dotfiles/dot-folders are skipped.
// - Everything here is deterministic and side-effect free except
//   `sortGroupItems`, which sorts its argument arrays in place.

/**
 * Extract a chapter number from a filename for ordering purposes.
 *
 * @param {string} name - A filename, e.g. `ch01-intro.md` or `3. setup.md`.
 * @returns {number} The parsed number, or 999 when none is found.
 *
 * Matches either a `ch`/`ch.` prefix followed by digits, or digits immediately
 * before a literal `.` (so `3.foo` and `ch3` both work). The 999 sentinel is
 * deliberate: unnumbered files sort *after* numbered ones rather than at 0.
 */
export function numFromName(name) {
  const m = name.match(/ch\.?(\d+)|(\d+)\./);
  return m ? parseInt(m[1] || m[2], 10) : 999;
}

/**
 * Derive a human-readable, Title-Cased label from a filename.
 *
 * @param {string} name - A filename, e.g. `ch01-getting_started.md`.
 * @returns {string} A display label, e.g. `Getting Started`.
 *
 * Strips the `.md` extension and any `chNN` prefix, then turns `-`/`_` into
 * spaces and capitalizes each word. Note it does NOT strip a bare leading
 * number (e.g. `3. foo`) — only the `ch`-style prefix — so the numeric prefix
 * is handled separately by shortTitle.
 */
export function labelFromName(name) {
  return name
    .replace(/\.md$/i, '')
    .replace(/^ch\.?\d+[-_. ]*/i, '') // strip ch01- / ch01_ prefix
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

/**
 * Build the sidebar title for a chapter: `N. Label` when numbered, else `Label`.
 *
 * @param {string} name - A filename.
 * @returns {string} e.g. `1. Getting Started`, or `Appendix` for unnumbered files.
 *
 * Uses the 999 sentinel from numFromName as the "no number" signal — unnumbered
 * files get a bare label with no leading `999.`.
 */
export function shortTitle(name) {
  const num = numFromName(name);
  const label = labelFromName(name);
  return num < 999 ? `${num}. ${label}` : label;
}

/**
 * Return a copy of the group map with keys reordered by their numeric content.
 *
 * @param {Object<string, any>} gm - Map keyed by group (folder) name.
 * @returns {Object<string, any>} A new object with the same entries, keys
 *   sorted ascending by the digits found in each name.
 *
 * Relies on JS object insertion order to carry the sort. Group names with no
 * digits collapse to 0 and therefore sort first; this is a numeric-only sort,
 * not a lexical tiebreak, so two names sharing a number keep insertion order.
 */
export function sortGroups(gm) {
  const sorted = {};
  Object.keys(gm)
    .sort((a, b) => {
      const na = parseInt(a.replace(/[^0-9]/g, '')) || 0;
      const nb = parseInt(b.replace(/[^0-9]/g, '')) || 0;
      return na - nb;
    })
    .forEach((k) => {
      sorted[k] = gm[k];
    });
  return sorted;
}

/**
 * Sort each group's item array in place.
 *
 * @param {Object<string, Array<{name: string}>>} groupMeta - Map of group name
 *   to its item list; each list is MUTATED (sorted) in place.
 *
 * Per group, if any item looks chapter-numbered (`chNN`) the whole group is
 * ordered by chapter number; otherwise it falls back to locale-aware name
 * sort. Mixed groups still go numeric — unnumbered items there land at 999.
 */
function sortGroupItems(groupMeta) {
  for (const g in groupMeta) {
    const hasCh = groupMeta[g].some((i) => /ch\.?(\d+)/i.test(i.name));
    groupMeta[g].sort((a, b) =>
      hasCh ? numFromName(a.name) - numFromName(b.name) : a.name.localeCompare(b.name)
    );
  }
}

/**
 * Build { folderName, folderMeta, groupMeta } from native Rust records
 * `{ path, name, content }`. `diskPath` keeps the full path under the root
 * (for watcher correlation + write_file + progress keys); `path` is
 * group-stripped to match the renderer's expectations.
 *
 * @param {string} name - Selected folder's display name; falls back to
 *   'Selected Folder' when empty.
 * @param {Array<{path: string, name: string, content: string}>} records -
 *   Flat file list from Rust; `path` is relative to the opened root.
 * @returns {{folderName: string, folderMeta: Array, groupMeta: Object}}
 *   `folderMeta` is every (non-dotfile) item flat; `groupMeta` holds only the
 *   subfoldered items, keyed and sorted by group.
 *
 * Edge cases: items whose top-level segment starts with `.` are skipped
 * (hidden files/folders like `.git`, `.obsidian`). A file at the root is
 * `grouped: false` and absent from `groupMeta`; only nested files are grouped.
 */
export function buildIndexFromRecords(name, records) {
  const folderMeta = [];
  const groupMeta = {};
  for (const r of records) {
    const parts = r.path.split('/');
    const folderPrefix = parts[0];
    // Skip hidden files/dirs (.git, .obsidian, dot-prefixed) — never user content.
    if (folderPrefix.startsWith('.')) continue;
    const isGrouped = parts.length > 1;
    const relativePath = isGrouped ? parts.slice(1).join('/') : r.name;
    const item = {
      path: relativePath,
      diskPath: r.path,
      name: r.name,
      content: r.content,
      grouped: isGrouped,
    };
    folderMeta.push(item);
    if (isGrouped) {
      (groupMeta[folderPrefix] ||= []).push(item);
    }
  }
  const sorted = sortGroups(groupMeta);
  sortGroupItems(sorted);
  return { folderName: name || 'Selected Folder', folderMeta, groupMeta: sorted };
}

/**
 * Group a flat item list into a nested folder tree for the sidebar.
 *
 * @param {Array<{path: string}>} items - Items with a `/`-separated `path`.
 * @returns {Array<Object>} Top-level nodes. A *folder* node has `name` +
 *   `children` (recursively the same shape); a *file* node has `name`, `path`,
 *   and the original `item`. At every level folders come first (sorted by name),
 *   then files (sorted by chapter number, name as tiebreak).
 *
 * The presence of `path` is the file-vs-folder discriminant throughout (only
 * leaf/file nodes carry it), which is why sorting and recursion branch on it.
 */
export function buildFolderTree(items) {
  const root = { name: '', children: {} };
  for (const item of items) {
    const parts = item.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      if (!node.children[part]) {
        node.children[part] = {
          name: part,
          folderPath: parts.slice(0, i + 1).join('/'),
          children: {},
        };
      }
      node = node.children[part];
    }
    const fileName = parts[parts.length - 1];
    node.children[fileName] = { name: fileName, path: item.path, item };
  }

  function treeToArray(node) {
    const folders = [];
    const files = [];
    for (const child of Object.values(node.children)) {
      (child.path ? files : folders).push(child);
    }
    folders.sort((a, b) => a.name.localeCompare(b.name));
    files.sort((a, b) => {
      const an = numFromName(a.name);
      const bn = numFromName(b.name);
      return an !== bn ? an - bn : a.name.localeCompare(b.name);
    });
    const result = [...folders, ...files];
    for (const child of result) {
      if (!child.path) child.children = treeToArray(child);
    }
    return result;
  }
  return treeToArray(root);
}
