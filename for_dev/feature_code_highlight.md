# Code highlight (Shiki)

## Vision & DoD (5W1H)

**What.** A ` ```lang ` code block renders with syntax highlighting (Shiki), a language label in the top-right, and a "copy" button. The user can paste any of dozens of languages and the renderer figures out the lexer.

**Why.** Code samples are common in technical books. Plain text is fine for short snippets, but anything 20+ lines needs highlighting to be scannable.

**Who.** Authors writing technical content. Readers who want to copy snippets.

**When.** A chapter with ` ```js `, ` ```ts `, ` ```rust `, etc. fences is opened. Shiki is loaded lazily on first use (it's ~1 MB compressed).

**Where.** `src/lib/renderers/shiki.js` is the renderer. The rendered block has a `.shiki-block` class for styling and a `.shiki-copy` button for the copy action.

**How (acceptance / DoD).**
- Code fences render with the right lexer for the language tag.
- A "Copy" button appears in the top-right; clicking copies the raw text.
- The language label appears in the top-right.
- The block respects the active theme (light/dark via Shiki's dual themes).
- A code block larger than the chapter content width scrolls horizontally (no wrapping that would re-flow the code).

---

## How we implemented it

**What.** A renderer that uses Shiki to produce a dual-theme (light + dark) code HTML. The copy button is plain JS that calls `navigator.clipboard.writeText()`.

**Why this shape.** Shiki ships the lexers pre-bundled — no client-side language detection. The dual-theme output is a single HTML string with both light and dark spans, switched via CSS based on `data-theme` on `<html>`. This avoids re-rendering on theme flip.

**When.** Lazy-loaded. The first time a Shiki-fenced chapter opens, the import takes ~100 ms; subsequent chapters are instant.

**Where.**
- `src/lib/renderers/shiki.js` — the renderer.
- `src/lib/renderers/manifest.json` — declares which languages get Shiki treatment (default: anything in the `lang-` prefix that matches a Shiki grammar).

**How (tech).**
- **Shiki**: `shiki` v1. We use the dual-theme API: `codeToHtml(code, { lang, themes: { light: 'github-light', dark: 'github-dark' } })`. The result has spans with `style="--shiki-light:...; --shiki-dark:..."` which CSS swaps based on `[data-theme]`.
- **Lazy import**: `await import('shiki')` inside the renderer, not at module top. Keeps the initial JS bundle small.
- **Copy button**: a small DOM element appended to the block; on click, `navigator.clipboard.writeText(rawText)`. Shows "Copied!" briefly.
- **No reflow on theme flip**: because the spans already contain both colors, the theme change is a CSS-variable swap, not a re-render.

**Gotchas.**
- Shiki's grammar list is huge; we let Shiki dynamically load grammars on demand. The first render of a rare language is slow (~200 ms), then cached.
- A common bug: forgetting the language tag (` ``` ` instead of ` ```js `) defaults to plain text. We don't auto-detect — that would be expensive.
- A "code theme picker" in Settings lets the user pick between github-light/dark/nord. This is a *separate* config from the app theme.
