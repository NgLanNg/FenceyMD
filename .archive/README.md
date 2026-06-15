# .archive/

Stash recovery artifacts that weren't folded into a release.

## stash-pre-rebrand-2026-06-10.patch

A patch of the unique work-in-progress from `stash@{0}` (created on
`feat/phase-2-registry`, base commit `f7a9b73`, dated 2026-06-10).
The stash was 1593 lines / 13 files, but most of it was the pre-rebrand
version of work that landed in v1.1.0 under the `fenceymd-*` namespace
(rebrand commit `cf59f9e`). This filtered patch keeps only the hunks
that are NOT already in v1.1.0 — 526 lines of unique work, useful for
recovering anything the rebrand accidentally dropped.

To recover:
```
git apply /path/to/.archive/stash-pre-rebrand-2026-06-10.patch
```

The original `stash@{0}` is still in the reflog (`git stash list`) and
can be applied for the full 1593-line diff if you want to compare
pre- and post-rebrand implementations.
