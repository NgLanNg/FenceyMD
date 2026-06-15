//! FenceyMD — Tauri backend.
//!
//! Single responsibility: the privileged, filesystem-and-OS-facing half of the
//! desktop Markdown book reader. Everything that the sandboxed WKWebView frontend
//! cannot do itself lives here and is reached over Tauri IPC via `#[tauri::command]`
//! handlers (registered in `main`):
//!   - scanning a chosen folder for `.md` files and streaming their contents up;
//!   - persisting lightweight state (last folder, recents, per-file reading
//!     progress) to `app_data_dir/state.json`;
//!   - writing edited markdown / pasted images back to disk;
//!   - exporting a chapter to PDF by driving the system's headless Chrome;
//!   - native clipboard / save-dialog / external-editor / window-snapshot bridges
//!     the webview has no API for;
//!   - watching the open folder and emitting `library-changed` on edits;
//!   - a file-based debug log the user can inspect when the hidden webview console
//!     is unavailable.
//!
//! Collaborators: the frontend (`src/lib/*.js`, Svelte components) is the only
//! caller; `src/lib/renderers/manifest.json` is the shared source of truth for
//! fence-language rendering and is embedded here at compile time so the PDF path
//! and the JS registry can never drift.
//!
//! Trust model / key invariants a maintainer MUST keep in mind:
//!   - `folder` / `rel_path` pairs arriving from the frontend are UNTRUSTED. Every
//!     write path (`write_file`, `save_clipboard_image`, `update_excalidraw_block`,
//!     `rename_file`) MUST canonicalize and bounds-check against the chosen root
//!     and refuse symlinked leaves — see the per-fn notes. Do not add a write that
//!     skips this.
//!   - `chapter_html` for the PDF path is sanitized upstream by `renderMarkdown`,
//!     but the chapter `title` is NOT — it is HTML-escaped before it reaches
//!     headless Chrome (see `build_print_html` in `pdf.rs`).
//!   - Printed/exported PDFs are ALWAYS light-themed regardless of the live theme
//!     (see `build_print_html` in `pdf.rs`); edit `build_print_html` for print
//!     styling, never the app's CSS. All PDF/print code lives in `pdf.rs`.

// Prevents a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, Debouncer};
use serde::{Deserialize, Serialize};
use tauri::async_runtime as tauri_async;

// ROADMAP integration: local MCP server (AI agent control surface).
mod mcp;
// One-click registration of FenceyMD into each AI agent's MCP config.
mod agents;
// `fenceymd` CLI install (symlink onto PATH) so the binary is runnable as a
// command and agent configs can use `command: "fenceymd"`.
mod cli;
// PDF export via headless Chrome (build_print_html + the print_pdf command).
mod pdf;
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

// ── Data returned to the frontend ───────────────────────────────────────────

/// One markdown file discovered by a folder scan, with its body inlined so the
/// frontend can build its index in a single IPC round-trip.
#[derive(Serialize, Deserialize, Clone)]
pub struct MdFile {
    /// Relative path under the chosen root, using '/' separators.
    path: String,
    name: String,
    content: String,
}

/// Result of scanning a folder: its display name, the absolute root (the key
/// everything else is stored under), and every `.md` file found.
#[derive(Serialize, Clone)]
pub struct ScanResult {
    folder_name: String,
    /// Absolute path of the scanned root (used as the persistence key).
    pub root: String,
    pub files: Vec<MdFile>,
}

/// A recents-list entry. `exists` is computed at read time so the UI can grey
/// out folders that have since been moved/deleted without dropping them.
#[derive(Serialize)]
struct RecentEntry {
    path: String,
    name: String,
    exists: bool,
}

// ── Persisted state (app_data_dir/state.json) ────────────────────────────────

/// Per-file reading state. `scroll` is a 0..1 fraction (resolution-independent).
/// Every field is `#[serde(default)]` so older state.json files load forward.
#[derive(Serialize, Deserialize, Default, Clone)]
struct FileProgress {
    #[serde(default)]
    scroll: f64,
    #[serde(default)]
    bookmarked: bool,
}

/// The whole persisted document (`app_data_dir/state.json`). Read/written
/// wholesale on each mutation — it is tiny, so we accept the read-modify-write
/// over the complexity of incremental persistence. All fields default so a
/// missing/partial file deserializes cleanly rather than wiping state.
#[derive(Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    last_folder: Option<String>,
    #[serde(default)]
    recents: Vec<String>,
    /// folder root -> (relative file path -> progress)
    #[serde(default)]
    progress: HashMap<String, HashMap<String, FileProgress>>,
}

/// Maximum number of folders kept in the recents list (oldest evicted).
const RECENTS_CAP: usize = 12;

/// Absolute path of the state file. Falls back to the OS temp dir if the app
/// data dir can't be resolved, and best-effort creates the parent so callers
/// can write unconditionally.
fn store_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&dir);
    dir.join("state.json")
}

/// Load the persisted store. A missing, unreadable, or malformed file yields a
/// default `Store` rather than an error — losing state is preferable to a hard
/// failure, and the next write heals the file.
fn read_store(app: &AppHandle) -> Store {
    let path = store_path(app);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the store. Best-effort: serialization or write failure is swallowed
/// so a transient disk error never breaks the foreground operation that
/// triggered the save.
fn write_store(app: &AppHandle, store: &Store) {
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(store_path(app), json);
    }
}

/// One-time migration: copy the user's state from the pre-rebrand
/// `com.mdreader.app` data dir into the new `com.fenceymd.app` data dir.
///
/// Runs at app setup. Idempotent: if the new state already has data
/// (recents or progress), the old data is *merged in* (recents deduped,
/// progress merged per-chapter) rather than replaced. After a successful
/// merge the old state file is renamed to `state.json.migrated` so we
/// never re-run. Best-effort: any failure is silently ignored — a missing
/// or unreadable old dir just means there's no prior state to migrate.
///
/// Why: the rebrand moved the bundle id, so the OS now writes app data
/// under a new directory. Users who already had MD Reader installed lose
/// their recents + reading progress unless we copy it. This is the
/// canonical place to do that copy.
fn migrate_old_state(app: &AppHandle) {
    let new_path = store_path(app);
    let new_dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return,
    };
    let new_dir_str = new_dir.to_string_lossy().to_string();
    // Don't run if the new data dir isn't the FenceyMD dir (defensive — a
    // different bundle id like a test build shouldn't trigger the migration).
    if !new_dir_str.contains("com.fenceymd.app") {
        return;
    }

    // Locate the old MD Reader data dir. On macOS the convention is the
    // bundle id reversed-DNS; on Linux it's $XDG_DATA_HOME/<id>; on
    // Windows it's %APPDATA%\<id>. For the migration we only need
    // *some* known location — we try the macOS path first (most users
    // are on macOS), then fall back to a `$HOME` heuristic.
    let home = match std::env::var_os("HOME").map(PathBuf::from) {
        Some(h) => h,
        None => return,
    };
    let old_state = home
        .join("Library/Application Support/com.mdreader.app/state.json");

    if !old_state.exists() {
        return; // No prior install to migrate from.
    }
    let old_json = match std::fs::read_to_string(&old_state) {
        Ok(s) => s,
        Err(_) => return,
    };
    let old_store: Store = match serde_json::from_str(&old_json) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Load whatever the new state has now (defaults if missing/malformed).
    let mut new_store: Store = std::fs::read_to_string(&new_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Merge: prefer old values, but never lose new ones.
    if old_store.last_folder.is_some() {
        new_store.last_folder = old_store.last_folder.clone();
    }
    // Recents: union, dedupe, old-first order.
    let mut seen = std::collections::HashSet::new();
    let mut recents = Vec::new();
    for r in old_store.recents.iter().chain(new_store.recents.iter()) {
        if seen.insert(r.clone()) {
            recents.push(r.clone());
        }
    }
    new_store.recents = recents;
    // Progress: per-book merge — for each book, take all old chapters
    // plus any new chapters not already present.
    for (book, old_chapters) in old_store.progress {
        let entry = new_store.progress.entry(book).or_default();
        for (ch, val) in old_chapters {
            entry.entry(ch).or_insert(val);
        }
    }
    // Prefs (if any) — currently the Rust Store has no prefs field, but we
    // accept a `prefs` key for forward compatibility (the JS side may write
    // UI prefs into the same state file in the future). No-op today.

    // Persist the merged state to the new location.
    if let Ok(json) = serde_json::to_string_pretty(&new_store) {
        let _ = std::fs::write(&new_path, json);
    }
    // Rename the old state so we don't re-migrate on next launch. (Keep
    // the file on disk for the user's reference rather than deleting.)
    let _ = std::fs::rename(&old_state, old_state.with_extension("json.migrated"));
    eprintln!("[fenceymd] migrated state from {old_state:?} → {new_path:?}");
}

