---
title: Math
---

# Math

Inline math: $E = mc^2$, $a^2 + b^2 = c^2$, or the quadratic
$\Delta = b^2 - 4ac$.

A block equation stands on its own line and centers:

$$
\int_0^1 x^2 \, dx = \frac{1}{3}
$$

A matrix renders cleanly too:

$$
A = \begin{pmatrix} a & b \\ c & d \end{pmatrix}, \quad
\det(A) = ad - bc
$$

## Why it matters

Math is half the reason tech books are hard to render in the
browser. With katex in lean core, a chapter can drop a formula
in plain `$…$` and the reader renders it — same font, same
spacing, same color as the surrounding text, light or dark.

No MathJax script tag, no render-blocking font load, no
plugin to enable. It just works.
