//! `fenceymd` CLI install — put the binary on the user's PATH so they can run
//! `fenceymd …` from a terminal and agent configs can use a stable
//! `command: "fenceymd"` instead of the fragile absolute bundle path
//! (`…/FenceyMD.app/Contents/MacOS/fenceymd`).
//!
//! ## Why first-launch, not install-time
//! A macOS `.dmg` drag-to-Applications install can't run code, so there's no
//! install hook. The app installs the CLI on **first launch** (release builds
//! only — a dev build must never symlink its `target/debug` binary over a real
//! install). A Settings action can (re)install or show status.
//!
//! ## What "install" means
//! We create a symlink named `fenceymd` in the first writable on-PATH bin dir
//! (`/opt/homebrew/bin` or `/usr/local/bin`). The symlink points at the real app binary; running it as
//! `fenceymd --mcp-bridge` resolves back to the app binary's bridge subcommand.
//! If the app later moves, [`install_cli`] re-points the symlink on next launch.
//!
//! ## Invariants
//! - **Never clobber a non-symlink** named `fenceymd` (could be the user's own).
//! - **Release-only auto-install** (`cfg!(debug_assertions)` / `/target/` guard
//!   lives at the call site in `main.rs`).
//! - The merge/select core ([`install_into`]) is filesystem-only and takes an
//!   explicit candidate list, so it's unit-tested against a temp dir.

use std::path::{Path, PathBuf};

/// The command name we install on PATH.
pub const CLI_NAME: &str = "fenceymd";

/// The bin directories we install into. Both are on macOS's PATH for their
/// Homebrew layout — `path_helper` keeps `/usr/local/bin` on the default PATH,
/// and Homebrew adds `/opt/homebrew/bin`. We deliberately do NOT fall back to
/// `~/.local/bin` / `~/bin`: those aren't on the default macOS PATH, so
/// installing there would leave `fenceymd` present-but-not-found — worse than
/// a clear "not installed". If neither is writable (a Mac with no Homebrew),
/// nothing is installed and Settings reports it rather than installing invisibly.
fn candidate_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin"), // Apple-Silicon Homebrew (on PATH)
        PathBuf::from("/usr/local/bin"),    // Intel Homebrew / default macOS PATH
    ]
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
#[cfg(not(unix))]
fn make_symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "CLI symlink install is only implemented for Unix",
    ))
}

