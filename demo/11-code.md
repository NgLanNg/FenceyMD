---
title: Code
---

# Code

Fenced code blocks render with **shiki** — the same highlighter
VS Code uses. Theme follows the app (github-light / github-dark).

## JavaScript

```js
const fib = (n) => (n < 2 ? n : fib(n - 1) + fib(n - 2));
console.log(fib(10)); // 55
```

## TypeScript

```ts
type Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };
const ok = <T,>(value: T): Result<T> => ({ ok: true, value });
```

## Python

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class Point:
    x: float
    y: float

    def distance_to(self, other: "Point") -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5
```

## Rust

```rust
fn main() {
    let nums: Vec<i32> = (1..=10).collect();
    let sum: i32 = nums.iter().sum();
    println!("sum = {sum}");
}
```

## SQL

```sql
SELECT u.id, u.email, COUNT(o.id) AS orders
FROM users u
LEFT JOIN orders o ON o.user_id = u.id
WHERE u.created_at >= NOW() - INTERVAL '30 days'
GROUP BY u.id, u.email
ORDER BY orders DESC
LIMIT 20;
```

## Bash

```bash
# Find chapters modified in the last 7 days
find . -name "*.md" -mtime -7 -not -path "./node_modules/*"
```

## JSON

```json
{
  "name": "fenceymd",
  "version": "1.0.0",
  "features": ["math", "syntax-highlight", "slides", "pdf"]
}
```

## YAML

```yaml
renderer:
  name: shiki
  themes:
    light: github-light
    dark: github-dark
  languages:
    - js
    - ts
    - py
    - rs
    - go
    - sql
```

## Why it matters

Tech books live or die on code. Plain `<pre>` monospace gets
the job done but is visually flat. shiki gives every code
block the same look the reader sees in their editor — the
already-familiar GitHub palette, theme-aware, zero plugins.
