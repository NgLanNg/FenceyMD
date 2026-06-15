//! PDF export via headless Chrome.
//!
//! The reader renders a chapter to a self-contained HTML document
//! (`build_print_html`, which inlines KaTeX CSS, forces a light palette, and
//! falls back to a `<pre>` for fence languages Chrome can't paint), writes it
//! to a per-call temp dir, and shells out to headless Chrome's
//! `--print-to-pdf`. PDFs are ALWAYS light — edit `build_print_html` here for
//! print styling, never `app.css`.
//!
//! Only `print_pdf` (the Tauri command) and `build_print_html` (exercised by
//! the test suite in `main.rs`) cross the module boundary; everything else is
//! private to this module.

use std::path::{Path, PathBuf};
use std::process::Command;

// ── PDF export via headless Chrome ────────────────────────────────────────────

/// Monotonic counter giving each `print_pdf` call a unique temp dir, so
/// concurrent exports don't clobber a shared file.
static PDF_EXPORT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Render `chapter_html` (already-rendered markdown + mermaid SVGs) inside a
/// full HTML document and generate a PDF using the system's Chrome/Chromium in
/// headless mode. Returns the raw PDF bytes.
///
/// `vars` is the snapshot of CSS custom properties the chapter relies on
/// (theme colors, fonts, spacing) — computed on the frontend from the live
/// `:root` so the PDF matches the on-screen rendering exactly.
#[tauri::command]
pub(crate) async fn print_pdf(
    title: String,
    chapter_html: String,
    vars: std::collections::HashMap<String, String>,
) -> Result<Vec<u8>, String> {
    let html = build_print_html(&title, &chapter_html, &vars);

    // Write the self-contained HTML to a temp file so Chrome can open it via
    // file://. Use a UNIQUE subdir per call (pid + monotonic counter): two
    // concurrent exports would otherwise clobber a shared export.html/.pdf, and
    // a fresh dir guarantees `read(pdf_path)` can't return a stale PDF left by a
    // previous run if Chrome exits 0 without actually writing one.
    let uniq = format!(
        "{}-{}",
        std::process::id(),
        PDF_EXPORT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let tmp_dir = std::env::temp_dir().join("fenceymd_pdf").join(&uniq);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let html_path = tmp_dir.join("export.html");
    std::fs::write(&html_path, html).map_err(|e| e.to_string())?;

    // Find Chrome on macOS / Linux / Windows.
    let chrome = find_chrome();
    let pdf_path = tmp_dir.join("export.pdf");
    let file_url = format!("file://{}", html_path.display());
    let print_arg = format!("--print-to-pdf={}", pdf_path.display());

    // Render Chrome's sandbox ENABLED (the secure default). `--no-sandbox`
    // disables Chrome's primary containment layer and is a well-known footgun;
    // a normal desktop launch doesn't need it. We only fall back to it if the
    // sandboxed run fails — which happens in some Linux/container environments
    // (e.g. running as root, or without user-namespace support) where Chrome
    // refuses to start otherwise. Secure by default, resilient where required.
    let run = |no_sandbox: bool| -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new(&chrome);
        cmd.arg("--headless");
        if no_sandbox {
            cmd.args(["--no-sandbox", "--disable-setuid-sandbox"]);
        }
        cmd.args([
            "--disable-gpu",
            "--no-pdf-header-footer",
            "--virtual-time-budget=2000",
            &print_arg,
            &file_url,
        ]);
        cmd.output()
    };

    let mut output = run(false)
        .map_err(|e| format!("failed to spawn {}: {}", chrome.display(), e))?;
    if !output.status.success() {
        // Retry with the sandbox disabled (constrained Linux/container hosts).
        let first_err = String::from_utf8_lossy(&output.stderr).to_string();
        eprintln!("[fenceymd] PDF: sandboxed Chrome failed ({first_err}); retrying with --no-sandbox");
        output = run(true)
            .map_err(|e| format!("failed to spawn {}: {}", chrome.display(), e))?;
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Chrome exited with {}: {}", output.status, stderr));
    }

    let bytes = std::fs::read(&pdf_path).map_err(|e| e.to_string())?;

    // Clean up the per-call temp dir (and its files).
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(bytes)
}

/// Locate the system Chrome/Chromium binary for the headless PDF render.
/// Probes the well-known per-OS install locations first, then falls back to a
/// bare `google-chrome` (resolved via PATH at spawn time). The returned path is
/// only ever passed to `Command::new` as the program — never through a shell —
/// so a path containing spaces is safe. If nothing is found the caller's spawn
/// fails and `print_pdf` surfaces the error.
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

/// How a manifest entry attaches to markdown, mirroring the JS registry's
/// `kind` field: a fenced code block, an inline span, or math.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RendererKind { Fence, Inline, Math }

/// One row of the shared renderer manifest (`src/lib/renderers/manifest.json`).
/// Deserialized chiefly to validate the JSON at compile/test time — see
/// `load_renderer_manifest` — so not every field is consumed by Rust.
#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)] // deserialized for validation; not every field is read
pub(crate) struct ManifestEntry {
    pub(crate) lang: String,
    kind: RendererKind,
    // `defaultFor` is camelCase in the manifest to match the JS-side
    // property name. We rename on deserialize so the Rust struct stays
    // idiomatic.
    #[serde(rename = "defaultFor", default)]
    pub(crate) default_for: Option<String>,
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

/// Map a fence language to its PDF rendering mode. The single source of truth
/// for which renderers need a print-time fallback: only `excalidraw` (an
/// interactive Svelte mount that doesn't survive headless Chrome) is
/// `PreFallback`; everything else is `Passthrough`. Adding another fallback
/// renderer is a one-line addition here, not a new code path in
/// `transform_for_pdf`.
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
pub(crate) fn transform_for_pdf(html: &str) -> String {
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
pub(crate) fn load_renderer_manifest() -> Vec<ManifestEntry> {
    let raw = include_str!("../../src/lib/renderers/manifest.json");
    serde_json::from_str(raw).unwrap_or_else(|e| {
        eprintln!("[fenceymd] renderer manifest parse error: {}", e);
        Vec::new()
    })
}

/// Wrap an already-rendered chapter in a self-contained HTML document for the
/// headless-Chrome PDF render. `vars` is the live `:root` CSS-custom-property
/// snapshot (fonts/spacing/radii are honored), but COLOR vars are deliberately
/// overridden to a fixed light palette so exports are always print-friendly
/// regardless of the in-app theme — edit print styling HERE, never the app CSS.
///
/// Inlines katex + shiki CSS so math and highlighted code are legible offline,
/// and runs `transform_for_pdf` to swap interactive fences for printable
/// fallbacks. `title` is UNTRUSTED (first heading / file name) and is
/// HTML-escaped before interpolation; `chapter_html` is trusted (sanitized
/// upstream by `renderMarkdown`) and embedded verbatim. Returns the full HTML.
pub(crate) fn build_print_html(
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
        eprintln!("[fenceymd] katex CSS not embedded in PDF: {}", e);
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
      // Escape the title: it's the untrusted chapter title (first heading /
      // file name) and is interpolated into <title> and <h1>. Without this a
      // title like `</title><script>…` would execute in the headless-Chrome
      // render. (chapter_html is already sanitized upstream by renderMarkdown.)
      title = html_escape(title),
      chapter_html = chapter_html,
      root_vars = root_vars,
      dark = "light",
    )
}
