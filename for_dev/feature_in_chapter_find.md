# In-chapter Find (⌘F)

## Vision & DoD (5W1H)

**What.** A small find bar at the top of the Reader that searches within the current chapter only. The user types a term; the bar shows the hit count ("1 of 5"); the matched text is highlighted in the chapter; Enter steps to the next match.

**Why.** Most "where is that in this chapter?" needs are single-chapter. The cross-chapter search is for cross-book; this one is for "I was reading this chapter and want to find a specific sentence."

**Who.** Any reader.

**When.** ⌘F opens the find bar. The bar floats at the top of the Reader and stays until the user dismisses it (Esc or the X button).

**Where.** `src/components/Reader.svelte` hosts the bar. The find logic is in the Reader's own handler.

**How (acceptance / DoD).**
- ⌘F opens the find bar; focuses the input.
- Typing a term highlights matches in the chapter and shows the hit count.
- Enter / F3 steps forward; Shift+Enter / Shift+F3 steps backward.
- The current match scrolls into view.
- The match is highlighted with a distinct background (typically yellow).
- Esc closes the bar; focus returns to the chapter.
- A second ⌘F after closing re-opens the bar with the previous query.

---

## How we implemented it

**What.** A small input + hit-count display at the top of the Reader. The search walks the chapter's text nodes (not the HTML), wraps matches in `<mark>` elements, and removes the wraps on close.

**Why this shape.** The simplest find implementation is a DOM-text-walk + wrap-in-mark. We don't use the browser's built-in find (it doesn't let us style or count). We don't use a third-party library (the cases we care about are simple substring searches).

**When.** ⌘F opens. Closes on Esc, X click, or chapter navigation.

**Where.**
- `src/components/Reader.svelte` — the bar + handler.

**How (tech).**
- **Walk**: a TreeWalker over the chapter's text nodes. For each node, `indexOf(query)` finds matches. We split the text node at match boundaries, wrap the match in a `<mark>`.
- **Hit counter**: increment on each match found. "X of Y" updates as the user types.
- **Step**: keep a `currentMatch` index. Scroll the corresponding `<mark>` into view.
- **Reset**: on close, walk the chapter again and replace all `<mark>` with their text content. Cheap and reliable.
- **The v1.0 bug**: a `/g` regex's `lastIndex` advanced across text nodes, silently skipping matches. The fix: don't use a global regex for the membership test; instead, do per-node `indexOf` and reset the search on every input change.

**Gotchas.**
- A `<mark>` element inside a `<pre><code>` block doesn't render its yellow background. We use a different style for code-block matches.
- The user's selection (a real text selection) can be confused with a match. We clear the selection on close.
- The "step forward" must work even when there are zero matches; we just don't scroll.