/// Record `root` as the last-opened folder and move it to the front of recents,
/// de-duplicating and capping at `RECENTS_CAP`. Called from every open path.
fn record_open(app: &AppHandle, root: &str) {
    let mut store = read_store(app);
    store.last_folder = Some(root.to_string());
    store.recents.retain(|p| p != root);
    store.recents.insert(0, root.to_string());
    store.recents.truncate(RECENTS_CAP);
    write_store(app, &store);
}

// ── Debug log (file-based, for the user to inspect) ──────────────────────────
//
// When something goes wrong on the JS side (e.g. "I clicked a recent folder
// and nothing happened") the console is hidden inside the WKWebView. This log
// lives at `<app_data_dir>/debug.log` and is appended to from a JS helper
// (see src/lib/debug-log.js). Every folder-open path writes a trace here
// so we can see exactly where the chain broke.
//
// The log is append-only and rotated lazily — users can clear it from the
// Settings panel. We never read it back into the UI automatically; the
// "Open debug log folder" button in Settings hands the path to the OS.

/// Absolute path of the debug log (`<app_data_dir>/debug.log`). Same temp-dir
/// fallback + best-effort parent creation as `store_path`.
pub fn debug_log_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&dir);
    dir.join("debug.log")
}

/// Append a single line to the debug log. Best-effort — failures are
/// swallowed (we never want the log writer to break the actual operation).
/// `line` should already be a single line; newlines are escaped here.
#[tauri::command]
fn debug_log(app: AppHandle, line: String) {
    let path = debug_log_path(&app);
    // ISO-8601-ish local timestamp, second precision. Cheap to format
    // and easy to grep. SystemTime / UNIX_EPOID gives us UTC seconds.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let safe = line.replace('\n', "\\n").replace('\r', "\\r");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("[{ts}] {safe}\n").as_bytes()));
}

/// Truncate the debug log (called from Settings → "Clear debug log").
#[tauri::command]
fn debug_log_clear(app: AppHandle) -> Result<(), String> {
    let path = debug_log_path(&app);
    std::fs::write(&path, b"").map_err(|e| e.to_string())
}

/// Return the absolute path of the debug log. Used by the Settings panel
/// to label the "Reveal in Finder" button.
#[tauri::command]
fn debug_log_path_str(app: AppHandle) -> String {
    debug_log_path(&app).to_string_lossy().to_string()
}

/// Reveal the debug log in the OS file browser (Finder / Explorer / xdg-open).
/// Falls back to opening the parent dir if the file doesn't exist yet.
#[tauri::command]
fn debug_log_reveal(app: AppHandle) -> Result<(), String> {
    let path = debug_log_path(&app);
    let target = if path.exists() { path.clone() } else { path.parent().unwrap_or(&path).to_path_buf() };
    let target_str = target.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        // `open -R` reveals the file in Finder; `open <dir>` opens the dir.
        let mut cmd = Command::new("open");
        if path.exists() {
            cmd.arg("-R").arg(&target_str);
        } else {
            cmd.arg(&target_str);
        }
        let _ = cmd.spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("explorer").arg(&target_str).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(&target_str).spawn();
    }
    // Suppress unused-variable warnings on platforms where `app` isn't read.
    let _ = app;
    Ok(())
}

// ── Folder scanning ──────────────────────────────────────────────────────────

/// Recursively scan `root` for `.md` files, returning their relative paths +
/// contents. Hidden files/dirs (any path segment starting with '.') are skipped.
/// Whether the folder scan should descend into / keep this entry. Prunes
/// dependency, build, and VCS/hidden directories: they never hold the user's
/// content but can hold tens of thousands of files (a single `node_modules`
/// dwarfs a real book and ships its own `README.md`s), which made opening a
/// project folder or monorepo crawl — and an agent's auto-resolve folder-switch
/// appear to hang. The root itself (depth 0) is never pruned; files pass
/// through and are filtered by the `.md`/hidden/size checks in `scan_folder`.
fn scan_should_descend(e: &walkdir::DirEntry) -> bool {
    if e.depth() == 0 {
        return true;
    }
    if e.file_type().is_dir() {
        let name = e.file_name().to_string_lossy();
        !(name.starts_with('.')
            || matches!(name.as_ref(), "node_modules" | "target" | "dist" | "build"))
    } else {
        true
    }
}

pub fn scan_folder(root: &Path) -> ScanResult {
    let folder_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Selected Folder".into());

    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(scan_should_descend)
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let name = match p.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if !name.ends_with(".md") {
            continue;
        }
        let rel = match p.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.split('/').any(|seg| seg.starts_with('.')) {
            continue;
        }
        // Read the body, but: (1) cap per-file size so one pathological file
        // can't blow up memory on every (re)scan — the watcher re-scans on each
        // change; (2) log read failures instead of silently turning them into
        // empty content (which masked permission/encoding problems).
        const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB — generous for markdown
        let content = match std::fs::metadata(p) {
            Ok(m) if m.len() > MAX_FILE_BYTES => {
                eprintln!(
                    "[fenceymd] scan: skipping oversized file ({} bytes): {}",
                    m.len(),
                    p.display()
                );
                String::new()
            }
            _ => match std::fs::read_to_string(p) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[fenceymd] scan: failed to read {}: {}", p.display(), e);
                    String::new()
                }
            },
        };
        files.push(MdFile {
            path: rel_str,
            name,
            content,
        });
    }

    // Stable order so the JS index build is deterministic.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    ScanResult {
        folder_name,
        root: root.to_string_lossy().to_string(),
        files,
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Opens a native folder picker, scans the chosen directory, and records it as
/// the last-opened folder + most recent.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<ScanResult> {
    let dir = rfd::AsyncFileDialog::new().pick_folder().await?;
    log_from_rust(&app, &format!("[rust] pick_folder: scanning {}", dir.path().display()));
    let scan_start = std::time::Instant::now();
    let result = scan_folder(dir.path());
    let total_bytes: usize = result.files.iter().map(|f| f.content.len()).sum();
    log_from_rust(
        &app,
        &format!(
            "[rust] pick_folder: files={} bytes={} elapsed_ms={}",
            result.files.len(),
            total_bytes,
            scan_start.elapsed().as_millis()
        ),
    );
    record_open(&app, &result.root);
    Some(result)
}

/// Open a folder by absolute path (recents click / reopen). Returns None if the
/// path no longer exists. Records it as last-opened + most recent.
#[tauri::command]
fn open_folder_path(app: AppHandle, path: String) -> Option<ScanResult> {
    let p = Path::new(&path);
    if !p.is_dir() {
        eprintln!("[fenceymd] open_folder_path: path is not a dir: {path}");
        log_from_rust(&app, &format!("[rust] open_folder_path: not a dir: {path}"));
        return None;
    }
    let scan_start = std::time::Instant::now();
    let result = scan_folder(p);
    let elapsed = scan_start.elapsed();
    let total_bytes: usize = result.files.iter().map(|f| f.content.len()).sum();
    eprintln!(
        "[fenceymd] open_folder_path: scanned {} files, {} bytes, {:?}",
        result.files.len(),
        total_bytes,
        elapsed
    );
    log_from_rust(
        &app,
        &format!(
            "[rust] open_folder_path: path={path} files={} bytes={} elapsed_ms={}",
            result.files.len(),
            total_bytes,
            elapsed.as_millis()
        ),
    );
    record_open(&app, &result.root);
    Some(result)
}

/// Reopen the most recently opened folder, if it still exists.
#[tauri::command]
fn open_last(app: AppHandle) -> Option<ScanResult> {
    let last = read_store(&app).last_folder?;
    open_folder_path(app, last)
}

/// List recent folders (most recent first) with an existence flag.
#[tauri::command]
fn get_recents(app: AppHandle) -> Vec<RecentEntry> {
    read_store(&app)
        .recents
        .into_iter()
        .map(|path| {
            let p = Path::new(&path);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            RecentEntry {
                exists: p.is_dir(),
                name,
                path,
            }
        })
        .collect()
}

