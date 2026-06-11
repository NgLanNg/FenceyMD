// Resolves a relative `.md` / `.html` link from the rendered Reader
// back to a chapter path in the open book.
//
// ROADMAP v1.1 #20 — Markdown link-to-md navigation. The Reader renders
// chapter markdown with showdown, which turns `[text](other.md)` into
// `<a href="other.md">text</a>`. Without interception the browser
// navigates the whole WebView to that URL (broken in Tauri; in dev
// it just reloads the Vite app). The Reader calls `resolveChapterLink`
// on every anchor click and, on a hit, calls `goChapter(...)` +
// `preventDefault()` so navigation stays inside the app.
//
// Resolution is plain file-URL style: take the current chapter's
// directory, append the href, then normalize. Examples:
//
//   currentPath   href                       resolved
//   ──────────    ─────────────────────     ─────────────────────
//   part-i/00-    01-reading.md              part-i/01-reading.md
//     welcome.md
//   part-i/00-    ../part-ii/06-slides.md    part-ii/06-slides.md
//     welcome.md
//   README.md     part-i/00-welcome.md       part-i/00-welcome.md   (already absolute)
//   part-i/00-    ./01-reading.md            part-i/01-reading.md
//     welcome.md
//   part-i/00-    01-reading#section         part-i/01-reading.md   (fragment stripped)
//     welcome.md
//   part-i/00-    01-reading?foo=bar         part-i/01-reading.md   (query stripped)
//     welcome.md
//   part-i/00-    01-reading.md              null                    (not in folderMeta)
//     welcome.md
//
// Anything we can't resolve falls through — the click is left to the
// browser. External links, fragment-only links, and links to non-md
// files are short-circuited early so we don't even try to resolve them.

/**
 * @param {string} currentPath  Path of the chapter being read (e.g. "part-i/00-welcome.md").
 *                              May be a top-level file ("README.md") or grouped ("part-i/00-welcome.md").
 * @param {string} href         The raw `href` attribute on the clicked <a>.
 * @param {Array<{path: string}>} folderMeta  Current `folderMeta` array; we match against `.path`.
 * @returns {string|null}       The resolved chapter path (matches `folderMeta[i].path`),
 *                              or `null` if the link doesn't resolve to a known chapter.
 */
export function resolveChapterLink(currentPath, href, folderMeta) {
  if (!href || typeof href !== 'string') return null;

  // 1. Ignore external + protocol-relative + data URIs.
  //    Match the common ones; let the browser take over for anything else too.
  const lower = href.toLowerCase().trim();
  if (
    lower.startsWith('http://') ||
    lower.startsWith('https://') ||
    lower.startsWith('mailto:') ||
    lower.startsWith('tel:') ||
    lower.startsWith('data:') ||
    lower.startsWith('javascript:') ||
    lower.startsWith('//')
  ) {
    return null;
  }

  // 2. Strip URL fragment + query so we can resolve the path cleanly.
  //    The Reader handles `#anchor` itself (browser scrolls), and we
  //    also drop it here so a link like `01-reading.md#sec-2` resolves
  //    to the same chapter as `01-reading.md`.
  const hashIdx = href.indexOf('#');
  const queryIdx = href.indexOf('?');
  let cutAt = href.length;
  if (hashIdx !== -1) cutAt = Math.min(cutAt, hashIdx);
  if (queryIdx !== -1) cutAt = Math.min(cutAt, queryIdx);
  const cleanHref = href.slice(0, cutAt);

  // 3. Pure fragment or empty path → let the browser handle it
  //    (the Reader's own click handler will short-circuit on `#…`).
  if (!cleanHref) return null;

  // 4. Only intercept markdown / html chapter links. Anything else
  //    (e.g. a relative path to an image) falls through to the browser.
  if (!/\.(md|markdown|html|htm)$/i.test(cleanHref)) return null;

  // 5. Resolve relative to the current chapter's directory.
  //    `currentPath` may be undefined (route changes mid-mount);
  //    treat that as the root of the book.
  const currentDir = currentPath && currentPath.includes('/')
    ? currentPath.slice(0, currentPath.lastIndexOf('/'))
    : '';

  const resolved = normalizePath(
    currentDir ? `${currentDir}/${cleanHref}` : cleanHref
  );

  // 6. Look up in folderMeta. `folderMeta[i].diskPath` is the full
  //    path (with group prefix, e.g. `part-i/01-reading.md`); `.path`
  //    is the group-stripped relative path (`01-reading.md`). The
  //    resolver works in `diskPath` space (so `../` and `./` resolve
  //    against the chapter's actual folder), so we look up by
  //    `diskPath`. We return the canonical `path` (group-stripped)
  //    so the rest of the app — which keys off `.path` — gets the
  //    right value back. Fall back to a `.path` match in case the
  //    caller built folderMeta without `diskPath` (defensive).
  if (!Array.isArray(folderMeta)) return null;
  const hit = folderMeta.find((f) => f && (f.diskPath === resolved || f.path === resolved));
  return hit ? hit.path : null;
}

// Normalize a file path: collapse `./` and `../` segments, drop
// leading `./`, normalize slashes. We don't go through `new URL`
// because the inputs are POSIX-ish paths, not URLs, and we want to
// stay file-system-shaped (the resolved string is what folderMeta
// stores as `.path`).
function normalizePath(p) {
  if (!p) return p;
  // Drop any leading `./` segments; preserve leading `/` if present
  // (we don't expect absolute paths in the book, but be defensive).
  const isAbs = p.startsWith('/');
  const parts = (isAbs ? p.slice(1) : p).split('/');
  const stack = [];
  for (const seg of parts) {
    if (!seg || seg === '.') continue;
    if (seg === '..') {
      // Don't pop past the root — keep the `..` literally so the
      // lookup just fails to match and we return null. That's the
      // right behavior: a `..` that escapes the book is a broken
      // link, not a reason to throw.
      if (stack.length === 0) return null;
      stack.pop();
    } else {
      stack.push(seg);
    }
  }
  return (isAbs ? '/' : '') + stack.join('/');
}