/// Install `fenceymd` → `exe` into the first writable directory in `dirs`.
/// Returns the path of the created/verified symlink. Skips a directory if a
/// *non-symlink* `fenceymd` already lives there (don't clobber a real file).
/// If a symlink already points at `exe`, it's a no-op success; a stale symlink
/// is replaced. This is the testable core — `install_cli` supplies the real
/// candidate list.
///
/// Refuses to create a self-referential symlink (where the target equals the
/// link path). This happens when `current_exe()` is the symlink itself rather
/// than the real .app binary — e.g. the user launched the app via the symlink
/// we previously created, the dock stored a relative path, or some Apple-event
/// launch pathway resolved through the symlink. Without this guard we'd
/// `symlink(a, a)` and brick the CLI: `which fenceymd` finds nothing, and any
/// call returns "too many levels of symbolic links". The recovery: remove the
/// broken symlink and relaunch the .app directly (or run `--install-cli`
/// again from the real binary).
pub fn install_into(dirs: &[PathBuf], exe: &Path) -> Result<PathBuf, String> {
    let mut last_err = String::from("no candidate bin directory was writable");
    // Canonicalize `exe` so a symlink-relative `current_exe()` resolves to the
    // real binary path. Without this, an `exe` like `/opt/homebrew/bin/fenceymd`
    // would compare equal to a candidate `link` of the same path, and the
    // no-op-success branch at line ~91 below would silently return — leaving
    // a broken symlink in place. `fs::canonicalize` follows symlinks and
    // returns the resolved real path; if it fails (e.g. exe vanished), fall
    // back to the literal path so the next check can still reject the loop.
    let exe_canon = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    for dir in dirs {
        if !dir.is_dir() {
            continue; // only install into an existing on-PATH bin dir; never mkdir one
        }
        let link = dir.join(CLI_NAME);
        let existing = std::fs::symlink_metadata(&link).ok();
        let already_ours = matches!(
            &existing,
            Some(m) if m.file_type().is_symlink()
                && std::fs::read_link(&link).ok().as_deref() == Some(exe)
        );
        if already_ours {
            return Ok(link); // already correct — the idempotent re-install path
        }
        // Self-reference guard (CREATE-time only): if the link would resolve
        // to the same file as the exe (after canonicalization), don't try to
        // create a symlink at that path. This reproduces-and-fixes the v1.0 →
        // v1.1 bug where `current_exe()` was the symlink itself, so
        // `make_symlink(link, link)` bricked the CLI. Doesn't fire on the
        // already-ours branch above (which is the legitimate idempotent case).
        if std::fs::canonicalize(&link).unwrap_or_else(|_| link.clone()) == exe_canon {
            last_err = format!(
                "refusing to symlink exe to itself ({})",
                link.display()
            );
            continue;
        }
        match existing {
            Some(meta) => {
                if meta.file_type().is_symlink() {
                    // Stale symlink: not pointing at us (the already-ours case
                    // was handled above). Replace it.
                    let _ = std::fs::remove_file(&link);
                } else {
                    // A real file/dir named `fenceymd` — never clobber it.
                    last_err = format!("{} exists and is not our symlink", link.display());
                    continue;
                }
            }
            None => { /* nothing there — create below */ }
        }
        match make_symlink(exe, &link) {
            Ok(()) => return Ok(link),
            Err(e) => {
                last_err = format!("{}: {e}", dir.display());
                continue;
            }
        }
    }
    Err(last_err)
}

/// Install the CLI into the first writable well-known dir. `exe` should be the
/// resolved real app binary (`std::env::current_exe()`).
pub fn install_cli(exe: &Path) -> Result<PathBuf, String> {
    install_into(&candidate_dirs(), exe)
}

/// Reported to the Settings UI.
#[derive(serde::Serialize, Default)]
pub struct CliStatus {
    /// A `fenceymd` symlink exists in a candidate dir.
    pub installed: bool,
    /// Where it is (if installed).
    pub path: Option<String>,
    /// The symlink points at the current app binary (vs. a stale/other target).
    pub points_at_current: bool,
}

/// Report whether the CLI is installed and current, for `exe` = current binary.
pub fn current_cli_status(exe: &Path) -> CliStatus {
    for dir in candidate_dirs() {
        let link = dir.join(CLI_NAME);
        if let Ok(target) = std::fs::read_link(&link) {
            return CliStatus {
                installed: true,
                path: Some(link.display().to_string()),
                points_at_current: target.as_path() == exe,
            };
        }
    }
    CliStatus::default()
}

/// The command an agent config should use to launch the bridge: the bare
/// `fenceymd` when the CLI is installed and current (clean + the user asked for
/// it), otherwise the absolute binary path (always works).
pub fn preferred_command(exe: &Path) -> String {
    let st = current_cli_status(exe);
    if st.installed && st.points_at_current {
        CLI_NAME.to_string()
    } else {
        exe.to_string_lossy().to_string()
    }
}

// ── Tauri commands (Settings → AI agent control) ────────────────────────────

/// Install (or re-point) the `fenceymd` CLI symlink. Returns the path created.
#[tauri::command]
pub fn cli_install() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    install_cli(&exe).map(|p| p.display().to_string())
}