/// Remove a folder from the recents list.
#[tauri::command]
fn remove_recent(app: AppHandle, path: String) {
    let mut store = read_store(&app);
    store.recents.retain(|p| p != &path);
    if store.last_folder.as_deref() == Some(&path) {
        store.last_folder = None;
    }
    write_store(&app, &store);
}

/// Per-file reading progress for a folder: relative path -> {scroll, bookmarked}.
#[tauri::command]
fn get_progress(app: AppHandle, folder: String) -> HashMap<String, FileProgress> {
    read_store(&app).progress.remove(&folder).unwrap_or_default()
}

/// Read the persisted recents list. Used by the MCP `open_file`
/// resolver to find a folder that contains an absolute path the
/// agent asked to open. Most-recent first.
///
/// We only return paths that still exist on disk. Stale entries are
/// filtered — they're useless to the resolver and the agent would
/// just see a "folder gone" error otherwise.
pub fn mcp_recents(app: &AppHandle) -> Vec<String> {
    read_store(app)
        .recents
        .into_iter()
        .filter(|p| std::path::Path::new(p).is_dir())
        .collect()
}

/// Save reading progress (scroll fraction + bookmark) for a file in a folder.
#[tauri::command]
fn save_progress(app: AppHandle, folder: String, path: String, scroll: f64, bookmarked: bool) {
    let mut store = read_store(&app);
    let folder_map = store.progress.entry(folder).or_default();
    if scroll <= 0.0 && !bookmarked {
        folder_map.remove(&path);
    } else {
        folder_map.insert(path, FileProgress { scroll, bookmarked });
    }
    write_store(&app, &store);
}

/// Write edited markdown back to disk. `rel_path` is relative to `folder`;
/// path traversal outside the folder is rejected.
#[tauri::command]
fn write_file(folder: String, rel_path: String, content: String) -> Result<(), String> {
    let root = Path::new(&folder);
    let target = root.join(&rel_path);
    let root_canon = root.canonicalize().map_err(|e| e.to_string())?;

    // Reject traversal. The parent dir may not exist yet (a save into a new
    // subfolder), so — like save_clipboard_image — walk up to the nearest
    // *existing* ancestor and bounds-check its canonical path. This makes
    // "../escape.md" fail even when the immediate parent doesn't exist, and
    // lets a legitimate save into a not-yet-created subdir succeed.
    let mut probe = target.clone();
    let ancestor = loop {
        let parent = match probe.parent() {
            Some(p) => p,
            None => return Err("invalid path".into()),
        };
        if parent.as_os_str().is_empty() { return Err("invalid path".into()); }
        if parent.is_dir() { break parent.to_path_buf(); }
        match parent.file_name() {
            Some(_) => probe = parent.to_path_buf(),
            None => return Err("invalid path".into()),
        }
    };
    let ancestor_canon = ancestor.canonicalize().map_err(|e| e.to_string())?;
    if !ancestor_canon.starts_with(&root_canon) {
        return Err("refusing to write outside the selected folder".into());
    }
    // If a leaf already exists, make sure we won't write THROUGH a symlink out
    // of the sandbox. Use symlink_metadata (does NOT follow the link) rather
    // than exists()/canonicalize: exists() follows the link and returns false
    // for a *dangling* symlink, so a crafted `note.md -> /outside/x` (target
    // absent) would slip past and std::fs::write would then create the file
    // outside the root. We refuse any symlinked leaf, and bounds-check a real
    // file's canonical path.
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err("refusing to write through a symlink".into());
        }
        let target_canon = target.canonicalize().map_err(|e| e.to_string())?;
        if !target_canon.starts_with(&root_canon) {
            return Err("refusing to write outside the selected folder".into());
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, content).map_err(|e| e.to_string())
}

