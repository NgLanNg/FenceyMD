---
title: CSV
---

# CSV

Inline CSV becomes a real `<table>` — same calm-reading treatment
as the rest of the editorials. First row is the header, the rest
is data, and a quiet note at the bottom says how many rows
parsed.

```csv
language,year,paradigm
JavaScript,1995,multi-paradigm
Python,1991,multi-paradigm
Rust,2010,systems
Go,2009,systems
Haskell,1990,pure functional
```

## What it's for

Tables of language features, library comparisons, perf benchmarks,
price lists, a list of every chapter in a book — anything where the
source is "rows of structured text" and the user wants to read it
without leaving the chapter.

No interactive grid, no sort, no filter. The fence is a *table in
prose*, not a spreadsheet. If the data wants to be interactive, it
belongs in a different file.

## Edge cases

Empty fields render as blanks, not as `null`:

```csv
key,value
alpha,1
beta,
gamma,3
```

A long header wraps; the table itself scrolls horizontally on
narrow screens.

```csv
metric,Q1 2025,Q2 2025,Q3 2025,Q4 2025,total
active readers,12480,13102,14009,15210,54801
chapters read,42811,46392,50118,55704,195025
PDF exports,1283,1411,1592,1804,6090
```

If parsing fails the source stays visible as a `<pre>` so the
data isn't lost.
