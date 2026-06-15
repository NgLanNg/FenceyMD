# `fenceymd` CLI install

## Vision & DoD

**Who.** Users who drive FenceyMD from a terminal or an AI agent.

**What.** A `fenceymd` command on the user's PATH, available **right away after
install**, so they can run `fenceymd …` in a shell and agent configs can use a
clean `command: "fenceymd"` instead of the deep
`/Applications/FenceyMD.app/Contents/MacOS/fenceymd` path.

**When / Where.** Installed on **first app launch** (a `.dmg` drag-install can't
run code, so first launch is the only hook), and re-installable on demand from
**Settings → AI agent control → Install CLI** or the app binary's `--install-cli`
flag. Note the bootstrap can't use the `fenceymd` command (it doesn't exist
yet) — it runs from the app's own startup code, the Settings button, or the
bundle binary by full path.

**Why.** The absolute bundle path is ugly and fragile (breaks if the app moves).
A PATH command is the standard "VS Code `code`" affordance and makes the MCP
agent story clean and stable.

**Done when.** After installing and launching the app once, `which fenceymd`
resolves and `fenceymd --mcp-bridge` works; agent registrations use `fenceymd`;
moving the app and relaunching re-points the symlink.

## How we implemented it

- **`src-tauri/src/cli.rs`.** `install_into(dirs, exe)` symlinks `fenceymd` →
  the real app binary in the first writable directory of a best-first list:
  `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `~/bin`. Homebrew's
  dirs are reliably on PATH and user-writable (no sudo). Rules:
  - **Never clobber** a non-symlink named `fenceymd` (could be the user's own).
  - A symlink already pointing at us → no-op; a **stale** symlink → replaced.
  - Only `mkdir` candidate dirs **under `$HOME`** (never a system bin dir).
  - `install_cli(exe)` supplies the real candidate list; `install_into` is the
    filesystem-only, unit-tested core (6 tests).
- **Auto-install on launch** (`main.rs` setup): release builds only, and never
  from a `/target/` path, so `cargo tauri dev` can't symlink its debug binary
  over a real install. Best-effort, logged to the activity log.
- **`--install-cli`** flag on the app binary (an early branch in `main()`, like
  `--mcp-bridge`): explicit (re)install/repair + the headless way to verify.
  Bootstrap it by full path —
  `/Applications/FenceyMD.app/Contents/MacOS/fenceymd --install-cli` — because
  `fenceymd` isn't on PATH until this (or first launch / the Settings button)
  has run. Once installed, `fenceymd --install-cli` re-points the symlink.
- **Tauri commands** `cli_install` / `cli_status` back the Settings UI.
- **Agent registration** (`agents.rs`) calls `cli::preferred_command(exe)`:
  `fenceymd` when the CLI is installed and current, else the absolute path.
  `refresh_registrations` upgrades older absolute-path entries to `fenceymd`
  once the CLI exists.

**Gotchas.**
- A Finder-launched GUI app has a minimal PATH, so we target well-known dirs by
  writability rather than trusting the process PATH.
- If no candidate dir is writable (locked-down Mac with no Homebrew), the CLI
  isn't installed; the Settings status shows that and the absolute-path
  registration still works.
- The symlink points at the real binary; `std::env::current_exe()` resolves it
  back, so `fenceymd --mcp-bridge` runs the bridge correctly.