/// Find the Nth ```` ```excalidraw ```` fenced code block in `content` and
/// return its body range plus the leading whitespace of the first body line.
/// Returns `None` if no such block exists.
///
/// Walks by char (not byte) to safely handle non-ASCII markdown content.
/// Only counts opening fences (those with a language tag) and only counts
/// excalidraw fences. Closes fences inside nested code blocks are ignored
/// because the inner fence's body always contains "```" with leading
/// whitespace on its own line, which we skip past via the line-prefix check.
fn locate_excalidraw_block(content: &str, block_index: usize) -> Option<(usize, usize, String)> {
    let mut i = 0usize;
    let mut n_seen = 0usize;
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    while i + 3 <= chars.len() {
        let (byte_pos, _ch) = chars[i];
        let line_start = content[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_prefix = &content[line_start..byte_pos];
        if line_prefix.chars().all(|c| c == ' ' || c == '\t') && content[byte_pos..].starts_with("```") {
            let fence_end_rel = match content[byte_pos..].find('\n') {
                Some(p) => p,
                // An opening fence with no trailing newline is the file's last
                // line: it has no body and no closing fence. Bail instead of
                // computing an out-of-bounds body_start (byte_pos+len+1), which
                // would panic on the `&content[body_start..]` slice below.
                None => return None,
            };
            let fence_line = &content[byte_pos..byte_pos + fence_end_rel];
            // The info string is whatever's between the ``` and the newline,
            // trimmed. A closing fence has no info (or its info starts with
            // backticks, which we treat as "no info").
            let info = fence_line.trim_start_matches('`').trim_end_matches('\n').trim();
            if info.starts_with('`') || info.is_empty() {
                // Either a closing fence (no info) or weirdly formed. Skip
                // past 3 backticks.
                i += 3;
                continue;
            }
            let is_excalidraw = info.starts_with("excalidraw");
            // Find the closing fence: a `\n` followed by optional
            // whitespace followed by `\`\`\`` on its own line.
            let body_start = byte_pos + fence_end_rel + 1;
            let after = &content[body_start..];
            let after_bytes = after.as_bytes();
            let mut close_rel: Option<usize> = None;
            let mut search_from = 0usize;
            while let Some(nl_off) = after[search_from..].find('\n') {
                let line_off = search_from + nl_off + 1;
                if line_off >= after.len() { break; }
                // Skip leading whitespace on the line.
                let mut p = line_off;
                while p < after.len() && (after_bytes[p] == b' ' || after_bytes[p] == b'\t') {
                    p += 1;
                }
                if p < after.len() && after_bytes[p..].starts_with(b"```") {
                    close_rel = Some(line_off - 1); // position of the \n
                    break;
                }
                // Advance past this line and try again.
                if let Some(next_nl) = after[line_off..].find('\n') {
                    search_from = line_off + next_nl;
                } else {
                    break;
                }
            }
            if let Some(close_rel_val) = close_rel {
                // close_start is the position of the first ``` in the closing
                // marker (after the optional leading whitespace). Walk past
                // the optional whitespace.
                let mut cs = body_start + close_rel_val + 1;
                while cs < content.len() && (content.as_bytes()[cs] == b' ' || content.as_bytes()[cs] == b'\t') {
                    cs += 1;
                }
                let close_start = cs;
                let close_end_byte = close_start + 3;
                let close_end_char = chars.partition_point(|(p, _)| *p < close_end_byte);
                if is_excalidraw {
                    if n_seen == block_index {
                        let indent: String = content[body_start..]
                            .chars()
                            .take_while(|c| *c == ' ' || *c == '\t')
                            .collect();
                        return Some((body_start, close_start, indent));
                    }
                    n_seen += 1;
                    i = close_end_char;
                    continue;
                }
                i = close_end_char;
                continue;
            }
            // Unterminated fence — bail.
            return None;
        }
        i += 1;
    }
    None
}

/// Update the Nth ```` ```excalidraw ```` fenced code block in a markdown
/// file. `new_inner` is the JSON content that goes between the opening and
/// closing fences. The leading indent of the original JSON is preserved
/// line-by-line so we don't churn the file's whitespace. The new JSON is
/// written with the same per-line indent as the original.
///
/// `block_index` is 0-based. The function re-reads the file each call so a
/// second save (after a successful first) works even if the in-memory
/// `json` prop on the frontend is stale.
///
/// Returns the new file content.
#[tauri::command]
fn update_excalidraw_block(
    folder: String,
    rel_path: String,
    block_index: usize,
    new_inner: String,
) -> Result<String, String> {
    let root = Path::new(&folder);
    let target = root.join(&rel_path);
    let root_canon = root.canonicalize().map_err(|e| e.to_string())?;
    let parent = target.parent().ok_or("invalid path")?;
    let parent_canon = parent.canonicalize().map_err(|e| e.to_string())?;
    if !parent_canon.starts_with(&root_canon) {
        return Err("refusing to write outside the selected folder".into());
    }
    let current = std::fs::read_to_string(&target).map_err(|e| e.to_string())?;
    let (body_start, close_start, indent) = locate_excalidraw_block(&current, block_index)
        .ok_or_else(|| format!("no ```excalidraw block at index {} in {}", block_index, rel_path))?;

    // Build the new body: each line of `new_inner` gets prefixed with `indent`.
    // Strip a trailing newline from new_inner so we don't leave a blank line.
    let trimmed = new_inner.trim_end_matches('\n').to_string();
    let mut new_body = String::with_capacity(trimmed.len() + indent.len() * 8);
    let mut first = true;
    for line in trimmed.split('\n') {
        if !first { new_body.push('\n'); }
        first = false;
        new_body.push_str(&indent);
        new_body.push_str(line);
    }
    new_body.push('\n');

    let mut s = String::with_capacity(current.len() + new_body.len());
    s.push_str(&current[..body_start]);
    s.push_str(&new_body);
    s.push_str(&current[close_start..]);
    std::fs::write(&target, &s).map_err(|e| e.to_string())?;
    Ok(s)
}

/// Save exported bytes (PDF, PNG, …) via a native save dialog. The WKWebView has
/// no download manager, so an `<a download>` does nothing in the desktop app —
/// the frontend hands us the bytes and we write them where the user picks. The
/// file extension in `default_name` drives the dialog's type filter.
/// Returns the saved path, or None if the dialog was cancelled.
#[tauri::command]
async fn save_export(default_name: String, bytes: Vec<u8>) -> Result<Option<String>, String> {
    let ext = default_name
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && *e != default_name)
        .unwrap_or("bin")
        .to_lowercase();
    let file = rfd::AsyncFileDialog::new()
        .set_file_name(&default_name)
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .save_file()
        .await;
    match file {
        Some(handle) => {
            let path = handle.path().to_path_buf();
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Copy a PNG image (diagram) to the system clipboard. The WKWebView doesn't
/// support `navigator.clipboard.write()` for images, so the frontend hands us the
/// PNG bytes and we decode + place them on the native clipboard.
#[tauri::command]
fn copy_image(bytes: Vec<u8>) -> Result<(), String> {
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("decode failed: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(img.into_raw()),
    };
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(data).map_err(|e| e.to_string())
}

/// Snapshot the FenceyMD window to the system clipboard.
///
/// The user-visible affordance is "take a snapshot of what the app looks
/// like right now and put it on the clipboard so I can paste it
/// somewhere". The first cut captures the entire OS window (chrome
/// + content). A future cut can take a `(x, y, w, h)` rect for
/// region-select; the JS side is already structured around that
/// extension point — see `snapshotApp()` in src/lib/tauri.js.
///
/// Capturing your OWN app's window does not require screen-recording
/// permission on any of the supported platforms: on macOS xcap goes
/// through CGWindowListCreateImage which allows self-capture; on
/// Windows the app is just sampling its own HWND; on Linux X11
/// ignores the security extension for the focused window. We still
/// fall through to a clean error string if the platform refuses, so
/// the UI can surface a hint instead of silently failing.
#[tauri::command]
fn snapshot_app_to_clipboard(app: AppHandle) -> Result<SnapshotInfo, String> {
    log_from_rust(&app, "[rust] snapshot_app_to_clipboard: start");
    // We identify our window by app name + owning pid rather than by
    // title, because the title contains the chapter name and we want
    // a stable match. xcap's `Window::all()` exposes both.
    let pid = std::process::id();
    let windows = xcap::Window::all().map_err(|e| {
        log_from_rust(&app, &format!("[rust] snapshot: xcap::Window::all failed: {e}"));
        format!("enumerate windows failed: {e}")
    })?;
    // xcap returns Result from every accessor. We swallow inner errors
    // because the comparison short-circuits on Err() via `.unwrap_or`,
    // which is the conventional way to ask "is this still our window"
    // when the platform doesn't expose that exact field.
    let me = windows
        .into_iter()
        .find(|w| {
            let minimized = w.is_minimized().unwrap_or(true);
            if minimized {
                return false;
            }
            let wpid = w.pid().unwrap_or(0);
            if wpid == pid {
                return true;
            }
            // Fallback: match by app name. Title changes per chapter.
            w.app_name()
                .map(|n| n.to_lowercase().contains("fenceymd"))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            log_from_rust(&app, "[rust] snapshot: no matching window found");
            "FenceyMD window not found".to_string()
        })?;

    let img = me.capture_image().map_err(|e| {
        log_from_rust(&app, &format!("[rust] snapshot: capture_image failed: {e}"));
        format!("capture failed: {e}")
    })?;
    let (w, h) = (img.width(), img.height());
    let rgba = img.into_raw();
    let data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(rgba),
    };
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(data).map_err(|e| e.to_string())?;
    let info = SnapshotInfo {
        width: w,
        height: h,
        bytes: w as usize * h as usize * 4,
    };
    log_from_rust(
        &app,
        &format!(
            "[rust] snapshot: ok {}x{} ({} bytes RGBA)",
            info.width, info.height, info.bytes
        ),
    );
    Ok(info)
}

/// Returned to the JS side after a successful snapshot. Lets the UI
/// show a confirmation toast with the dimensions ("Copied 1100 × 820
/// to clipboard") without re-encoding the image.
#[derive(Serialize)]
struct SnapshotInfo {
    width: u32,
    height: u32,
    bytes: usize,
}

/// Save a pasted clipboard image as `<folder>/<rel_path>` and return the
/// absolute path actually written. `rel_path` is relative to `folder` and is
/// canonicalize-checked against the chosen folder — same traversal defense
/// as `write_file`. The directory containing the file is created if it
/// doesn't already exist (so pasting into a chapter's `images/` subdir
/// works on the first paste). Used by the editor's ⌘V handler.
#[tauri::command]
fn save_clipboard_image(folder: String, rel_path: String, bytes: Vec<u8>) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("empty image bytes".into());
    }
    let root = Path::new(&folder);
    let target = root.join(&rel_path);
    // Reject traversal: the canonicalized parent must stay inside the root.
    // The parent may not exist yet (we create it below), so we walk up
    // until we find a directory that does and use *its* canonical path
    // for the bounds check — that way refusing "../escape.png" works
    // even when no `images/` dir exists yet.
    let mut probe = target.clone();
    let ancestor = loop {
        let parent = match probe.parent() {
            Some(p) => p,
            None => return Err("invalid path".into()),
        };
        if parent.as_os_str().is_empty() { return Err("invalid path".into()); }
        if parent.is_dir() { break parent.to_path_buf(); }
        match parent.file_name() {
            Some(_) => probe = parent.to_path_buf(),
            None => return Err("invalid path".into()),
        }
    };
    let root_canon = root.canonicalize().map_err(|e| e.to_string())?;
    let ancestor_canon = ancestor.canonicalize().map_err(|e| e.to_string())?;
    if !ancestor_canon.starts_with(&root_canon) {
        return Err("refusing to write outside the selected folder".into());
    }
    // Same symlink-leaf defense as write_file: don't write through a symlinked
    // leaf (a dangling one slips past exists()/canonicalize and would escape).
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err("refusing to write through a symlink".into());
        }
        let target_canon = target.canonicalize().map_err(|e| e.to_string())?;
        if !target_canon.starts_with(&root_canon) {
            return Err("refusing to write outside the selected folder".into());
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Open `path` in the user's external editor / OS handler. Honors a
/// per-user override stored on the frontend (the editor command is
/// passed as `editor_override`; if set we run that, otherwise we fall
/// back to the OS default). On macOS we use `open -t` so the file opens
/// in TextEdit (the universal fallback) — `open` without `-t` would
/// open the user's default app for that extension, which is usually
/// fine but `-t` is the explicit "give me an editor" affordance.
//
// TRUST BOUNDARY: `editor_override` is a user-chosen setting (analogous to
// git's `core.editor`), entered in the Settings UI. We spawn it directly —
// NOT through a shell — so the args carry no shell-metacharacter meaning and
// the only thing that runs is the named binary. The setting is the user's own
// choice; we are not protecting against a user who deliberately points it at a
// harmful binary, but we do reject control characters as a defense-in-depth
// guard against a tampered/injected value (and to keep errors/logs clean).
#[tauri::command]
fn open_in_external_editor(path: String, editor_override: Option<String>) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_file() {
        return Err("file not found".into());
    }
    if let Some(editor) = editor_override {
        let trimmed = editor.trim();
        if !trimmed.is_empty() {
            // Reject control characters (newlines, NUL, etc.). A legitimate
            // editor command never contains them; their presence signals a
            // tampered setting, so we refuse rather than spawn something odd.
            if trimmed.chars().any(|c| c.is_control()) {
                return Err("invalid editor command".into());
            }
            // If the whole override is an existing file path, treat it as the
            // binary with no args — this handles editor binaries whose install
            // path contains spaces (e.g. "/Applications/Visual Studio
            // Code.app/Contents/MacOS/Electron"), which a naive whitespace
            // split would shatter into a bogus "/Applications/Visual" command.
            // Otherwise it's a "cmd [args]" form (e.g. "subl -w"): split.
            let mut cmd = if Path::new(trimmed).is_file() {
                Command::new(trimmed)
            } else {
                let mut parts = trimmed.split_whitespace();
                let bin = parts.next().unwrap_or(trimmed);
                let mut c = Command::new(bin);
                for a in parts { c.arg(a); }
                c
            };
            return cmd.arg(p).spawn().map(|_| ()).map_err(|e| format!("failed to spawn `{trimmed}`: {e}"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg("-t").arg(p).spawn()
            .map(|_| ()).map_err(|e| format!("open -t failed: {e}"))
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(p).spawn()
            .map(|_| ()).map_err(|e| format!("xdg-open failed: {e}"))
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/c", "start", "", &path]).spawn()
            .map(|_| ()).map_err(|e| format!("start failed: {e}"))
    }
}

/// Rename a markdown file within the folder. `rel_path` is relative to `folder`;
/// `new_name` is just the new file name (extension added if missing). Refuses to
/// escape the folder or overwrite an existing file. Returns the new relative path.
#[tauri::command]
fn rename_file(folder: String, rel_path: String, new_name: String) -> Result<String, String> {
    let root = Path::new(&folder);
    let src = root.join(&rel_path);
    if !src.is_file() {
        return Err("file not found".into());
    }
    // Sanitize: take just the file name, strip any path separators.
    let clean = new_name.trim().replace(['/', '\\'], "");
    if clean.is_empty() || clean.starts_with('.') {
        return Err("invalid name".into());
    }
    let clean = if clean.to_lowercase().ends_with(".md") {
        clean
    } else {
        format!("{clean}.md")
    };
    let parent = src.parent().ok_or("invalid path")?;
    let dst = parent.join(&clean);

    let root_canon = root.canonicalize().map_err(|e| e.to_string())?;
    let parent_canon = parent.canonicalize().map_err(|e| e.to_string())?;
    if !parent_canon.starts_with(&root_canon) {
        return Err("refusing to write outside the selected folder".into());
    }
    if dst.exists() {
        return Err("a file with that name already exists".into());
    }
    std::fs::rename(&src, &dst).map_err(|e| e.to_string())?;

    // Build the new relative path from rel_path's own parent (avoids any
    // canonicalize/symlink mismatch that strip_prefix on root would hit).
    let rel = match Path::new(&rel_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => {
            format!("{}/{}", p.to_string_lossy().replace('\\', "/"), clean)
        }
        _ => clean,
    };
    Ok(rel)
}

// ── Live file watching ─────────────────────────────────────────────────────────

/// Managed state holding the single active folder watcher (or `None` before the
/// first `watch_folder`). The `Debouncer` must be kept alive for events to keep
/// firing; storing it here ties its lifetime to the app and replacing it drops
/// (and stops) the previous watcher.
type WatcherState = Mutex<Option<Debouncer<notify::RecommendedWatcher>>>;

/// Start watching `path` recursively. On any debounced change, re-scan and emit
/// `library-changed` with the fresh ScanResult. Replaces any existing watcher.
#[tauri::command]
fn watch_folder(app: AppHandle, state: tauri::State<'_, WatcherState>, path: String) -> Result<(), String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err("not a directory".into());
    }

    let app_for_cb = app.clone();
    let root_for_cb = root.clone();
    let mut debouncer = new_debouncer(
        std::time::Duration::from_millis(400),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            if res.is_err() {
                return;
            }
            log_from_rust(&app_for_cb, "[rust] watcher fired, re-scanning");
            let result = scan_folder(&root_for_cb);
            let _ = app_for_cb.emit("library-changed", result);
        },
    )
    .map_err(|e| e.to_string())?;

    debouncer
        .watcher()
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    log_from_rust(&app, &format!("[rust] watch_folder: started on {}", root.display()));

    // Keep the debouncer alive by storing it in managed state (drops the old one).
    // Recover from a poisoned mutex (a prior panic while the lock was held)
    // instead of cascading into a second panic that would take down the command
    // handler — the stored value is just a watcher handle, safe to replace.
    let mut guard = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(debouncer);
    Ok(())
}

/// Scan a folder by absolute path without recording it (test helper; harmless).
#[tauri::command]
fn scan_path(path: String) -> ScanResult {
    scan_folder(Path::new(&path))
}

/// Internal helper: append a one-shot line to the debug log from Rust code
/// (e.g. the watcher, the PDF pipeline). Same format as the JS-side log.
pub fn log_from_rust(app: &AppHandle, line: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let safe = line.replace('\n', "\\n").replace('\r', "\\r");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(debug_log_path(app))
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("[{ts}] {safe}\n").as_bytes()));
}

