// Prevents a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, Debouncer};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

// ── Data returned to the frontend ───────────────────────────────────────────

#[derive(Serialize, Clone)]
struct MdFile {
    /// Relative path under the chosen root, using '/' separators.
    path: String,
    name: String,
    content: String,
}

#[derive(Serialize, Clone)]
struct ScanResult {
    folder_name: String,
    /// Absolute path of the scanned root (used as the persistence key).
    root: String,
    files: Vec<MdFile>,
}

#[derive(Serialize)]
struct RecentEntry {
    path: String,
    name: String,
    exists: bool,
}

// ── Persisted state (app_data_dir/state.json) ────────────────────────────────

#[derive(Serialize, Deserialize, Default, Clone)]
struct FileProgress {
    #[serde(default)]
    scroll: f64,
    #[serde(default)]
    bookmarked: bool,
}

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

const RECENTS_CAP: usize = 12;

fn store_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&dir);
    dir.join("state.json")
}

fn read_store(app: &AppHandle) -> Store {
    let path = store_path(app);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_store(app: &AppHandle, store: &Store) {
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(store_path(app), json);
    }
}

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

fn debug_log_path(app: &AppHandle) -> PathBuf {
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
fn scan_folder(root: &Path) -> ScanResult {
    let folder_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Selected Folder".into());

    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
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
        let content = std::fs::read_to_string(p).unwrap_or_default();
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
        eprintln!("[md-reader] open_folder_path: path is not a dir: {path}");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_log_path(&app))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, format!("[rust] open_folder_path: not a dir: {path}\n").as_bytes())
            });
        return None;
    }
    let scan_start = std::time::Instant::now();
    let result = scan_folder(p);
    let elapsed = scan_start.elapsed();
    let total_bytes: usize = result.files.iter().map(|f| f.content.len()).sum();
    eprintln!(
        "[md-reader] open_folder_path: scanned {} files, {} bytes, {:?}",
        result.files.len(),
        total_bytes,
        elapsed
    );
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(debug_log_path(&app))
        .and_then(|mut f| {
            std::io::Write::write_all(
                &mut f,
                format!(
                    "[rust] open_folder_path: path={path} files={} bytes={} elapsed_ms={}\n",
                    result.files.len(),
                    total_bytes,
                    elapsed.as_millis()
                )
                .as_bytes(),
            )
        });
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
    // Reject traversal: the canonicalized parent must stay inside the root.
    let root_canon = root.canonicalize().map_err(|e| e.to_string())?;
    let parent = target.parent().ok_or("invalid path")?;
    let parent_canon = parent.canonicalize().map_err(|e| e.to_string())?;
    if !parent_canon.starts_with(&root_canon) {
        return Err("refusing to write outside the selected folder".into());
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
            let fence_end_rel = content[byte_pos..].find('\n').unwrap_or(content.len() - byte_pos);
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
#[tauri::command]
fn open_in_external_editor(path: String, editor_override: Option<String>) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("file not found".into());
    }
    if let Some(editor) = editor_override {
        let trimmed = editor.trim();
        if !trimmed.is_empty() {
            let mut parts = trimmed.split_whitespace();
            let bin = parts.next().unwrap_or(trimmed);
            let rest: Vec<&str> = parts.collect();
            let mut cmd = Command::new(bin);
            for a in rest { cmd.arg(a); }
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

// ── PDF export via headless Chrome ────────────────────────────────────────────

/// Render `chapter_html` (already-rendered markdown + mermaid SVGs) inside a
/// full HTML document and generate a PDF using the system's Chrome/Chromium in
/// headless mode. Returns the raw PDF bytes.
///
/// `vars` is the snapshot of CSS custom properties the chapter relies on
/// (theme colors, fonts, spacing) — computed on the frontend from the live
/// `:root` so the PDF matches the on-screen rendering exactly.
#[tauri::command]
async fn print_pdf(
    title: String,
    chapter_html: String,
    _dark: bool, // kept for backwards compat; theme is now read from `vars`
    vars: std::collections::HashMap<String, String>,
) -> Result<Vec<u8>, String> {
    let html = build_print_html(&title, &chapter_html, &vars);

    // Write the self-contained HTML to a temp file so Chrome can open it via file://.
    let tmp_dir = std::env::temp_dir().join("mdreader_pdf");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let html_path = tmp_dir.join("export.html");
    std::fs::write(&html_path, html).map_err(|e| e.to_string())?;

    // Find Chrome on macOS / Linux / Windows.
    let chrome = find_chrome();
    let pdf_path = tmp_dir.join("export.pdf");

    let output = Command::new(&chrome)
        .args([
            "--headless",
            "--no-sandbox",
            "--disable-setuid-sandbox",
            "--disable-gpu",
            "--no-pdf-header-footer",
            "--virtual-time-budget=2000",
            &format!("--print-to-pdf={}", pdf_path.display()),
            &format!("file://{}", html_path.display()),
        ])
        .output()
        .map_err(|e| format!("failed to spawn {}: {}", chrome.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Chrome exited with {}: {}", output.status, stderr));
    }

    let bytes = std::fs::read(&pdf_path).map_err(|e| e.to_string())?;

    // Clean up temp files.
    let _ = std::fs::remove_file(&html_path);
    let _ = std::fs::remove_file(&pdf_path);

    Ok(bytes)
}

fn find_chrome() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for p in &paths {
            if Path::new(p).exists() {
                return PathBuf::from(p);
            }
        }
        // Try PATH
        if let Ok(p) = std::process::Command::new("which").arg("google-chrome").output() {
            let s = String::from_utf8_lossy(&p.stdout).trim().to_string();
            if !s.is_empty() && Path::new(&s).exists() {
                return PathBuf::from(s);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let paths = [
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
            "/snap/bin/chromium",
        ];
        for p in &paths {
            if Path::new(p).exists() {
                return PathBuf::from(p);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let program_dirs = [
            std::env::var("ProgramFiles").ok(),
            std::env::var("ProgramFiles(x86)").ok(),
            std::env::var("LOCALAPPDATA").ok(),
        ];
        for dir in program_dirs.into_iter().flatten() {
            let chrome = PathBuf::from(dir).join("Google/Chrome/Application/chrome.exe");
            if chrome.exists() {
                return chrome;
            }
            let chromium = PathBuf::from(dir).join("Chromium/Application/chrome.exe");
            if chromium.exists() {
                return chromium;
            }
        }
    }
    // Fallback to PATH
    PathBuf::from("google-chrome")
}

/// Read katex's bundled CSS from node_modules so math renders in the printed
/// PDF. The path is resolved relative to CARGO_MANIFEST_DIR (the src-tauri/
/// directory at build time) — `../node_modules/katex/dist/katex.min.css`.
/// This works in both dev (cargo run from src-tauri) and release (node_modules
/// is shipped alongside the binary in the .app bundle). If the file is
/// missing, the caller logs a warning and skips — the PDF will still render
/// text content, just with raw `$…$` math.
fn read_katex_css() -> Result<String, String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let candidates = [
        // dev / source build: walk up from src-tauri/ to repo root
        PathBuf::from(manifest_dir).join("../node_modules/katex/dist/katex.min.css"),
        // release build: alongside the binary in the .app bundle's Resources
        PathBuf::from(manifest_dir).join("../../../node_modules/katex/dist/katex.min.css"),
        // resource_path at runtime (Tauri resolves this to the .app's Resources)
        PathBuf::from(manifest_dir).join("node_modules/katex/dist/katex.min.css"),
    ];
    for path in &candidates {
        if path.exists() {
            return std::fs::read_to_string(path)
                .map_err(|e| format!("read {}: {}", path.display(), e));
        }
    }
    Err("katex.min.css not found in any candidate path".to_string())
}

// ── Renderer manifest (Phase 2) ───────────────────────────────────────────────
//
// The PDF pipeline needs to know which fence languages the JS-side registry
// handles, because some renderers (mermaid, svg) produce HTML that prints
// fine, while others (excalidraw) mount interactive components that don't
// survive headless Chrome rasterization. We embed the JS manifest at compile
// time via `include_str!` so the Rust side and the JS side stay in sync —
// adding a renderer on the JS side means adding a row to the same JSON the
// Rust side reads.

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RendererKind { Fence, Inline, Math }

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)] // deserialized for validation; not every field is read
struct ManifestEntry {
    lang: String,
    kind: RendererKind,
    // `defaultFor` is camelCase in the manifest to match the JS-side
    // property name. We rename on deserialize so the Rust struct stays
    // idiomatic.
    #[serde(rename = "defaultFor", default)]
    default_for: Option<String>,
    #[serde(default)]
    module: Option<String>,
}

/// The PDF rendering mode for a given fence language.
///
/// - `Passthrough`: the JS side has already produced printable HTML
///   (svg → `<div class="svg-block"><svg>`, shiki → `<pre class="shiki">`,
///   math → `<span class="katex">`, html → `<div class="html-block">`,
///   mermaid → `<pre class="mermaid"><svg>`). The PDF CSS handles styling.
/// - `PreFallback`: the JS side has mounted an interactive component
///   (excalidraw) that doesn't render in headless Chrome. We replace
///   the wrapper with a `<pre>` showing the source JSON so the printed
///   PDF still includes the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PdfMode { Passthrough, PreFallback }

fn pdf_mode_for(lang: &str) -> PdfMode {
    match lang {
        "excalidraw" => PdfMode::PreFallback,
        // The JS manifest is the source of truth; everything else is
        // passthrough. If a new fence is added with `kind: "fence"` and
        // it's not excalidraw, it joins the passthrough set.
        _ => PdfMode::Passthrough,
    }
}

/// Minimal HTML-attribute escape for safe insertion into a `<pre>`.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

/// Walk `chapter_html` and apply PDF-specific transforms per the manifest.
///
/// Currently:
/// - `excalidraw` blocks: the JS-side Svelte mount produces DOM that
///   doesn't render in headless Chrome. We replace the whole `<pre>`
///   with a `<pre class="excalidraw-fallback">` showing the original
///   JSON (carried on `data-excalidraw-json` by markdown.js). The
///   fallback is recognizable but readable in the printed PDF.
///
/// We operate on the source HTML (string-level) because the chapter
/// is already fully rendered — the JS-side enhance() has done the
/// expensive work. The transform is intentionally small and cheap.
fn transform_for_pdf(html: &str) -> String {
    // Two patterns the JS-side enhance() emits:
    //   <pre class="excalidraw-block" data-excalidraw-json="…">…
    //   <pre class="excalidraw-block …" data-excalidraw-json='…'>…
    // The JSON may be empty if the JS side didn't set it (e.g. the
    // source was malformed). We use a single pass to find every
    // <pre …excalidraw-block… data-excalidraw-json="…">…</pre>.
    //
    // We use a regex-free manual walk to avoid pulling the `regex`
    // crate in (deps are tight); the chapter_html is well-formed
    // HTML emitted by showdown + our DOM ops.

    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() + 64);
    let mut i = 0usize;
    while i < bytes.len() {
        // Look for the next <pre …excalidraw-block…> opening tag.
        if let Some(rel) = find_subsequence(bytes, i, b"<pre") {
            // Find the end of this opening tag.
            let tag_start = i + rel;
            let tag_close = match find_subsequence(bytes, tag_start + 4, b">") {
                Some(p) => p,
                None => {
                    out.push_str(&html[i..]);
                    return out;
                }
            };
            let tag = &html[tag_start..=tag_close];
            let is_excalidraw = tag.contains("excalidraw-block");
            if !is_excalidraw {
                // Not an excalidraw pre — emit everything up to + incl.
                // the opening tag and keep scanning from after it.
                out.push_str(&html[i..=tag_close]);
                i = tag_close + 1;
                continue;
            }
            // Find the matching </pre>. The chapter HTML we produce
            // has no nested <pre> (we don't allow it), so the first
            // `</pre>` after the opening tag is the close.
            let pre_close = match find_subsequence(bytes, tag_close + 1, b"</pre>") {
                Some(p) => p,
                None => {
                    out.push_str(&html[i..]);
                    return out;
                }
            };
            // Emit everything up to (but not including) the original
            // <pre> opening tag.
            out.push_str(&html[i..tag_start]);
            // Dispatch via the manifest-driven table. Currently only
            // excalidraw needs a transform (Svelte mount → JSON pre);
            // every other fence is passthrough and the original <pre>
            // is preserved. We still consult pdf_mode_for so adding a
            // new PreFallback renderer in the future is a one-line
            // change in the table, not a new code path.
            let lang = extract_attr(tag, "data-excalidraw-json")
                .map(|_| "excalidraw".to_string())
                .unwrap_or_default();
            match pdf_mode_for(&lang) {
                PdfMode::Passthrough => {
                    // Keep the original <pre> intact.
                    out.push_str(&html[tag_start..pre_close + "</pre>".len()]);
                }
                PdfMode::PreFallback => {
                    // Replace the mounted viewer with a JSON <pre>.
                    let json = extract_attr(tag, "data-excalidraw-json")
                        .unwrap_or_else(|| "{\"elements\":[]}".to_string());
                    out.push_str("<pre class=\"excalidraw-fallback\">");
                    out.push_str(&html_escape(&json));
                    out.push_str("</pre>");
                }
            }
            i = pre_close + "</pre>".len();
        } else {
            out.push_str(&html[i..]);
            return out;
        }
    }
    out
}

/// Find the first occurrence of `needle` in `haystack` starting at
/// position `from` (byte offset). Returns the absolute byte offset
/// of the match, or None.
fn find_subsequence(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() || needle.is_empty() || needle.len() > haystack.len() - from {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/// Extract the value of a single-quoted or double-quoted attribute
/// from a tag string. Returns None if the attribute is absent or the
/// value contains a quote (rare in well-formed HTML).
fn extract_attr<'a>(tag: &'a str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse the embedded manifest into a Vec of entries. We don't use the
/// result for dispatch (pdf_mode_for is the lookup), but parsing it
/// catches typos and missing files at compile time. The function is
/// `const`-ish — it parses the embedded string into memory and is
/// called once per `build_print_html` invocation. Manifest is small.
fn load_renderer_manifest() -> Vec<ManifestEntry> {
    let raw = include_str!("../../src/lib/renderers/manifest.json");
    serde_json::from_str(raw).unwrap_or_else(|e| {
        eprintln!("[md-reader] renderer manifest parse error: {}", e);
        Vec::new()
    })
}

fn build_print_html(
    title: &str,
    chapter_html: &str,
    vars: &std::collections::HashMap<String, String>,
) -> String {
    // Theme detection: the app sets `data-theme="dark"` on <html>. The vars
    // map carries the resolved palette, but the attribute is still useful for
    // a few overrides — we mirror the app by setting it on <html>.
    let _ = vars; // theme is encoded in the var values themselves
    // Build a :root block from the live CSS variables the app uses, so the
    // PDF inherits the user's theme (light/dark) + font choices.
    let mut root_vars = String::new();
    for (k, v) in vars.iter() {
        root_vars.push_str(&format!("  {}: {};\n", k, v));
    }

    // Embed katex CSS so rendered math (katex emits <span class="katex">…)
    // is legible in the PDF. node_modules is shipped with the app, both in
    // dev and release. Path is computed from CARGO_MANIFEST_DIR at compile
    // time, but the file is read at runtime so the PDF stays in sync with
    // the installed katex version. If the file is missing, skip with a
    // warning — the PDF will show raw `$…$` text (graceful degrade).
    let katex_css = read_katex_css().unwrap_or_else(|e| {
        eprintln!("[md-reader] katex CSS not embedded in PDF: {}", e);
        String::new()
    });

    // Minimal shiki block CSS — the inline spans carry their own colors via
    // CSS variables, but the wrapper card needs padding/border/radius. We
    // duplicate the reader's `.shiki-block` rules here so the PDF matches
    // the on-screen look without depending on a CSS file.
    let shiki_css = r#"
.shiki, .shiki-block {
  background: var(--shiki-light-bg, #ffffff);
  color: var(--shiki-light, #24292f);
  border: 1px solid var(--surface-variant);
  border-radius: var(--radius-md, 8px);
  padding: var(--space-6, 1.5rem);
  margin: 0 0 var(--space-6, 1.5rem) 0;
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 0.82rem;
  line-height: 1.7;
  overflow-x: auto;
}
.shiki code, .shiki-block code { background: transparent; border: none; padding: 0; color: inherit; }
.shiki span { color: var(--shiki-light, #24292f); }
[data-theme="dark"] .shiki, [data-theme="dark"] .shiki-block {
  background: var(--shiki-dark-bg, #0d1117);
}
[data-theme="dark"] .shiki span { color: var(--shiki-dark, #e6edf3); }
"#
    .to_string();

    // Phase 2: read the manifest at compile time. The Vec is held in
    // a local here so the dispatch is "manifest-driven" — the function
    // table below uses pdf_mode_for() which is keyed off the same set
    // of langs the manifest declares.
    let _manifest = load_renderer_manifest();

    // Manifest-driven pre-filter: walk the chapter_html for <pre> elements
    // that need a PDF-specific transform (currently: excalidraw → JSON
    // <pre> fallback because the Svelte-mounted viewer doesn't render in
    // headless Chrome). For all other fence languages the JS-side
    // enhance() has already produced printable HTML that the CSS styles.
    let chapter_html = transform_for_pdf(chapter_html);

    format!(r#"<!DOCTYPE html>
<html lang="en" data-theme="{dark}">
<head>
<meta charset="UTF-8">
<title>{title}</title>
<style>
/* katex — embedded from node_modules so math renders in the PDF */
{katex_css}

/* shiki block wrapper — minimal rules so highlighted code looks the same as on screen */
{shiki_css}

:root {{
{root_vars}}}

/* ── Force a print-friendly LIGHT palette ───────────────────────────────
   The vars above are snapshotted from the app's CURRENT theme. If the user
   exports while in dark mode they carry dark colors, which print as a dark
   rectangle floating in white page margins ("just a box") and waste ink. A
   printed/exported document should always be light and high-contrast, so we
   override every COLOR var here — this :root block wins by source order.
   Fonts, spacing, and radii from the snapshot above are kept. */
:root {{
  --surface: #ffffff;
  --surface-variant: #e2e5e4;
  --surface-container-low: #f5f6f5;
  --surface-container-lowest: #f7f8f7;
  --surface-container: #eeeeed;
  --surface-container-high: #e7e7e6;
  --ink: #1f2222;
  --ink-secondary: #4a4d4d;
  --ink-muted: #6c6e6e;
  --tertiary: #a33e34;
  --tertiary-dim: rgba(163, 62, 52, 0.08);
}}

* {{ box-sizing: border-box; margin: 0; padding: 0; }}

html, body {{
  width: 100%;
  background: #ffffff;
}}

body {{
  font-family: var(--font-serif, Georgia, 'Times New Roman', serif);
  color: var(--ink);
  font-size: 11pt;
  line-height: 1.8;
  padding: 0;
  margin: 0;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}}

.header {{
  margin-bottom: var(--space-6, 1.5rem);
  border-bottom: 1px solid var(--surface-variant);
  padding-bottom: var(--space-3, 0.75rem);
}}
.header h1 {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 2.2rem;
  font-weight: 600;
  letter-spacing: -0.025em;
  line-height: 1.2;
  color: var(--ink);
  margin-bottom: var(--space-2, 0.5rem);
}}

/* ── chapter-markdown — matches the in-app reader styles ─────────── */
.chapter-markdown {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 1rem;
  line-height: 1.8;
  color: var(--ink);
}}
.chapter-markdown h1 {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 2.2rem;
  font-weight: 600;
  letter-spacing: -0.025em;
  line-height: 1.2;
  color: var(--ink);
  margin: var(--space-10, 2.5rem) 0 var(--space-6, 1.5rem);
  page-break-after: avoid;
}}
.chapter-markdown h2 {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 1.4rem;
  font-weight: 600;
  letter-spacing: -0.015em;
  margin: var(--space-10, 2.5rem) 0 var(--space-4, 1rem);
  padding-bottom: var(--space-3, 0.75rem);
  border-bottom: 1px solid var(--surface-variant);
  color: var(--ink);
  page-break-after: avoid;
}}
.chapter-markdown h3 {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 1.1rem;
  font-weight: 600;
  margin: var(--space-8, 2rem) 0 var(--space-3, 0.75rem);
  color: var(--ink);
  page-break-after: avoid;
}}
.chapter-markdown h4 {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 1rem;
  font-weight: 600;
  margin: var(--space-6, 1.5rem) 0 var(--space-3, 0.75rem);
  color: var(--ink);
}}
.chapter-markdown p {{
  margin: 0 0 var(--space-5, 1.25rem) 0;
  line-height: 1.8;
  color: var(--ink);
}}
.chapter-markdown a {{
  color: var(--tertiary, #2a8b8b);
  text-decoration: none;
  border-bottom: 1px solid var(--surface-variant);
  word-break: break-word;
}}

.chapter-markdown code {{
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 0.82em;
  background: var(--surface-container-lowest);
  color: var(--ink);
  padding: 2px 5px;
  border-radius: var(--radius-sm, 2px);
  border: 1px solid var(--surface-variant);
  /* Inline code is `display: inline` by default — the background naturally
     only covers the text glyphs (the same as the in-app reader). The border
     + padding give it a clear pill look. */
  word-break: break-word;
}}
.chapter-markdown pre {{
  background: var(--surface-container-lowest);
  border-radius: var(--radius-md, 4px);
  padding: var(--space-6, 1.5rem);
  overflow-x: auto;
  white-space: pre-wrap;
  word-wrap: break-word;
  margin: 0 0 var(--space-6, 1.5rem) 0;
  border: 1px solid var(--surface-variant);
  page-break-inside: avoid;
}}
.chapter-markdown pre code {{
  background: none;
  border: none;
  padding: 0;
  font-size: 0.82rem;
  line-height: 1.7;
  white-space: pre-wrap;
  color: var(--ink);
}}

.chapter-markdown blockquote {{
  border-left: 3px solid var(--tertiary);
  padding: var(--space-4, 1rem) var(--space-6, 1.5rem);
  margin: var(--space-6, 1.5rem) 0;
  background: var(--tertiary-dim);
  color: var(--ink);
  font-style: italic;
  border-radius: 0 var(--radius-md, 4px) var(--radius-md, 4px) 0;
  page-break-inside: avoid;
}}

.chapter-markdown ul, .chapter-markdown ol {{
  padding-left: var(--space-6, 1.5rem);
  margin-bottom: var(--space-4, 1rem);
}}
.chapter-markdown li {{ margin-bottom: var(--space-2, 0.5rem); line-height: 1.7; }}

.chapter-markdown table {{
  border-collapse: collapse;
  width: 100%;
  table-layout: auto;
  margin: var(--space-4, 1rem) 0;
  font-size: 0.9em;
}}
.chapter-markdown th {{
  background: var(--surface-container-low);
  font-weight: 600;
  text-align: left;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--surface-variant);
  color: var(--ink);
  word-break: break-word;
}}
.chapter-markdown td {{
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--surface-variant);
  color: var(--ink);
  word-break: break-word;
}}
.chapter-markdown tr:nth-child(even) td {{ background: var(--surface-container-lowest); }}

.chapter-markdown hr {{
  border: none;
  border-top: 1px solid var(--surface-variant);
  margin: var(--space-6, 1.5rem) 0;
}}
.chapter-markdown img {{
  max-width: 100%;
  border-radius: var(--radius-md, 4px);
  margin: var(--space-4, 1rem) 0;
}}

/* Mermaid diagrams — keep their own background, fit the page width. */
.chapter-markdown .mermaid,
.chapter-markdown pre.mermaid {{
  background: transparent;
  border: 1px solid var(--surface-variant);
  text-align: center;
  padding: var(--space-4, 1rem);
  border-radius: var(--radius-md, 4px);
  margin: var(--space-4, 1rem) 0;
  page-break-inside: avoid;
}}
.chapter-markdown .mermaid svg {{
  max-width: 100%;
  /* Cap height to just under the A4 printable height so a tall, detailed
     diagram scales down to fit on one page instead of being clipped by the
     page-break-inside: avoid above. preserveAspectRatio keeps it undistorted. */
  max-height: 9.5in;
  height: auto;
  display: block;
  margin: 0 auto;
}}

/* Callout blocks (e.g. :::note ... :::) */
.chapter-markdown .callout {{
  background: var(--surface-container-lowest);
  border-left: 3px solid var(--tertiary);
  padding: var(--space-5, 1.25rem) var(--space-6, 1.5rem);
  border-radius: 0 var(--radius-md, 4px) var(--radius-md, 4px) 0;
  margin: var(--space-6, 1.5rem) 0;
  color: var(--ink);
  page-break-inside: avoid;
}}
.chapter-markdown .callout-title {{
  font-family: var(--font-serif, Georgia, serif);
  font-size: 1rem;
  font-weight: 600;
  color: var(--ink);
  margin-bottom: var(--space-2, 0.5rem);
}}

/* SVG inline blocks (the in-app `<pre class="svg-block">` becomes raw SVG). */
.chapter-markdown .svg-block {{
  background: var(--surface-container-low);
  border: 1px solid var(--surface-variant);
  border-radius: var(--radius-md, 4px);
  padding: var(--space-4, 1rem);
  text-align: center;
}}
.chapter-markdown .svg-block svg {{ max-width: 100%; max-height: 9.5in; height: auto; }}

/* Excalidraw scenes: the JS-side enhance() mounts an interactive
   viewer; the Rust PDF transform replaces it with `.excalidraw-fallback`
   showing the original JSON, so the printed PDF is still readable. */
.chapter-markdown .excalidraw-fallback {{
  background: var(--surface-container-lowest);
  border: 1px solid var(--surface-variant);
  border-radius: var(--radius-md, 4px);
  padding: var(--space-4, 1rem);
  margin: var(--space-4, 1rem) 0;
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 0.7rem;
  color: var(--ink-muted);
  max-height: 200px;
  overflow: auto;
  page-break-inside: avoid;
}}

/* CSV fence — the JS-side enhance() turns ```csv into a real <table>
   inside a .csv-block card. Match the reader's typography. */
.chapter-markdown .csv-block {{
  background: var(--surface-container-lowest);
  border: 1px solid var(--surface-variant);
  border-radius: var(--radius-md, 4px);
  padding: var(--space-3, 0.75rem) var(--space-5, 1.25rem);
  margin: var(--space-4, 1rem) 0;
  page-break-inside: avoid;
}}
.chapter-markdown .csv-block table {{
  width: 100%;
  border-collapse: collapse;
  /* `auto` lets columns size to content and wrap; a wide CSV then fits the
     page instead of overflowing off the right edge and being clipped. */
  table-layout: auto;
  font-size: 0.78rem;
  font-family: var(--font-sans, sans-serif);
}}
.chapter-markdown .csv-block th {{
  background: transparent;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  text-align: left;
  font-weight: 600;
  color: var(--ink);
  border-bottom: 1.5px solid var(--ink-muted);
  /* Wrap instead of nowrap so wide headers don't force horizontal overflow. */
  white-space: normal;
  word-break: break-word;
}}
.chapter-markdown .csv-block td {{
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-bottom: 1px solid var(--surface-variant);
  color: var(--ink);
  vertical-align: top;
  white-space: normal;
  word-break: break-word;
}}
.chapter-markdown .csv-block tbody tr:nth-child(even) td {{
  background: var(--surface-container-low);
}}
.chapter-markdown .csv-block-note {{
  margin-top: var(--space-3, 0.75rem);
  padding-top: var(--space-2, 0.5rem);
  border-top: 1px dashed var(--surface-variant);
  color: var(--ink-muted);
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 0.7rem;
}}

/* Strip the in-app diagram toolbar (Copy / PNG / theme toggle) — print only. */
.chapter-markdown .diagram-tools,
.chapter-markdown .copy-btn,
.chapter-markdown .code-block-wrapper .copy-btn {{ display: none !important; }}

/* Print page setup: A4 portrait, modest margins. */
@page {{
  size: A4;
  margin: 18mm;
}}

@media print {{
  .header {{ page-break-after: avoid; }}
  pre, blockquote, table, .mermaid {{ page-break-inside: avoid; }}
}}
</style>
</head>
<body>
<div class="header">
  <h1>{title}</h1>
</div>
<div class="chapter-markdown">
{chapter_html}
</div>
</body>
</html>"#,
      title = title,
      chapter_html = chapter_html,
      root_vars = root_vars,
      dark = "light",
    )
}

// ── Live file watching ─────────────────────────────────────────────────────────

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
    *state.lock().unwrap() = Some(debouncer);
    Ok(())
}

/// Scan a folder by absolute path without recording it (test helper; harmless).
#[tauri::command]
fn scan_path(path: String) -> ScanResult {
    scan_folder(Path::new(&path))
}

/// Internal helper: append a one-shot line to the debug log from Rust code
/// (e.g. the watcher, the PDF pipeline). Same format as the JS-side log.
fn log_from_rust(app: &AppHandle, line: &str) {
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

fn main() {
    tauri::Builder::default()
        .manage::<WatcherState>(Mutex::new(None))
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
            print_pdf,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> PathBuf {
        // Per-test unique temp dir so parallel execution doesn't race
        // on a fixed path. The old single-name fixture flaked when one
        // test removed the dir while another was reading from it.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pid = std::process::id();
        let base = std::env::temp_dir().join(format!("mdreader_test_book_{pid}_{n}"));
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
        assert!(r.folder_name.starts_with("mdreader_test_book"));
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
    fn write_file_rejects_traversal() {
        let base = fixture();
        let folder = base.to_string_lossy().to_string();
        // Legit write inside the folder succeeds.
        assert!(write_file(folder.clone(), "intro.md".into(), "# Edited\nx".into()).is_ok());
        assert_eq!(fs::read_to_string(base.join("intro.md")).unwrap(), "# Edited\nx");
        // Traversal outside the folder is rejected.
        assert!(write_file(folder, "../escape.md".into(), "nope".into()).is_err());
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
        assert!(folder_name.starts_with("mdreader_test_book"));
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
        let path = std::env::temp_dir().join("mdreader_verify.html");
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
        let dir = std::env::temp_dir().join("mdreader_excalidraw_test");
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
        let dir = std::env::temp_dir().join("mdreader_clip_image_test");
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
        let dir = std::env::temp_dir().join("mdreader_clip_image_traversal_test");
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