/// Report CLI install status for the Settings panel.
#[tauri::command]
pub fn cli_status() -> CliStatus {
    match std::env::current_exe() {
        Ok(exe) => current_cli_status(&exe),
        Err(_) => CliStatus::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "fenceymd_cli_test_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn installs_symlink_into_first_writable_dir() {
        let dir = tmp("install");
        let exe = dir.join("realbin");
        std::fs::write(&exe, b"x").unwrap();
        // Candidate must already exist (we only `mkdir` dirs under $HOME, never
        // a system bin dir — so a non-home candidate is used as-is or skipped).
        let bindir = dir.join("bin");
        std::fs::create_dir_all(&bindir).unwrap();
        let link = install_into(&[bindir.clone()], &exe).unwrap();
        assert_eq!(link, bindir.join("fenceymd"));
        assert_eq!(std::fs::read_link(&link).unwrap(), exe);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn idempotent_when_already_pointing_at_us() {
        let dir = tmp("idem");
        let exe = dir.join("realbin");
        std::fs::write(&exe, b"x").unwrap();
        let a = install_into(&[dir.clone()], &exe).unwrap();
        let b = install_into(&[dir.clone()], &exe).unwrap();
        assert_eq!(a, b);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn replaces_a_stale_symlink() {
        let dir = tmp("stale");
        let old = dir.join("old");
        let new = dir.join("new");
        std::fs::write(&old, b"o").unwrap();
        std::fs::write(&new, b"n").unwrap();
        std::os::unix::fs::symlink(&old, dir.join(CLI_NAME)).unwrap();
        let link = install_into(&[dir.clone()], &new).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), new);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn never_clobbers_a_real_file() {
        let dir = tmp("realfile");
        let exe = dir.join("realbin");
        std::fs::write(&exe, b"x").unwrap();
        // A real (non-symlink) `fenceymd` already present.
        std::fs::write(dir.join(CLI_NAME), b"important").unwrap();
        let err = install_into(&[dir.clone()], &exe).unwrap_err();
        assert!(err.contains("not our symlink"), "got: {err}");
        // The real file is untouched.
        assert_eq!(std::fs::read(dir.join(CLI_NAME)).unwrap(), b"important");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_self_referential_symlink() {
        // Reproduces the v1.0 → v1.1 bug: when `current_exe()` is the symlink
        // itself (e.g. user launched the app via `fenceymd` from a terminal),
        // `install_into` would `symlink(link, link)` and brick the CLI.
        // The fix: detect the self-reference and return an error rather than
        // creating a circular symlink.
        let dir = tmp("selfref");
        let exe = dir.join(CLI_NAME);
        std::fs::write(&exe, b"x").unwrap();
        // No `bin/` subdir — the candidate IS the dir itself, so the
        // candidate's `link` resolves to the same path as `exe`.
        let err = install_into(&[dir.clone()], &exe).unwrap_err();
        assert!(
            err.contains("refusing to symlink exe to itself"),
            "expected self-reference error, got: {err}"
        );
        // No circular symlink created.
        let link = dir.join(CLI_NAME);
        assert!(!std::fs::symlink_metadata(&link)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn falls_through_to_next_candidate_when_first_unwritable() {
        let dir = tmp("fallthrough");
        let exe = dir.join("realbin");
        std::fs::write(&exe, b"x").unwrap();
        let good = dir.join("good");
        std::fs::create_dir_all(&good).unwrap();
        // First candidate doesn't exist and isn't user-creatable (a bogus
        // absolute path under root); second is writable.
        let bogus = PathBuf::from("/nonexistent-system-dir-xyz/bin");
        let link = install_into(&[bogus, good.clone()], &exe).unwrap();
        assert_eq!(link, good.join("fenceymd"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn preferred_command_falls_back_to_abs_path_when_not_installed() {
        // A path that definitely has no `fenceymd` symlink pointing at it in
        // any real candidate dir → expect the absolute path back.
        let exe = PathBuf::from("/some/where/FenceyMD.app/Contents/MacOS/fenceymd");
        assert_eq!(preferred_command(&exe), exe.to_string_lossy());
    }
}
