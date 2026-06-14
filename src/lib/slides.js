// Split a markdown chapter into slides for the slide-deck view.
// Rules (in order):
//   1. Strip an optional Marp-style frontmatter block at the top
//      (`---\n...directives...\n---`). The directives aren't a slide;
//      they configure the whole deck.
//   2. If the remaining body contains any standalone `---` line,
//      split on those.
//   3. Otherwise, if it has 2+ H1 headings (`# `), split on H1
//      boundaries. The first H1 starts the first slide; later H1s
//      begin new slides.
//   4. Otherwise, treat the whole body as a single slide.
//
// Frontmatter stripping is lenient: we only treat the leading
// `---\n...directives...\n---` block as frontmatter if every
// non-empty line in between looks like a Marp directive
// (`key: value`, or `<!-- _key: value -->`). That keeps us from
// accidentally eating a real slide that's just `---` followed by
// another `---`.

const HR_RE = /^---$/m;
const H1_RE = /^# \S/gm;
// A frontmatter line is one of: a `key: value` directive, an
// `<!-- _key: value -->` HTML-comment directive, or blank.
// Anything else means we're past the frontmatter into content.
const DIRECTIVE_RE = /^\s*[a-zA-Z][\w-]*\s*:\s*\S/;
const COMMENT_DIRECTIVE_RE = /^\s*<!--\s*_[a-zA-Z][\w-]*\s*:/;
const BLANK_RE = /^\s*$/;

/**
 * Split a chapter into slide source strings using the four rules in the
 * file header (frontmatter strip → `---` split → 2+ H1 split → single).
 *
 * @param {string} markdown  Raw chapter markdown.
 * @returns {string[]} Slide sources in document order. Empty array for
 *   empty/whitespace-only input; each returned string is trimmed and
 *   non-empty (blank slides from adjacent separators are dropped).
 */
export function splitIntoSlides(markdown) {
  if (!markdown || !markdown.trim()) return [];

  const { body } = stripMarpFrontmatter(markdown);

  // Rule 1: explicit `---` separators win.
  if (HR_RE.test(body)) {
    return body
      .split(/^---$/m)
      .map((c) => c.trim())
      .filter(Boolean);
  }

  // Rule 2: H1 fallback. Need at least 2 H1s to be worth splitting.
  const h1Matches = [...body.matchAll(H1_RE)];
  if (h1Matches.length < 2) {
    const trimmed = body.trim();
    return trimmed ? [trimmed] : [];
  }

  const slides = [];
  const lines = body.split('\n');
  let current = [];
  let h1Seen = false;

  // Accumulate lines into `current`; flush on each H1 *after* the first.
  // The first H1 only sets `h1Seen` — any preamble before it stays in the
  // opening slide rather than being dropped, and we avoid emitting an empty
  // leading slide when the body starts with `# `.
  for (const line of lines) {
    if (/^# \S/.test(line)) {
      if (h1Seen && current.length > 0) {
        slides.push(current.join('\n').trim());
        current = [line];
      } else {
        current.push(line);
        h1Seen = true;
      }
      continue;
    }
    current.push(line);
  }
  if (current.length > 0) {
    const trimmed = current.join('\n').trim();
    if (trimmed) slides.push(trimmed);
  }
  return slides;
}

/** Returns true when the doc yields 2+ slides (i.e. slide mode is meaningful). */
export function hasMultipleSlides(markdown) {
  return splitIntoSlides(markdown).length > 1;
}

/**
 * Strip a leading Marp frontmatter block from `markdown`, if one is
 * present. Returns the body (without the frontmatter) and the
 * directives that were declared (so callers can inspect theme,
 * class, etc.).
 *
 * A Marp frontmatter is a leading `---\n...\n---` block whose
 * non-empty inner lines all look like directives. We deliberately
 * keep the rule strict — if a single content line sneaks into the
 * first block, we treat the whole thing as content (no stripping).
 */
export function stripMarpFrontmatter(markdown) {
  if (!markdown) return { body: '', directives: {} };
  const lines = markdown.split('\n');
  if (lines[0]?.trim() !== '---') return { body: markdown, directives: {} };

  // Find the closing `---` on its own line.
  let close = -1;
  for (let i = 1; i < lines.length; i++) {
    if (lines[i].trim() === '---') { close = i; break; }
  }
  if (close === -1) return { body: markdown, directives: {} };

  // Validate: every non-blank line in the block must look like a
  // directive (`key: value` or `<!-- _key: value -->` HTML-comment).
  const inner = lines.slice(1, close);
  const allValid = inner.every((line) =>
    BLANK_RE.test(line) || DIRECTIVE_RE.test(line) || COMMENT_DIRECTIVE_RE.test(line)
  );
  if (!allValid) return { body: markdown, directives: {} };

  // Parse `key: value` pairs (ignore HTML-comment form for now — we
  // let Marp handle those inside the rendered slides).
  const directives = {};
  for (const line of inner) {
    const m = line.match(/^\s*([a-zA-Z][\w-]*)\s*:\s*(.*?)\s*$/);
    if (m) directives[m[1]] = m[2];
  }

  const body = lines.slice(close + 1).join('\n');
  return { body, directives };
}
