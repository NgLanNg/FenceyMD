# Contributing

Thanks for considering a contribution to FenceyMD. The bar is
mostly "match the existing patterns and verify before claiming
done."

## Ground rules

1. **Read the relevant files first.** Don't edit blind. The
   codebase map: `src/components/*.svelte` (UI), `src/lib/stores/*`
   (state, split into `state` / `prefs` / `progress` / `library` /
   `files`), `src/lib/markdown.js` (render + `enhance()` for
   highlight, mermaid, diagram tools), `src-tauri/src/main.rs` (all
   Tauri commands).
2. **Match the existing patterns:**
   - Frontend: **Svelte 5 runes** (`$state`, `$derived`, `$effect`,
     `$props`). No legacy reactive declarations.
   - CSS: component-local `<style>` blocks (Svelte scoping on);
     theme via CSS custom properties in `src/app.css`.
   - Rust: 2021 edition, `cargo fmt` clean, no `unwrap()` outside
     test code, idiomatic `use` order (std → external → crate).
3. **No new dependencies without justification.** The release
   DMG is ~5 MB; weight matters. If you need a dep, call it out
   in the PR description with a one-line reason.
4. **Keep the test suite green:**
   - `cd src-tauri && cargo test`. 9 Rust unit tests.
   - `node e2e-test.mjs`. 16 Puppeteer e2e cases. Add a case
     for any new user-facing flow you touch.
5. **One commit per logical change.** Short imperative summary,
   optionally a body explaining *why*. Match the existing
   history: `Slide view: rewrite on Marp core`, `Excalidraw
   save: …`, etc.
6. **No secrets in the repo.** `.env` is gitignored. Never
   commit signing material, API keys, or paths containing
   personal data.

## Branching

- Branch from `main`. Don't push to `main` directly.
- Open a PR with a one-paragraph description of the *what* and
  the *why*. The diff speaks for itself.
- Squash or rebase-merge is fine; the maintainer will pick.

## Dev loop

```bash
npm install
npm run dev                      # browser preview at http://localhost:1420
                                 # add ?test=1 for the bundled tour book
cd src-tauri && cargo test       # Rust unit tests
node e2e-test.mjs                # Puppeteer regression suite
```

For visual work, capture a Puppeteer screenshot and look at it
in both themes. For backend changes, also run
`cd src-tauri && cargo build --release` to catch compile errors
before opening a PR.

### Per-OS build

Tauri uses each OS's native webview and toolchain, so a build
must run on the target OS. No cross-compile from macOS.

- **macOS** → `npm run build:desktop` → `.app` + `.dmg`
- **Windows** → `scripts\build-windows.ps1` (PowerShell, on a
  Windows host) → `.msi` + `-setup.exe`. Requires **Visual
  Studio C++ Build Tools** (MSVC) and **WebView2 Runtime**.
- **Linux** → `./scripts/build-linux.sh` (Debian/Ubuntu
  installs the WebKitGTK and GTK dev packages) → `.deb` +
  `.AppImage` + `.rpm`. `./scripts/docker-build-linux.sh` for a
  clean containerized build.

## File I/O and security

- All file access from the renderer goes through Rust commands
  in `src-tauri/src/main.rs`. No `fetch('file://...')` from
  Svelte.
- Tauri capabilities are declared in
  `src-tauri/capabilities/`. Keep the allowlist tight. Don't
  add `shell:execute` or `fs:*` without a clear reason.
- Excalidraw saves must strip the live editor runtime state.
  Only the document survives. Allowlist: `viewBackgroundColor` +
  `gridSize` + shapes.

## What we're not accepting

- Linters and formatters not already in the project. The Rust
  side has `cargo fmt` and `cargo clippy`; the frontend has no
  JS linter by design.
- A second package manager. Use npm (the lockfile is
  committed).
- Cross-platform shortcuts. Tauri uses each OS's native
  webview and can't cross-compile. If a feature works on one
  OS only, document it. Don't paper over it.

## Questions?

Open an issue. There's no Discord or Slack. Async in the
issue tracker is the expected channel.
