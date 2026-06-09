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
    let result = scan_folder(dir.path());
    record_open(&app, &result.root);
    Some(result)
}

/// Open a folder by absolute path (recents click / reopen). Returns None if the
/// path no longer exists. Records it as last-opened + most recent.
#[tauri::command]
fn open_folder_path(app: AppHandle, path: String) -> Option<ScanResult> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return None;
    }
    let result = scan_folder(p);
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

* {{ box-sizing: border-box; margin: 0; padding: 0; }}

html, body {{
  width: 100%;
  background: var(--surface);
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
  margin: var(--space-4, 1rem) 0;
  font-size: 0.95em;
}}
.chapter-markdown th {{
  background: var(--surface-container-low);
  font-weight: 600;
  text-align: left;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--surface-variant);
  color: var(--ink);
}}
.chapter-markdown td {{
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--surface-variant);
  color: var(--ink);
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
.chapter-markdown .svg-block svg {{ max-width: 100%; height: auto; }}

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
            let result = scan_folder(&root_for_cb);
            let _ = app_for_cb.emit("library-changed", result);
        },
    )
    .map_err(|e| e.to_string())?;

    debouncer
        .watcher()
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    // Keep the debouncer alive by storing it in managed state (drops the old one).
    *state.lock().unwrap() = Some(debouncer);
    Ok(())
}

/// Scan a folder by absolute path without recording it (test helper; harmless).
#[tauri::command]
fn scan_path(path: String) -> ScanResult {
    scan_folder(Path::new(&path))
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
            rename_file,
            watch_folder,
            scan_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> PathBuf {
        let base = std::env::temp_dir().join("mdreader_test_book");
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
        assert_eq!(r.folder_name, "mdreader_test_book");
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["intro.md", "part-i/ch1.md"]);
        let ch1 = r.files.iter().find(|f| f.path == "part-i/ch1.md").unwrap();
        assert_eq!(ch1.content, "# Ch1\nbody");
        assert_eq!(ch1.name, "ch1.md");
        assert!(r.root.ends_with("mdreader_test_book"));
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
        assert_eq!(json["folder_name"], "mdreader_test_book");
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
}