/// `--help` text. Printed by `fenceymd --help`; kept in sync with the flags
/// handled at the top of `main()`.
const CLI_HELP: &str = "\
FenceyMD — a local Markdown reader with a read-only MCP server for AI agents.

USAGE:
    fenceymd                 Launch the app (the MCP server starts with it)
    fenceymd --mcp-bridge    Bridge stdio JSON-RPC to the running app's MCP server
    fenceymd --install-cli   (Re)install the `fenceymd` symlink onto your PATH
    fenceymd --help, -h      Show this help
    fenceymd --version, -V   Show the version

The MCP server is read-only (open/read/observe). See docs/MCP_SETUP.md.
";

/// App entry point: register the empty watcher state, wire up every IPC command
/// the frontend can invoke, and run the Tauri event loop. The
/// `generate_handler!` list is the authoritative set of callable commands — a
/// command not listed here is unreachable from the frontend.
fn main() {
    // Native MCP stdio↔HTTP bridge. When an agent spawns this binary with
    // `--mcp-bridge`, we act as a pure stdio bridge to the running app's local
    // MCP server and NEVER start the GUI. This MUST be the first statement in
    // main(): no Tauri/logging init may run first, and only JSON-RPC frames
    // may reach stdout (all bridge logging goes to stderr). See
    // `mcp::run_bridge` for the contract.
    if std::env::args().any(|a| a == "--mcp-bridge") {
        mcp::run_bridge();
        return;
    }

    // Explicit CLI (re)install/repair: `fenceymd --install-cli` symlinks the
    // binary onto PATH and exits, printing the path. The app also does this
    // automatically on first launch; this subcommand is for manual repair (e.g.
    // after moving the app) and is the headless way to verify the install.
    if std::env::args().any(|a| a == "--install-cli") {
        match std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| cli::install_cli(&exe))
        {
            Ok(p) => {
                println!("installed: {}", p.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("install failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // `--help` / `--version`: print and exit BEFORE Tauri init. Without this,
    // an unknown flag like `--help` falls through to launching the GUI app —
    // which spins up a stray instance + a port file that then goes stale
    // (exactly the mess `fenceymd --help` caused during MCP testing).
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        print!("{CLI_HELP}");
        return;
    }
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("fenceymd {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    tauri::Builder::default()
        .manage::<WatcherState>(Mutex::new(None))
        .setup(|app| {
            // Rebrand migration: copy state from the pre-rebrand
            // `com.mdreader.app` data dir into the new dir before any
            // other subsystem reads it. Idempotent; rename-on-success
            // means we never re-run.
            migrate_old_state(&app.handle());

            // ROADMAP integration: start the MCP server. Spawned on
            // Tauri's tokio runtime so the UI thread is unaffected.
            // The server binds a random localhost port and writes a
            // port file in the app-data dir (see `mcp::port_dir`) for
            // agent discovery. Failures are logged to debug.log and
            // stderr but never block app startup.
            let app_handle = app.handle().clone();
            tauri_async::spawn(async move {
                mcp::start(app_handle).await;
            });
            // Make the `fenceymd` CLI available on PATH (symlink into the first
            // writable well-known bin dir) so users can run it from a terminal
            // and agent configs use a clean `command: "fenceymd"`. First launch
            // is our only install hook — a .dmg drag-install can't run code.
            // Release-only and never from a build tree, so `cargo tauri dev`
            // can't symlink its debug binary over a real install. Best-effort.
            if !cfg!(debug_assertions) {
                if let Ok(exe) = std::env::current_exe() {
                    if !exe.to_string_lossy().contains("/target/") {
                        let h = app.handle().clone();
                        match cli::install_cli(&exe) {
                            Ok(p) => log_from_rust(&h, &format!("[cli] fenceymd -> {}", p.display())),
                            Err(e) => log_from_rust(&h, &format!("[cli] install skipped: {e}")),
                        }
                    }
                }
            }
            // Self-heal any agent registrations the user previously
            // enabled: if the app binary moved (update / drag to a new
            // path), rewrite the stored command to the current path.
            // Touches ONLY agents already registered — never opts an
            // agent in. Best-effort; logs to stderr on failure.
            agents::refresh_registrations();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            open_folder_path,
            open_last,
            get_recents,
            remove_recent,
            get_progress,
            save_progress,
            write_file,
            update_excalidraw_block,
            save_export,
            pdf::print_pdf,
            copy_image,
            save_clipboard_image,
            open_in_external_editor,
            rename_file,
            watch_folder,
            scan_path,
            debug_log,
            debug_log_clear,
            debug_log_path_str,
            debug_log_reveal,
            snapshot_app_to_clipboard,
            mcp::mcp_update_view_state,
            mcp::mcp_set_active_folder,
            mcp::mcp_clear_active_folder,
            mcp::mcp_status,
            agents::agents_detect,
            agents::agents_register,
            agents::agents_unregister,
            cli::cli_install,
            cli::cli_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // ROADMAP integration: clean up the MCP port file on
            // shutdown. We catch both ExitRequested (user clicked
            // the close button — graceful) and WindowEvent::CloseRequested
            // is fire-and-forget per Tauri 2 semantics. We don't
            // block the exit; the worst case if cleanup fails is a
            // stale `port` file that the next launch overwrites.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                mcp::cleanup_port_file(app_handle);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{build_print_html, load_renderer_manifest, transform_for_pdf};
    use std::fs;

    fn fixture() -> PathBuf {
        // Per-test unique temp dir so parallel execution doesn't race
        // on a fixed path. The old single-name fixture flaked when one
        // test removed the dir while another was reading from it.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pid = std::process::id();
        let base = std::env::temp_dir().join(format!("fenceymd_test_book_{pid}_{n}"));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("part-i")).unwrap();
        fs::create_dir_all(base.join(".hidden")).unwrap();
        fs::write(base.join("intro.md"), "# Intro\nhello").unwrap();
        fs::write(base.join("part-i").join("ch1.md"), "# Ch1\nbody").unwrap();
        fs::write(base.join("notes.txt"), "ignored").unwrap();
        fs::write(base.join(".hidden").join("secret.md"), "nope").unwrap();
        base
    }

    #[test]
    fn scans_md_skips_txt_and_hidden() {
        let base = fixture();
        let r = scan_folder(&base);
        // Folder name embeds the pid + counter (see fixture()) — assert
        // it matches the path's leaf rather than a literal string.
        assert!(r.folder_name.starts_with("fenceymd_test_book"));
        assert_eq!(
            r.folder_name,
            base.file_name().unwrap().to_string_lossy().to_string()
        );
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["intro.md", "part-i/ch1.md"]);
        let ch1 = r.files.iter().find(|f| f.path == "part-i/ch1.md").unwrap();
        assert_eq!(ch1.content, "# Ch1\nbody");
        assert_eq!(ch1.name, "ch1.md");
        assert!(r.root.ends_with(&r.folder_name));
    }

    #[test]
    fn scan_prunes_node_modules_and_build_dirs() {
        // Opening a project folder/monorepo must not walk dependency or build
        // trees (a single node_modules can hold tens of thousands of files and
        // its own README.md). Only the real top-level chapter should survive.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("fenceymd_prune_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&base);
        for d in ["node_modules/pkg", "target/debug", "dist", "build", ".git"] {
            fs::create_dir_all(base.join(d)).unwrap();
            fs::write(base.join(d).join("README.md"), "# should be pruned").unwrap();
        }
        fs::write(base.join("real.md"), "# real").unwrap();
        let r = scan_folder(&base);
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["real.md"], "only the top-level chapter should survive pruning");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_file_rejects_traversal() {
        let base = fixture();
        let folder = base.to_string_lossy().to_string();
        // Legit write inside the folder succeeds.
        assert!(write_file(folder.clone(), "intro.md".into(), "# Edited\nx".into()).is_ok());
        assert_eq!(fs::read_to_string(base.join("intro.md")).unwrap(), "# Edited\nx");
        // Traversal outside the folder is rejected.
        assert!(write_file(folder, "../escape.md".into(), "nope".into()).is_err());
    }

    #[test]
    fn write_file_creates_missing_parent_dirs() {
        // A save into a not-yet-existing subfolder must succeed (write_file used
        // to require the parent dir to already exist) while still bounds-checked.
        let base = fixture();
        let folder = base.to_string_lossy().to_string();
        assert!(write_file(folder.clone(), "notes/sub/new.md".into(), "hello".into()).is_ok());
        assert_eq!(fs::read_to_string(base.join("notes/sub/new.md")).unwrap(), "hello");
        // Traversal through a new subdir is still rejected.
        assert!(write_file(folder, "notes/../../escape.md".into(), "nope".into()).is_err());
    }

    // Exercises the real Tauri IPC dispatch: registers commands via
    // generate_handler and invokes one through the mock runtime.
    #[test]
    fn ipc_dispatch_invokes_registered_command() {
        let base = fixture();
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![scan_path])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let res = tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "scan_path".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                    "path": base.to_string_lossy()
                })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        );
        let json = res
            .expect("ipc command should succeed")
            .deserialize::<serde_json::Value>()
            .unwrap();
        // Folder name is per-test unique now — confirm it parses and starts
        // with the canonical prefix.
        let folder_name = json["folder_name"].as_str().unwrap();
        assert!(folder_name.starts_with("fenceymd_test_book"));
        assert_eq!(json["files"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn locate_excalidraw_block_finds_simple_block() {
        let content = "\
# Title

Some prose.

```excalidraw
{\"elements\":[]}
```

More prose.
";
        let (body_start, close_start, indent) = locate_excalidraw_block(content, 0).unwrap();
        let body = &content[body_start..close_start];
        assert!(body.contains("\"elements\""));
        assert!(body.contains("[]"));
        assert_eq!(indent, "");
    }

    #[test]
    fn locate_excalidraw_block_handles_crlf() {
        // Windows-authored files use \r\n. The locator must still find the
        // block (this path rewrites the user's file, so a wrong offset here
        // corrupts content).
        let content = "# Title\r\n\r\n```excalidraw\r\n{\"elements\":[1]}\r\n```\r\n\r\nafter\r\n";
        let (body_start, close_start, _indent) = locate_excalidraw_block(content, 0)
            .expect("CRLF block should be located");
        let body = &content[body_start..close_start];
        assert!(body.contains("\"elements\":[1]"), "body was {body:?}");
    }

    #[test]
    fn locate_excalidraw_block_handles_tab_indent() {
        // Tab-indented fence (e.g. inside a tab-indented list item).
        let content = "\t```excalidraw\n\t{\"elements\":[2]}\n\t```\n";
        let (body_start, close_start, indent) = locate_excalidraw_block(content, 0)
            .expect("tab-indented block should be located");
        assert_eq!(indent, "\t");
        assert!(content[body_start..close_start].contains("\"elements\":[2]"));
    }

    #[test]
    fn locate_excalidraw_block_accepts_trailing_info_string() {
        // `is_excalidraw` matches `starts_with("excalidraw")`, so an info
        // string with extra tokens must still count as an excalidraw fence.
        let content = "```excalidraw {\"clip\":true}\n{\"elements\":[3]}\n```\n";
        let (body_start, close_start, _indent) = locate_excalidraw_block(content, 0)
            .expect("trailing-info fence should be located");
        assert!(content[body_start..close_start].contains("\"elements\":[3]"));
    }

    #[test]
    fn locate_excalidraw_block_unterminated_trailing_fence_returns_none_not_panic() {
        // A file ending in an opening fence with NO trailing newline used to
        // compute body_start = len+1 and panic on `&content[body_start..]`.
        // It must return None (no complete block) instead of crashing.
        assert!(locate_excalidraw_block("# x\n\n```excalidraw", 0).is_none());
        assert!(locate_excalidraw_block("```js", 0).is_none());
    }

    #[test]
    fn locate_excalidraw_block_preserves_indent() {
        let content = "\
- item

  ```excalidraw
  {\"elements\":[]}
  ```
";
        eprintln!("content bytes: {:?}", content.as_bytes());
        eprintln!("content repr: {:?}", content);
        let r = locate_excalidraw_block(content, 0);
        eprintln!("result: {:?}", r);
        let (body_start, close_start, indent) = r.unwrap();
        assert_eq!(indent, "  ");
        // The body content itself includes the indented lines.
        let body = &content[body_start..close_start];
        assert!(body.starts_with("  {\"elements\""));
    }

    #[test]
    fn locate_excalidraw_block_skips_non_excalidraw_fences() {
        let content = "\
```js
console.log(1);
```

```excalidraw
{\"a\":1}
```

```mermaid
graph LR
A --> B
```
";
        // Block 0 should be the excalidraw one, not the js or mermaid.
        let (body_start, close_start, _) = locate_excalidraw_block(content, 0).unwrap();
        let body = &content[body_start..close_start];
        assert!(body.contains("\"a\":1"));
    }

    #[test]
    fn locate_excalidraw_block_handles_non_ascii() {
        // The em dash and other unicode must not break the byte walk.
        let content = "\
# Café — résumé

```excalidraw
{\"elements\":[{\"id\":\"x\"}]}
```
";
        let (body_start, close_start, _) = locate_excalidraw_block(content, 0).unwrap();
        let body = &content[body_start..close_start];
        assert!(body.contains("\"x\""));
    }

    #[test]
    fn locate_excalidraw_block_counts_multiple_correctly() {
        let content = "\
```excalidraw
{\"a\":1}
```

```mermaid
graph LR
A --> B
```

```excalidraw
{\"b\":2}
```
";
        // Block 0 = first excalidraw, block 1 = second excalidraw
        // (mermaid is skipped).
        let (b0, e0, _) = locate_excalidraw_block(content, 0).unwrap();
        assert!(content[b0..e0].contains("\"a\":1"));
        let (b1, e1, _) = locate_excalidraw_block(content, 1).unwrap();
        assert!(content[b1..e1].contains("\"b\":2"));
        // Block 2 = None
        assert!(locate_excalidraw_block(content, 2).is_none());
    }

    #[test]
    fn build_print_html_embeds_katex_css() {
        // The PDF pipeline must inline katex's stylesheet so rendered math
        // (katex emits <span class="katex">) is legible. node_modules is
        // present in the working tree during `cargo test`, so the read
        // should succeed.
        let mut vars = std::collections::HashMap::new();
        vars.insert("--ink".to_string(), "#1a1c1c".to_string());
        vars.insert("--surface".to_string(), "#ffffff".to_string());
        vars.insert("--surface-variant".to_string(), "#e3e2e1".to_string());
        let html = build_print_html("Math", "<p>before</p><span class=\"katex\">x</span><p>after</p>", &vars);
        // katex's stylesheet has the literal selector `.katex` — assert it
        // is present in the embedded <style> block, not the chapter body.
        let style_start = html.find("<style>").expect("<style> tag present");
        let style_end = html.find("</style>").expect("</style> close");
        let style_block = &html[style_start..style_end];
        assert!(style_block.contains(".katex"), "katex CSS rule missing from <style>: {}", &style_block[..style_block.len().min(200)]);
        // Shiki block rules should also be present so highlighted code looks right.
        assert!(style_block.contains(".shiki-block"), "shiki block CSS rule missing");
    }

    #[test]
    fn build_print_html_forces_light_palette_even_from_dark_theme() {
        // Regression: exporting a PDF while the app is in DARK mode used to
        // inject dark colors into :root, printing a dark rectangle floating in
        // white page margins ("just a box"). A PDF must always be light. We
        // pass a dark snapshot and assert the output forces the light palette.
        let mut vars = std::collections::HashMap::new();
        vars.insert("--surface".to_string(), "#242428".to_string()); // dark
        vars.insert("--ink".to_string(), "#e6e6e6".to_string()); // light text
        vars.insert("--font-serif".to_string(), "Newsreader, serif".to_string());
        let html = build_print_html("Dark export", "<p>hello</p>", &vars);
        let style_end = html.find("</style>").expect("</style>");
        let style_block = &html[..style_end];
        // The forced-light override must be present.
        assert!(
            style_block.contains("--surface: #ffffff;"),
            "forced white --surface missing"
        );
        assert!(
            style_block.contains("--ink: #1f2222;"),
            "forced dark --ink missing"
        );
        // Body must paint pure white so there is never a visible box.
        assert!(
            style_block.contains("background: #ffffff;"),
            "white body background missing"
        );
        // The dark snapshot value may still appear once (in the first :root),
        // but the forced light value must appear AFTER it so it wins by source
        // order. Assert the forced white --surface comes after the dark one.
        let dark_pos = style_block.find("--surface: #242428;");
        let light_pos = style_block
            .find("--surface: #ffffff;")
            .expect("light surface present");
        if let Some(dp) = dark_pos {
            assert!(
                light_pos > dp,
                "forced light --surface must override the dark snapshot (appear later)"
            );
        }
        // The <html> element is pinned to light.
        assert!(html.contains("data-theme=\"light\""), "PDF html must be light");
    }

    /// Throwaway helper: dumps a real `build_print_html` output (simulating an
    /// export while the app is in DARK mode) to /tmp so it can be rendered with
    /// headless Chrome for a visual sanity check. Gated by #[ignore] so it never
    /// runs in the normal suite. Run with: `cargo test dump_print_html -- --ignored`.
    #[test]
    #[ignore]
    fn dump_print_html_for_review() {
        let mut vars = std::collections::HashMap::new();
        // DARK-mode snapshot — what exportPDF() would pass from dark mode.
        vars.insert("--surface".to_string(), "#242428".to_string());
        vars.insert("--surface-variant".to_string(), "#3a3a40".to_string());
        vars.insert("--surface-container-low".to_string(), "#2c2c30".to_string());
        vars.insert("--surface-container-lowest".to_string(), "#2a2a2e".to_string());
        vars.insert("--ink".to_string(), "#e6e6e6".to_string());
        vars.insert("--ink-secondary".to_string(), "#b0b0b0".to_string());
        vars.insert("--ink-muted".to_string(), "#888888".to_string());
        vars.insert("--tertiary".to_string(), "#e06c5a".to_string());
        vars.insert("--font-serif".to_string(), "Georgia, serif".to_string());
        vars.insert("--font-sans".to_string(), "Helvetica, sans-serif".to_string());
        let body = r#"
<h2>Introduction</h2>
<p>This paragraph exists to prove the page background is full white, edge to edge, with no dark box even though the export was triggered from dark mode.</p>
<pre class="shiki-block"><code>fn main() { println!("hello"); }</code></pre>
<div class="csv-block"><table>
<thead><tr><th>A really long header column name</th><th>Another quite wide header</th><th>Third wide header here</th><th>Fourth</th><th>Fifth column</th><th>Sixth column header</th></tr></thead>
<tbody><tr><td>some fairly long cell value</td><td>more text in this cell</td><td>and yet more content</td><td>x</td><td>data</td><td>final column value text</td></tr></tbody>
</table></div>
<blockquote>A quoted line to check the accent and contrast.</blockquote>
"#;
        let html = build_print_html("Dark Export Sanity Check", body, &vars);
        let path = std::env::temp_dir().join("fenceymd_verify.html");
        std::fs::write(&path, &html).unwrap();
        eprintln!("WROTE: {}", path.display());
    }

    #[test]
    fn build_print_html_csv_tables_wrap_not_clip() {
        // Wide CSV tables used to overflow off the page (white-space: nowrap on
        // headers). They must wrap so the table fits the printable width.
        let mut vars = std::collections::HashMap::new();
        vars.insert("--surface".to_string(), "#ffffff".to_string());
        let html = build_print_html("CSV", "<div class=\"csv-block\"><table></table></div>", &vars);
        let style_end = html.find("</style>").expect("</style>");
        let style_block = &html[..style_end];
        assert!(
            style_block.contains("word-break: break-word;"),
            "csv cells must allow wrapping"
        );
        assert!(
            !style_block.contains("white-space: nowrap;"),
            "csv headers must not be nowrap (that clips wide tables)"
        );
    }

    #[test]
    fn build_print_html_escapes_untrusted_title() {
        // The chapter title is untrusted (first heading / file name) and is
        // interpolated into <title> and <h1>. It must be HTML-escaped so a
        // crafted title can't inject markup/script into the headless-Chrome
        // render.
        let vars = std::collections::HashMap::new();
        let html = build_print_html("</title><script>alert(1)</script>", "<p>x</p>", &vars);
        assert!(
            !html.contains("<script>alert(1)</script>"),
            "raw <script> from the title leaked into the print HTML"
        );
        assert!(
            html.contains("&lt;/title&gt;&lt;script&gt;"),
            "title should be HTML-escaped"
        );
    }

    #[test]
    fn renderer_manifest_parses_and_lists_langs() {
        // The PDF pipeline must read the same manifest the JS side
        // registers. If this test breaks, either the JSON is malformed
        // or the path is wrong.
        let m = load_renderer_manifest();
        assert!(!m.is_empty(), "manifest should declare at least one renderer");
        let langs: Vec<&str> = m.iter().map(|e| e.lang.as_str()).collect();
        for required in ["svg", "html", "mermaid", "excalidraw", "csv", "math", "shiki"] {
            assert!(langs.contains(&required), "manifest missing lang: {}", required);
        }
        // shiki is the defaultFor: "code" entry; confirm it parses.
        let shiki = m.iter().find(|e| e.lang == "shiki").unwrap();
        assert_eq!(shiki.default_for.as_deref(), Some("code"));
    }

    #[test]
    fn transform_for_pdf_falls_back_excalidraw_to_pre() {
        // The JS-side enhance() produces a <pre class="excalidraw-block"
        // data-excalidraw-json="…"> with the React-mounted viewer inside.
        // The PDF transform should replace it with a <pre
        // class="excalidraw-fallback">…</pre> showing the original JSON.
        let chapter = r#"<p>before</p>
<pre class="excalidraw-block" data-excalidraw-json="{&quot;elements&quot;:[{&quot;id&quot;:&quot;a&quot;}]}">viewer content</pre>
<p>after</p>"#;
        let out = transform_for_pdf(chapter);
        // The original excalidraw pre is replaced.
        assert!(!out.contains("excalidraw-block"), "excalidraw-block should be replaced in PDF: {}", out);
        // The fallback pre is present with the JSON.
        assert!(out.contains("excalidraw-fallback"), "excalidraw-fallback missing: {}", out);
        assert!(out.contains("elements"), "JSON body missing from fallback: {}", out);
        // Surrounding content is preserved.
        assert!(out.contains("<p>before</p>"));
        assert!(out.contains("<p>after</p>"));
    }

    #[test]
    fn transform_for_pdf_preserves_mermaid_and_svg() {
        // Mermaid and SVG are passthrough — the JS-side enhance() has
        // already rendered them to inline SVG, so the PDF just keeps
        // the HTML.
        let chapter = r#"<pre class="mermaid"><svg viewBox="0 0 100 100"></svg></pre>
<div class="svg-block"><svg viewBox="0 0 200 80"></svg></div>
<pre class="shiki-block"><code>x</code></pre>"#;
        let out = transform_for_pdf(chapter);
        assert!(out.contains("class=\"mermaid\""), "mermaid pre should be preserved");
        assert!(out.contains("class=\"svg-block\""), "svg-block should be preserved");
        assert!(out.contains("class=\"shiki-block\""), "shiki-block should be preserved");
    }

    #[test]
    fn update_excalidraw_block_round_trip() {
        // Write a fixture .md with one excalidraw block, update it, verify
        // the file's content is correct and other fences are untouched.
        let dir = std::env::temp_dir().join("fenceymd_excalidraw_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("chapter.md");
        let original = "\
# Hello

Intro paragraph.

```excalidraw
{\"elements\":[{\"id\":\"old\"}]}
```

Outro.

```mermaid
graph LR
A --> B
```
";
        std::fs::write(&file, original).unwrap();

        let folder = dir.to_string_lossy().to_string();
        let rel_path = "chapter.md".to_string();
        let new_inner = "{\"elements\":[{\"id\":\"new\"}]}".to_string();
        update_excalidraw_block(folder.clone(), rel_path.clone(), 0, new_inner.clone()).unwrap();

        let after = std::fs::read_to_string(&file).unwrap();
        // The new JSON should be present.
        assert!(after.contains("\"id\":\"new\""));
        assert!(!after.contains("\"id\":\"old\""));
        // The mermaid block should be untouched.
        assert!(after.contains("graph LR"));
        assert!(after.contains("A --> B"));
        // A second save should also work.
        let newer = "{\"elements\":[{\"id\":\"newer\"}]}".to_string();
        update_excalidraw_block(folder, rel_path, 0, newer.clone()).unwrap();
        let after2 = std::fs::read_to_string(&file).unwrap();
        assert!(after2.contains("\"id\":\"newer\""));
        assert!(!after2.contains("\"id\":\"new\""));
    }

    /// The editor's ⌘V clipboard-image handler calls `save_clipboard_image`
    /// with bytes the user just pasted. The command must:
    ///  1. Write the bytes to `<folder>/<rel_path>`, creating intermediate
    ///     directories (e.g. `images/`) on demand.
    ///  2. Reject any `rel_path` that escapes `folder` (same canonicalize
    ///     check as `write_file`) — even when the parent dir doesn't
    ///     exist yet, which is the common case for first paste.
    ///  3. Return the absolute path actually written.
    #[test]
    fn save_clipboard_image_writes_and_creates_images_dir() {
        let dir = std::env::temp_dir().join("fenceymd_clip_image_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let folder = dir.to_string_lossy().to_string();
        let rel = "images/pasted-1.png".to_string();
        // A minimal 1×1 PNG. The Rust side doesn't decode it, just
        // writes the bytes through, so the body content doesn't matter.
        let bytes: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
            0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41,
            0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
            0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82,
        ];

        let saved = save_clipboard_image(folder, rel.clone(), bytes.clone()).unwrap();
        let saved_path = std::path::PathBuf::from(&saved);
        assert!(saved_path.starts_with(&dir));
        assert!(saved_path.ends_with("images/pasted-1.png"));
        assert!(dir.join("images").is_dir());
        let read_back = std::fs::read(&saved_path).unwrap();
        assert_eq!(read_back, bytes);

        // Empty bytes are rejected (defense in depth — JS side filters too).
        let err = save_clipboard_image(dir.to_string_lossy().to_string(), "empty.png".into(), vec![]);
        assert!(err.is_err(), "empty bytes should be rejected");
    }

    #[test]
    fn save_clipboard_image_rejects_traversal() {
        let dir = std::env::temp_dir().join("fenceymd_clip_image_traversal_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let folder = dir.to_string_lossy().to_string();
        // Sibling escape.
        let err = save_clipboard_image(
            folder.clone(),
            "../escape.png".into(),
            vec![0x89, 0x50, 0x4E, 0x47],
        );
        assert!(err.is_err(), "traversal should be rejected");
        assert!(!dir.join("escape.png").exists());

        // Nested traversal — `images/../../escape.png` resolves outside
        // the folder after canonicalize.
        let err = save_clipboard_image(
            folder,
            "images/../../escape2.png".into(),
            vec![0x89, 0x50, 0x4E, 0x47],
        );
        assert!(err.is_err(), "nested traversal should be rejected");
    }
}
