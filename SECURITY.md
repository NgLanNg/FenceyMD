# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| `main` branch | yes |
| latest release tag | yes |
| older tags | best-effort, no backports |

The `main` branch is the source of truth. Tagged releases get
point-fix backports for security issues; older versions are
not patched.

## Reporting a vulnerability

**Please don't open a public issue for security problems.**

Email **security@mdreader.app** (PGP key below) with:

1. A short description of the issue
2. Steps to reproduce (preferably a minimal chapter that triggers it)
3. The MD Reader version + OS + Tauri version, if known
4. Your assessment of impact (RCE? arbitrary file read? UI spoofing?)
5. Whether you intend to disclose publicly and on what timeline

We aim to acknowledge within **3 business days** and ship a fix
within **30 days** for high-severity issues. We'll coordinate a
disclosure date with you — default is 90 days from the report.

## What counts as a security issue

- The Tauri command surface (`pick_folder`, `open_folder_path`,
  `write_file`, `update_excalidraw_block`, `rename_file`,
  `print_pdf`, `save_export`, `copy_image`, `watch_folder`,
  `get_recents`, `remove_recent`, `get_progress`, `save_progress`,
  `scan_path`) escaping its intended scope — e.g. writing outside
  the chosen folder, or loading arbitrary URLs in the WebView.
- The Markdown → DOM pipeline producing XSS in the reader (we
  trust the user's own files, but if a fence type or attribute
  path can be weaponized for cross-document attack within the
  same folder, that's a bug).
- Path traversal in the file watcher, the rename command, or
  the recents list.

## What is *not* a security issue

- "I can read my own files." MD Reader is a local app; the
  chosen folder is fully readable by design.
- "The app doesn't validate Markdown." It's the user's own
  content; the renderer trusts the file.
- Performance / size / bundle complaints. Open an issue.

## Scope notes

- **No network.** MD Reader makes no network calls at runtime.
  If you observe one, that's a security issue.
- **No telemetry.** Nothing leaves the app.
- **Writes stay in the chosen folder.** The `write_file` and
  `update_excalidraw_block` commands canonicalize the parent
  directory and refuse paths that escape. If you find a bypass,
  that's a high-severity issue.

## PGP

A public key for `security@mdreader.app` will be published
alongside the first signed release. Until then, the email
itself is the channel — GitHub does not require PGP for
disclosure workflows at this stage.

## Credits

Reporters who follow responsible disclosure and give us time
to fix before going public are credited in the release notes
(unless they prefer to remain anonymous).
