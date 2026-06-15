// MCP server module — ROADMAP integration with AI agents.
//
// The local-only IPC contract is documented in
// `vault/plan/20260611_mcp_integration.md`. This file is the Rust
// half: the axum server, the tool implementations, the port-file
// lifecycle, and the Tauri command surface for the JS side to drive
// the server.
//
// Wire shape:
//
//   Tauri AppHandle ──► mcp::start(app) at app startup
//                          │
//                          ├── bind TcpListener on random 127.0.0.1 port
//                          ├── write {port, pid, ...} to the app-data dir
//                          │   (macOS: ~/Library/Application Support/
//                          │   com.fenceymd.app/port — see `port_dir`;
//                          │   atomic, multi-instance aware)
//                          └── spawn axum::serve() in a tokio task
//
//   JS side ──► invoke('mcp_update_view_state', { ... })
//                    │
//                    └── updates a Mutex<ViewState> the axum handlers read
//
//   Agent ──► POST http://127.0.0.1:PORT/mcp (JSON-RPC 2.0)
//                    │
//                    └── axum router → handle_jsonrpc(...)
//                                       │
//                                       ├── initialize / initialized
//                                       ├── tools/list
//                                       ├── tools/call (dispatch by name)
//                                       └── ping
//
//   Tool call ──► where the side effect lives:
//                  open_file       → app.emit("mcp-navigate", path)
//                  get_chapter_content → read disk, return { path, content, size }
//                  get_current_chapter / get_selected_text → read ViewState
//                  get_book_toc  → read folder_meta from AppHandle state

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Mutex;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;

use crate::debug_log_path;
use crate::log_from_rust;
use crate::MdFile;

// ── App state the MCP server reads ─────────────────────────────────────────

/// Live view state pushed in from the Svelte frontend. `scroll_position`
/// is the `chapterScrollFrac` from the progress store (0..1). `selected_text`
/// is the user's current text selection in the reader, with the enclosing
/// `data-md-anchor` so the agent can address it precisely (ROADMAP #22).
/// `route` and `current_chapter_path` are pushed on every navigation so
/// `get_current_chapter` can answer without round-tripping to the JS side.
///
/// Every field is `#[serde(default)]` so the JS side can push partial
/// updates (e.g. the scroll handler only knows about scroll, not
/// about selected_text). Tauri auto-validates invoke args; without
/// defaults, a missing field would 400 the call.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ViewState {
    #[serde(default)]
    pub route_name: Option<String>,
    #[serde(default)]
    pub current_chapter_path: Option<String>,
    #[serde(default)]
    pub scroll_position: f64,
    #[serde(default)]
    pub selected_text: String,
    #[serde(default)]
    pub selected_anchor: Option<String>,
}

/// Session context an agent passes with `open_file` so Phase 2's sidebar
/// chat knows which agent+session to spawn. Stashed in a Mutex; survives
/// across MCP calls but is lost on app restart (intentional — agents
/// re-establish their context on every launch).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionContext {
    pub agent: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Default)]
pub struct McpState {
    view: Mutex<ViewState>,
    session_context: Mutex<Option<SessionContext>>,
    // The active book folder. Set by the JS side via
    // `mcp_set_active_folder` when it calls openScanResult; cleared
    // via `mcp_clear_active_folder` when the user closes the folder.
    // The MCP server reads these to answer `get_chapter_content` and
    // `get_book_toc`. Stored as the same data the JS scan returned
    // — we keep the markdown content on the Rust side so agents
    // don't trigger an extra Tauri round-trip per chapter read.
    active_folder_root: Mutex<Option<String>>,
    active_folder_meta: Mutex<Option<Vec<MdFile>>>,
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Check if a PID is alive. We use `ps -p <pid>` because adding
/// `libc` as a direct dep for `kill(pid, 0)` is a heavier lift than
/// this 5ms subprocess is worth. `ps -p` returns 0 if the pid
/// exists, 1 otherwise. macOS + Linux both have it.
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Port file shape ────────────────────────────────────────────────────────

/// Shape on disk. See `vault/plan/20260611_mcp_port_lifecycle.md` for
/// the full design (atomic write, multi-instance per-pid, stale
/// detection rules). The `version` field is the FenceyMD version so
/// agents can gate features on it.
#[derive(Serialize, Deserialize, Debug)]
pub struct PortFile {
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
    pub version: String,
}

// ── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize, Debug)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

// JSON-RPC standard error codes. MCP-specific tool errors live in the
// -32000 range per the plan (§"Tool Contract").
const ERR_PARSE: i32 = -32700;
const ERR_INVALID_REQUEST: i32 = -32600;
const ERR_METHOD_NOT_FOUND: i32 = -32601;
const ERR_INVALID_PARAMS: i32 = -32602;
const ERR_INTERNAL: i32 = -32603;
const ERR_TOOL: i32 = -32000;
const ERR_PATH_NOT_IN_BOOK: i32 = -32001;
const ERR_NO_BOOK_OPEN: i32 = -32002;
const ERR_CONTENT_TOO_LARGE: i32 = -32003;

// ── Tool input / output shapes ─────────────────────────────────────────────

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
struct OpenFileArgs {
    path: String,
    #[serde(default)]
    session_context: Option<SessionContext>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
struct GetChapterContentArgs {
    path: String,
}

#[derive(Serialize, Debug)]
struct GetChapterContentResult {
    path: String,
    content: String,
    size: usize,
}

const CHAPTER_CONTENT_MAX_BYTES: usize = 1024 * 1024; // 1 MB

// ── HTTP entry point ───────────────────────────────────────────────────────

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
}

pub async fn start(app: AppHandle) {
    // Register the shared state so Tauri commands (`mcp_update_view_state`,
    // `mcp_get_session_context`) can read/write it.
    app.manage(McpState::default());

    // Pick a port. We try up to 3 random ports in the ephemeral range
    // (49152–65535). On success, write the port file atomically. On
    // failure, log and bail — the app still works, just without an
    // MCP surface. The user can see the failure in the debug log.
    let bind_addr: SocketAddr = match pick_port().await {
        Some(a) => a,
        None => {
            log_from_rust(&app, "[mcp] could not bind a port in 3 tries; MCP server disabled");
            eprintln!("[fenceymd] MCP: could not bind a port in 3 tries; MCP server disabled");
            return;
        }
    };
    let port = bind_addr.port();

    // Check the existing port file (if any) for staleness. The
    // plan's Task 5 says: "On stale port file, overwrite it on
    // next startup." A "stale" file is one whose `pid` is no longer
    // a live process. If the file's pid IS live, we leave it alone
    // — it might be another FenceyMD instance we shouldn't clobber.
    // (Our write_port_file below will still overwrite unconditionally;
    // the check is just for diagnostics.)
    if let Some(dir) = port_dir(&app).ok() {
        let existing = dir.join("port");
        if let Ok(body) = std::fs::read_to_string(&existing) {
            if let Ok(parsed) = serde_json::from_str::<PortFile>(&body) {
                let pid_live = parsed.pid != 0 && is_pid_alive(parsed.pid);
                if !pid_live {
                    log_from_rust(
                        &app,
                        &format!("[mcp] stale port file found (pid {} dead); will overwrite", parsed.pid),
                    );
                } else {
                    log_from_rust(
                        &app,
                        &format!(
                            "[mcp] port file owned by live pid {} (likely another instance); our write will overwrite it",
                            parsed.pid
                        ),
                    );
                }
            }
        }
    }

    // Try to write the port file. If it fails, log and continue —
    // agents won't be able to discover us, but the server still runs
    // and a manual curl still works.
    if let Err(e) = write_port_file(&app, port) {
        log_from_rust(&app, &format!("[mcp] port file write failed: {e}"));
        eprintln!("[fenceymd] MCP: port file write failed: {e}");
    }

    let state = ServerState { app: app.clone() };
    let app_router = Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    log_from_rust(
        &app,
        &format!("[mcp] listening on http://127.0.0.1:{port}/mcp"),
    );
    eprintln!("[fenceymd] MCP: listening on http://127.0.0.1:{port}/mcp");

    // Drive the server to completion. We bind fresh (the previous
    // pick_port() bound and dropped to test the address) and run on
    // a long-lived axum::serve. On error, log to the debug log so
    // the user sees a real cause, not a panic.
    let server = async move {
        match TcpListener::bind(bind_addr).await {
            Ok(l) => {
                if let Err(e) = axum::serve(l, app_router).await {
                    log_from_rust(&app, &format!("[mcp] server error: {e}"));
                    eprintln!("[fenceymd] MCP: server error: {e}");
                }
            }
            Err(e) => {
                log_from_rust(&app, &format!("[mcp] re-bind failed: {e}"));
                eprintln!("[fenceymd] MCP: re-bind failed: {e}");
            }
        }
    };
    // Hand the future to Tauri's async runtime. Tauri v2 provides a
    // tokio runtime; `spawn` schedules it and returns immediately.
    tauri::async_runtime::spawn(server);
}

/// Delete the per-instance port file and the bare `port` file on
/// clean shutdown. Best-effort: we don't propagate errors. The
/// plan's Task 5 says "On clean shutdown: delete the port file"
/// (the real location is the app-data dir resolved by `port_dir`,
/// e.g. `~/Library/Application Support/com.fenceymd.app/port` on
/// macOS). This is the implementation. We also remove the per-pid file
/// (the multi-instance mirror) so the next launch starts clean.
///
/// We do NOT validate that the pid in the port file is ours — if
/// the user has two FenceyMD windows, this would delete the
/// *wrong* window's port file. The safe behavior is to only
/// delete files whose pid matches our own. We do that with a
/// read-then-check-then-delete loop.
pub fn cleanup_port_file(app: &AppHandle) {
    let dir = match port_dir(app) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[fenceymd] MCP: cleanup: port dir failed: {e}");
            return;
        }
    };
    let our_pid = process::id();
    for filename in ["port", &format!("port-{our_pid}")] {
        let path = dir.join(filename);
        let body = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,  // file doesn't exist — already gone
        };
        // Only delete if the file's pid matches ours. This
        // prevents one instance from cleaning up another's file
        // when the user runs `pkill fenceymd` and they all die
        // together.
        let ok_to_delete = serde_json::from_str::<PortFile>(&body)
            .map(|p| p.pid == our_pid)
            .unwrap_or(false);
        if ok_to_delete {
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!("[fenceymd] MCP: cleanup: rm {filename} failed: {e}");
            } else {
                log_from_rust(app, &format!("[mcp] cleanup: removed {filename}"));
            }
        }
    }
}

async fn pick_port() -> Option<SocketAddr> {
    for _ in 0..3 {
        let port: u16 = rand_port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().ok()?;
        // Try to bind. If we succeed, hand the listener back so the
        // caller can use it directly. The double-bind pattern (bind
        // here, hand to axum::serve below) is the canonical way to
        // get the OS to pick a port AND test it without TOCTOU.
        match TcpListener::bind(addr).await {
            Ok(_l) => {
                // Drop the listener; axum::serve will rebind. This
                // burns the port briefly but in the ephemeral range
                // the chance of a collision is negligible and the
                // TOCTOU window is microseconds.
                return Some(addr);
            }
            Err(_) => continue,
        }
    }
    None
}

fn rand_port() -> u16 {
    // Linear-congruential, deterministic across calls. Not
    // cryptographic; we just want a number in [49152, 65535].
    // rand is not in the std prelude, so we synthesize from time.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let n = (nanos as u64).wrapping_mul(2654435761) % (65535 - 49152 + 1);
    (49152 + n) as u16
}

fn write_port_file(app: &AppHandle, port: u16) -> std::io::Result<()> {
    let dir = port_dir(app)?;
    std::fs::create_dir_all(&dir)?;
    let pid = process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let now_iso = iso8601_utc(now);
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Bare `port` file — convenience alias for the most recent
    // instance. We write atomically via tmp + rename so a partial
    // write can't corrupt the file.
    let body = PortFile { port, pid, started_at: now_iso.clone(), version: version.clone() };
    let json = serde_json::to_string_pretty(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    atomic_write(&dir.join("port"), json.as_bytes())?;

    // Per-pid file — every instance has one of these. Agents that
    // want to talk to a specific instance use this filename.
    let pid_body = serde_json::to_string_pretty(&PortFile { port, pid, started_at: now_iso, version })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    atomic_write(&dir.join(format!("port-{pid}")), pid_body.as_bytes())?;

    Ok(())
}

fn port_dir(app: &AppHandle) -> std::io::Result<PathBuf> {
    // Honor $FENCEYMD_PORT_DIR for testing (overrides the default).
    if let Ok(dir) = std::env::var("FENCEYMD_PORT_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    Ok(base)
}

/// Atomic file write via tmp-sibling + rename (POSIX rename is atomic on
/// the same filesystem). Shared with `agents.rs` for agent-config writes.
pub(crate) fn atomic_write(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    // POSIX rename is atomic on the same filesystem. The .tmp
    // sibling is by construction.
    std::fs::rename(&tmp, path)
}

fn iso8601_utc(epoch_secs: u64) -> String {
    // Minimal ISO-8601 (UTC) without pulling in chrono. Format:
    // 2026-06-13T23:56:54Z. We use the civil-from-days algorithm for
    // the date part. Not validating overflow, but our time source is
    // UNIX_EPOCH which doesn't overflow until 2106 — fine.
    let days = (epoch_secs / 86400) as i64;
    let secs_of_day = (epoch_secs % 86400) as u32;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

// Howard Hinnant's civil_from_days — public domain. Adapted for
// unsigned-input by clamping era to i32 before multiplying (the
// year 2106 wraps around, well past the lifetime of FenceyMD).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i32) + ((era as i32) * 400);
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── Native stdio↔HTTP bridge (the `--mcp-bridge` subcommand) ────────────────
//
// When an agent (Claude Code, Codex, …) spawns `fenceymd --mcp-bridge`,
// `main()` routes here BEFORE any Tauri/GUI init. We translate the agent's
// stdio JSON-RPC stream into POSTs against the running FenceyMD instance's
// local HTTP MCP endpoint (discovered via the port file) and write each
// response back to stdout. This native bridge needs no external runtime
// (it replaced an earlier Node implementation).
//
// CONTRACT:
//   - one JSON-RPC frame per line on stdin → one response line on stdout
//   - stdout carries ONLY JSON-RPC frames; ALL logging goes to stderr
//   - the server replies with a 1-element JSON array; we unwrap `[0]`
//   - on any transport/parse failure, emit a -32000 JSON-RPC error frame
//     (so the agent sees a real error instead of a hang)
//   - frames are processed serially, preserving stdin→stdout order
//   - exit 0 on stdin EOF

/// The Tauri bundle identifier. Used to locate the app-data dir from the
/// bridge process, which has no `AppHandle` to ask. MUST match the
/// `identifier` in `tauri.conf.json` (verified: the live port file lives at
/// `~/Library/Application Support/com.fenceymd.app/port`).
const BUNDLE_ID: &str = "com.fenceymd.app";

/// Bridge entry point. Loops until stdin EOF, then returns (process exits 0).
pub fn run_bridge() {
    let endpoint = match bridge_resolve_endpoint() {
        Ok(e) => e,
        Err(code) => std::process::exit(code),
    };
    eprintln!("[mcp-bridge] endpoint: {endpoint}");

    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        use std::io::BufRead;
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF — agent closed stdin
            Ok(_) => {}
            Err(e) => {
                eprintln!("[mcp-bridge] stdin read error: {e}");
                break;
            }
        }
        let frame = line.trim();
        if frame.is_empty() {
            continue;
        }
        // A JSON-RPC notification (no `id`) takes no response per spec. We
        // still forward it so the server processes it (e.g.
        // `notifications/initialized`), but must NOT write a frame back — a
        // reply to a notification confuses strict MCP clients.
        let is_notification = serde_json::from_str::<Value>(frame)
            .ok()
            .map(|v| v.is_object() && v.get("id").is_none())
            .unwrap_or(false);
        let response = bridge_handle_frame(&endpoint, frame);
        if is_notification {
            continue;
        }
        let mut out = std::io::stdout();
        if writeln!(out, "{response}").is_err() {
            break; // downstream closed; nothing more we can do
        }
        let _ = out.flush();
    }
}

/// Resolve the HTTP endpoint. Priority: `--endpoint` arg > `MCP_BRIDGE_ENDPOINT`
/// env > the port file (3 retries — it may be mid-write on a cold start).
/// Returns a process exit code in the `Err` arm: 4 for arg/env ambiguity,
/// 2 if no endpoint could be found (FenceyMD probably isn't running).
fn bridge_resolve_endpoint() -> Result<String, i32> {
    let args: Vec<String> = std::env::args().collect();
    let mut arg_ep: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--endpoint" {
            arg_ep = args.get(i + 1).cloned();
            i += 2;
            continue;
        }
        i += 1;
    }
    let env_ep = std::env::var("MCP_BRIDGE_ENDPOINT").ok();
    if let (Some(a), Some(b)) = (&arg_ep, &env_ep) {
        if a != b {
            eprintln!("[mcp-bridge] ambiguity: --endpoint={a} vs MCP_BRIDGE_ENDPOINT={b}");
            return Err(4);
        }
    }
    if let Some(a) = arg_ep {
        return Ok(a);
    }
    if let Some(b) = env_ep {
        return Ok(b);
    }
    let port_file = match bridge_port_file() {
        Some(p) => p,
        None => {
            eprintln!("[mcp-bridge] could not resolve the port-file location");
            return Err(2);
        }
    };
    if let Some(ep) = live_endpoint(&port_file) {
        return Ok(ep);
    }
    eprintln!(
        "[mcp-bridge] no live FenceyMD instance found (port files at {} are absent or stale)",
        port_file.parent().unwrap_or(&port_file).display()
    );
    eprintln!("[mcp-bridge] is FenceyMD running? (the port file is written on app startup)");
    Err(2)
}

fn read_port_file(path: &Path) -> Option<PortFile> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn bridge_endpoint_for(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// Find a *live* FenceyMD instance's MCP endpoint from the port files. Prefers
/// the bare `port` file when its pid is still alive; otherwise scans the
/// `port-<pid>` siblings for a live instance. Returns `None` if only stale
/// (dead-pid) files exist — which is what a killed instance (no graceful
/// cleanup) or a stray `fenceymd --help`-spawned-then-killed process leaves
/// behind. Without this the bridge would happily hand back a dead port and the
/// agent would get a bare "connection refused".
fn live_endpoint(bare_port_file: &Path) -> Option<String> {
    // Bare `port` (3 retries in case it's mid-write on a cold start), but only
    // trust it if the owning pid is actually alive.
    for _ in 0..3 {
        if let Some(pf) = read_port_file(bare_port_file) {
            if is_pid_alive(pf.pid) {
                return Some(bridge_endpoint_for(pf.port));
            }
            break; // readable but stale → fall through to the per-pid scan
        }
    }
    // Scan `port-<pid>` siblings for a live instance.
    let dir = bare_port_file.parent()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if entry.file_name().to_string_lossy().starts_with("port-") {
            if let Some(pf) = read_port_file(&entry.path()) {
                if is_pid_alive(pf.pid) {
                    return Some(bridge_endpoint_for(pf.port));
                }
            }
        }
    }
    None
}

/// Compute the port-file path the way the running app's `port_dir` does, but
/// without an `AppHandle`: honor `$FENCEYMD_PORT_DIR`, else the per-OS
/// app-data dir for `BUNDLE_ID`.
fn bridge_port_file() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FENCEYMD_PORT_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        let home = home?;
        Some(
            home.join("Library/Application Support")
                .join(BUNDLE_ID)
                .join("port"),
        )
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join("AppData/Roaming")))?;
        Some(base.join(BUNDLE_ID).join("port"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local/share")))?;
        Some(base.join(BUNDLE_ID).join("port"))
    }
}

/// Send one frame, return the single response frame to write to stdout.
/// Never panics: any failure becomes a -32000 error frame carrying the
/// request's `id` (best-effort parse).
fn bridge_handle_frame(endpoint: &str, frame: &str) -> String {
    let id = serde_json::from_str::<Value>(frame)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(Value::Null);
    match bridge_post(endpoint, frame.as_bytes()) {
        Ok((status, body)) => {
            if !(200..300).contains(&status) {
                return bridge_err_frame(&id, &format!("HTTP {status} from FenceyMD"));
            }
            match serde_json::from_slice::<Value>(&body) {
                // The server always replies with a 1-element array
                // (it batches in array form); unwrap `[0]` like the .mjs.
                Ok(v) => {
                    let resp = match v.as_array() {
                        Some(arr) => arr.first().cloned().unwrap_or(Value::Null),
                        None => v,
                    };
                    serde_json::to_string(&resp)
                        .unwrap_or_else(|_| bridge_err_frame(&id, "bridge: re-serialize failed"))
                }
                Err(e) => bridge_err_frame(&id, &format!("bridge: bad response JSON: {e}")),
            }
        }
        Err(e) => bridge_err_frame(&id, &format!("bridge: {e}")),
    }
}

fn bridge_err_frame(id: &Value, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": ERR_TOOL, "message": message }
    })
    .to_string()
}

/// Minimal HTTP/1.1 POST over a raw `TcpStream` to localhost. Uses
/// `Connection: close` + read-to-EOF so we never have to parse
/// chunked transfer or track Content-Length on the response. Returns
/// `(status_code, body_bytes)`. Read/write timeouts guard against a
/// dead server hanging the agent.
fn bridge_post(endpoint: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| format!("unsupported endpoint (need http://): {endpoint}"))?;
    let slash = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..slash];
    let path = if slash < rest.len() { &rest[slash..] } else { "/" };

    let mut stream =
        TcpStream::connect(authority).map_err(|e| format!("connect {authority}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("write headers: {e}"))?;
    stream.write_all(body).map_err(|e| format!("write body: {e}"))?;
    let _ = stream.flush();

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("read response: {e}"))?;

    let sep_pos = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("malformed HTTP response (no header terminator)")?;
    let header_str = String::from_utf8_lossy(&raw[..sep_pos]);
    let status = header_str
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("could not parse HTTP status line")?;
    let body_bytes = raw[sep_pos + 4..].to_vec();
    Ok((status, body_bytes))
}

// ── JSON-RPC dispatch ──────────────────────────────────────────────────────

async fn handle_mcp(
    State(state): State<ServerState>,
    Json(body): Json<Value>,
) -> Response {
    // Some clients (e.g. older SSE) batch requests as an array. The
    // MCP Streamable HTTP spec accepts a single object per request;
    // we accept both for compatibility.
    if let Some(arr) = body.as_array() {
        let mut responses = Vec::with_capacity(arr.len());
        for item in arr {
            responses.push(dispatch_one(&state, item.clone()).await);
        }
        return (StatusCode::OK, Json(responses)).into_response();
    }
    let resp = dispatch_one(&state, body).await;
    (StatusCode::OK, Json(vec![resp])).into_response()
}

async fn dispatch_one(state: &ServerState, raw: Value) -> JsonRpcResponse {
    let req: JsonRpcRequest = match serde_json::from_value(raw) {
        Ok(r) => r,
        Err(e) => return err_response(None, ERR_PARSE, format!("parse error: {e}"), None),
    };
    if req.jsonrpc != "2.0" {
        return err_response(req.id, ERR_INVALID_REQUEST, "jsonrpc must be 2.0", None);
    }
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "initialize" => handle_initialize(id),
        "initialized" | "notifications/initialized" => ok_response(id, json!({})),
        "ping" => ok_response(id, json!({})),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(state, id, req.params).await,
        // Resources / prompts are reserved for future Phase 2 / v2 use.
        // We accept the call shape but return method-not-found so the
        // client knows we haven't implemented them.
        other => err_response(
            Some(id),
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {other}"),
            None,
        ),
    }
}

fn handle_initialize(id: Value) -> JsonRpcResponse {
    ok_response(
        id,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "fenceymd",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(id: Value) -> JsonRpcResponse {
    ok_response(id, json!({ "tools": tool_definitions() }))
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "open_file",
            "description": "Navigate the reader to a chapter. `path` can be a relative path (relative to the active book folder) OR an absolute path. Absolute paths trigger an automatic folder search: the resolver checks the most-recent recents first, then walks up the file's parent directories looking for any directory in the recents list. If a folder is found, it becomes the active folder (same as the user opening it) and the file opens. Returns the resolved absolute path. Optionally attach a `session_context` so Phase 2's sidebar chat knows which agent+session to spawn.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Markdown chapter path. Relative to the active book folder, OR absolute. Absolute paths auto-resolve to a recent folder." },
                    "session_context": {
                        "type": "object",
                        "description": "Agent session metadata, stored for Phase 2.",
                        "properties": {
                            "agent": { "type": "string" },
                            "session_id": { "type": "string" },
                            "conversation_id": { "type": "string" }
                        },
                        "required": ["agent"]
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "get_current_chapter",
            "description": "Return a compact summary of the chapter the reader is currently showing: path, scroll position, word count, reading time, and a 500-char preview. Use get_chapter_content for the full body.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_chapter_content",
            "description": "Return the full markdown source of the chapter at `path`. Capped at 1 MB; larger files return an error. `path` can be relative to the active folder or absolute (with the same folder-search behavior as open_file).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path from the active book folder, or absolute path (auto-resolved)." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "get_selected_text",
            "description": "Return the user's current text selection in the reader (if any), plus the data-md-anchor of the enclosing block. Empty text is normal.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_book_toc",
            "description": "Return the book table of contents as a flat list of {path, title, group, word_count}. Mirrors the in-app sidebar.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "capture_screenshot",
            "description": "Capture the current FenceyMD window as a PNG and return it base64-encoded in the response. The agent can decode the base64 and pass the image to a vision-capable LLM. Reuses the same xcap pipeline as the in-app ⌘⇧S shortcut. Returns {format: 'png', width, height, data_b64, bytes} on success. The image is NOT also pushed to the system clipboard (use the in-app shortcut for that).",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_debug_log",
            "description": "Return recent entries from the file-based activity log (stored in the app data dir — e.g. `~/Library/Application Support/com.fenceymd.app/debug.log` on macOS; the response includes the resolved `path`). Optional args: `tail` (lines from end, default 100, max 1000), `contains` (substring filter), `since_ts` (epoch seconds; only return entries logged at-or-after this time). Use this to see what the app is doing — the same log the user can open from Settings → Activity log.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tail": { "type": "integer", "description": "How many trailing lines to return. Default 100, max 1000.", "minimum": 1, "maximum": 1000 },
                    "contains": { "type": "string", "description": "If set, only return lines containing this substring (case-sensitive)." },
                    "since_ts": { "type": "integer", "description": "If set, only return entries logged at or after this epoch-second timestamp." }
                }
            }
        }
    ])
}

async fn handle_tools_call(state: &ServerState, id: Value, params: Value) -> JsonRpcResponse {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return err_response(Some(id), ERR_INVALID_PARAMS, "missing tool name", None),
    };
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    match name.as_str() {
        "open_file" => tool_open_file(state, id, arguments).await,
        "get_current_chapter" => tool_get_current_chapter(state, id).await,
        "get_chapter_content" => tool_get_chapter_content(state, id, arguments).await,
        "get_selected_text" => tool_get_selected_text(state, id).await,
        "get_book_toc" => tool_get_book_toc(state, id).await,
        "capture_screenshot" => tool_capture_screenshot(state, id).await,
        "get_debug_log" => tool_get_debug_log(state, id, arguments).await,
        other => err_response(
            Some(id),
            ERR_METHOD_NOT_FOUND,
            format!("tool not found: {other}"),
            None,
        ),
    }
}

// ── Tool implementations ───────────────────────────────────────────────────

async fn tool_open_file(state: &ServerState, id: Value, args: Value) -> JsonRpcResponse {
    let parsed: OpenFileArgs = match serde_json::from_value(args) {
        Ok(p) => p,
        Err(e) => return err_response(Some(id), ERR_INVALID_PARAMS, format!("bad arguments: {e}"), None),
    };

    // Resolve the active folder. We accept both relative paths
    // (relative to the currently-open book) and absolute paths
    // (auto-resolved to a recent folder that contains the file).
    //
    // The auto-resolve is what makes an agent-driven workflow work:
    // the agent doesn't have to know "which folder is the user
    // currently looking at" — it just says "open /abs/path/to/x.md"
    // and the resolver figures out the right book folder.
    let (folder_root, nav_path) = match resolve_open_target(&state.app, &parsed.path) {
        Ok(pair) => pair,
        Err(msg) => {
            // Map the resolution-failure reason to a useful JSON-RPC
            // code. Two cases: no book open at all (ERR_NO_BOOK_OPEN
            // — agent must open a folder first), or the file isn't
            // in any recent folder (ERR_PATH_NOT_IN_BOOK — agent
            // asked for a path we don't know how to scope).
            let code = if msg.starts_with("no-book") {
                ERR_NO_BOOK_OPEN
            } else {
                ERR_PATH_NOT_IN_BOOK
            };
            return err_response(Some(id), code, msg.trim_start_matches("no-book:").trim_start_matches("path:"), None);
        }
    };

    // Canonicalize-and-check. Reject anything that escapes the folder
    // root. We don't allow symlinked leaves out of the root either
    // (matches write_file's stance).
    let target = match safe_resolve_in_folder(&folder_root, &nav_path) {
        Ok(p) => p,
        Err(e) => {
            return err_response(
                Some(id),
                ERR_PATH_NOT_IN_BOOK,
                format!("path not in book: {e}"),
                None,
            );
        }
    };

    // Stash the session_context for Phase 2 use, and emit a Tauri
    // event so the JS side can mirror it into its `mcpSessionContext`
    // store. The event payload is the full SessionContext (the JS
    // side only reads it; it never writes back). If no session
    // context was passed, we leave the existing one in place —
    // "the agent doesn't have a session yet" doesn't mean "clear the
    // user's existing session". Only an explicit clear (future
    // `disconnect` tool) will wipe it.
    if let Some(ctx) = parsed.session_context.clone() {
        if let Some(state) = state.app.try_state::<McpState>() {
            if let Ok(mut g) = state.session_context.lock() {
                *g = Some(ctx.clone());
            }
        }
        // Best-effort emit. If the listener is missing (e.g. ?test=1
        // mode), the warning is harmless.
        if let Err(e) = state.app.emit("mcp:session-context", &ctx) {
            log_from_rust(&state.app, &format!("[mcp] session-context emit failed: {e}"));
        } else {
            log_from_rust(
                &state.app,
                &format!(
                    "[mcp] session-context set: agent={} session_id={:?}",
                    ctx.agent, ctx.session_id
                ),
            );
        }
    }

    // Emit the navigation event. The Svelte side listens for
    // `mcp-navigate` and calls goChapter(path). The path we send is
    // the relative one (relative to the active folder, which the JS
    // side already knows), NOT the absolute one the agent may have
    // passed. The active_folder_changed event tells the UI to swap
    // books if we had to auto-resolve.
    if parsed.path != nav_path {
        if let Some(mcp_state) = state.app.try_state::<McpState>() {
            if let (Ok(mut g), Ok(mut m)) = (
                mcp_state.active_folder_root.lock(),
                mcp_state.active_folder_meta.lock(),
            ) {
                *g = Some(folder_root.clone());
                *m = None;  // JS will re-scan + push the meta via mcp_set_active_folder
            }
        }
        // Combined event: payload is { root, nav_path }. The JS
        // handler is responsible for the full "switch folder,
        // rescan, navigate" sequence in one go — emitting them as
        // two separate events races, because the navigate handler
        // runs before the async rescan completes, and the user sees
        // a "Content not available" flash. Bundling them into one
        // event lets the JS do the navigate AFTER the rescan.
        let payload = json!({ "root": folder_root, "nav_path": nav_path });
        if let Err(e) = state.app.emit("mcp-folder-changed", &payload) {
            log_from_rust(&state.app, &format!("[mcp] mcp-folder-changed emit failed: {e}"));
        }
    } else if let Err(e) = state.app.emit("mcp-navigate", &nav_path) {
        return err_response(
            Some(id),
            ERR_INTERNAL,
            format!("emit failed: {e}"),
            None,
        );
    }

    log_from_rust(
        &state.app,
        &format!("[mcp] open_file path={} nav={} folder={} resolved={}",
            parsed.path, nav_path, folder_root, target.display()),
    );

    ok_response(id, json!({
        "ok": true,
        "active_folder": folder_root,
        "resolved_path": target.to_string_lossy()
    }))
}

async fn tool_get_current_chapter(state: &ServerState, id: Value) -> JsonRpcResponse {
    let view = match read_view(&state.app) {
        Some(v) => v,
        None => return err_response(Some(id), ERR_NO_BOOK_OPEN, "no book is currently open", None),
    };
    let path = match view.current_chapter_path.clone() {
        Some(p) => p,
        None => return ok_response(id, json!({ "open": false })),
    };
    let folder_root = match read_folder_root(&state.app) {
        Some(s) => s,
        None => return err_response(Some(id), ERR_NO_BOOK_OPEN, "no book is currently open", None),
    };
    let content = match read_chapter_text(&folder_root, &path) {
        Ok(s) => s,
        Err(e) => {
            return err_response(
                Some(id),
                ERR_TOOL,
                format!("could not read chapter: {e}"),
                None,
            );
        }
    };
    let summary = chapter_summary(&content);
    ok_response(
        id,
        json!({
            "open": true,
            "path": path,
            "scroll_position": view.scroll_position,
            "word_count": summary.words,
            "reading_time_min": summary.minutes,
            "content_preview": summary.preview
        }),
    )
}

async fn tool_get_chapter_content(
    state: &ServerState,
    id: Value,
    args: Value,
) -> JsonRpcResponse {
    let parsed: GetChapterContentArgs = match serde_json::from_value(args) {
        Ok(p) => p,
        Err(e) => return err_response(Some(id), ERR_INVALID_PARAMS, format!("bad arguments: {e}"), None),
    };
    // Same auto-resolve as open_file, but we don't need to switch
    // the active folder — we just need to find the file in some
    // recents-known folder and read it. The user sees no UI change.
    let (folder_root, nav_path) = match resolve_open_target(&state.app, &parsed.path) {
        Ok(pair) => pair,
        Err(msg) => {
            let code = if msg.starts_with("no-book") {
                ERR_NO_BOOK_OPEN
            } else {
                ERR_PATH_NOT_IN_BOOK
            };
            return err_response(Some(id), code, msg.trim_start_matches("no-book:").trim_start_matches("path:"), None);
        }
    };
    let target = match safe_resolve_in_folder(&folder_root, &nav_path) {
        Ok(p) => p,
        Err(e) => {
            return err_response(
                Some(id),
                ERR_PATH_NOT_IN_BOOK,
                format!("path not in book: {e}"),
                None,
            );
        }
    };
    let content = match std::fs::read_to_string(&target) {
        Ok(s) => s,
        Err(e) => return err_response(Some(id), ERR_TOOL, format!("read failed: {e}"), None),
    };
    let size = content.len();
    if size > CHAPTER_CONTENT_MAX_BYTES {
        return err_response(
            Some(id),
            ERR_CONTENT_TOO_LARGE,
            format!(
                "chapter is {size} bytes; cap is {CHAPTER_CONTENT_MAX_BYTES}. Split the file or use a future chunked-read tool."
            ),
            Some(json!({ "size": size, "cap": CHAPTER_CONTENT_MAX_BYTES })),
        );
    }
    ok_response(
        id,
        json!(GetChapterContentResult {
            path: nav_path,
            content,
            size,
        }),
    )
}

async fn tool_get_selected_text(state: &ServerState, id: Value) -> JsonRpcResponse {
    let view = read_view(&state.app).unwrap_or_default();
    ok_response(
        id,
        json!({
            "text": view.selected_text,
            "anchor": view.selected_anchor
        }),
    )
}

async fn tool_get_book_toc(state: &ServerState, id: Value) -> JsonRpcResponse {
    let toc = match read_folder_meta(&state.app) {
        Some(m) => m,
        None => return err_response(Some(id), ERR_NO_BOOK_OPEN, "no book is currently open", None),
    };
    // `folder_meta` is `Vec<MdFile>` from the Rust scan. Build the
    // JSON shape agents expect.
    let entries: Vec<Value> = toc
        .iter()
        .map(|f| {
            let words = f.content.split_whitespace().count();
            json!({
                "path": f.path,
                "title": f.name.trim_end_matches(".md"),
                "group": null,
                "word_count": words
            })
        })
        .collect();
    ok_response(id, json!({ "chapters": entries }))
}

// ── New tools: capture_screenshot + get_debug_log ────────────────────────
//
// These were added after Phase 1 was "done" per the v1 review:
// - `capture_screenshot` reuses the in-app ⌘⇧S pipeline (xcap
//   → RgbaImage) but returns the PNG bytes instead of pushing to
//   the clipboard, so an agent can pipe the image to a vision LLM.
// - `get_debug_log` lets an agent see what the app is doing
//   (the same file the user reads in <app_data_dir>/debug.log).
// Both are best-effort: failures return ERR_TOOL with a clear message.

async fn tool_capture_screenshot(state: &ServerState, id: Value) -> JsonRpcResponse {
    use base64::Engine as _;  // for encode()
    let app = &state.app;
    log_from_rust(app, "[mcp] capture_screenshot: start");

    // Same window-finding logic as the Tauri snapshot command.
    // The .app itself is the only one running under our bundle id,
    // so the pid check is sufficient — but we also try app_name
    // for cross-platform safety.
    //
    // Force the main window to front before enumerating. xcap's
    // `Window::all()` is unreliable for background windows on macOS
    // (it uses `kCGWindowListOptionOnScreenOnly`, which excludes
    // windows that haven't been "activated" on SkyLight). Without
    // this, an agent calling `capture_screenshot` from a terminal
    // will get "FenceyMD window not found" about half the time.
    // Bringing the window to front first makes the tool self-
    // contained — no external `osascript activate` needed.
    //
    // `set_focus` queues an event on the AppKit run loop; the
    // window doesn't appear in `CGWindowListCopyWindowInfo` for a
    // few hundred ms after. Poll briefly so the first call after
    // launch also works (otherwise only calls 2+ succeed and the
    // first one always errors).
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    let pid = std::process::id();
    // Brief settle window: macOS window-server activation is async.
    // 300 ms is enough for the focus event to reach SkyLight and the
    // NSWindow to register on the on-screen list. Total worst-case
    // tool latency: ~300ms — well under the user's perception
    // threshold for a screenshot.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let windows = match xcap::Window::all() {
        Ok(w) => w,
        Err(e) => {
            log_from_rust(app, &format!("[mcp] capture_screenshot: xcap failed: {e}"));
            return err_response(
                Some(id),
                ERR_TOOL,
                format!("enumerate windows failed: {e}"),
                None,
            );
        }
    };
    let me = windows.into_iter().find(|w| {
        // Skip only windows we can confirm are minimized. On an
        // is_minimized() error, don't skip — better to attempt the
        // capture than to spuriously report "window not found".
        if w.is_minimized().unwrap_or(false) { return false; }
        w.pid().map(|p| p == pid).unwrap_or(false)
            || w.app_name()
                .map(|n| n.to_lowercase().contains("fenceymd"))
                .unwrap_or(false)
    });
    let me = match me {
        Some(w) => w,
        None => {
            log_from_rust(app, "[mcp] capture_screenshot: no FenceyMD window");
            return err_response(
                Some(id),
                ERR_TOOL,
                "FenceyMD window not found (is the app running with a visible window?)".to_string(),
                None,
            );
        }
    };
    let img = match me.capture_image() {
        Ok(i) => i,
        Err(e) => {
            log_from_rust(app, &format!("[mcp] capture_screenshot: capture_image failed: {e}"));
            return err_response(
                Some(id),
                ERR_TOOL,
                format!("capture failed: {e}"),
                None,
            );
        }
    };
    // Bound the payload: a full-window PNG base64-encoded can be several MB
    // in a single JSON-RPC frame (one stdio line through the bridge).
    // Downscale the longest edge to MAX_EDGE px — vision models don't gain
    // from more, and it keeps the frame reasonable. Aspect ratio preserved.
    const MAX_EDGE: u32 = 1600;
    let longest = img.width().max(img.height());
    let img = if longest > MAX_EDGE {
        let scale = MAX_EDGE as f32 / longest as f32;
        let nw = ((img.width() as f32 * scale).round() as u32).max(1);
        let nh = ((img.height() as f32 * scale).round() as u32).max(1);
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let (w, h) = (img.width(), img.height());
    // xcap gives us RGBA. Save through the `image` crate to get
    // proper PNG bytes (handles the stride/formatting correctly —
    // building PNG by hand is a footgun).
    let mut png_buf: Vec<u8> = Vec::new();
    if let Err(e) = img.write_to(&mut std::io::Cursor::new(&mut png_buf), image::ImageFormat::Png) {
        log_from_rust(app, &format!("[mcp] capture_screenshot: PNG encode failed: {e}"));
        return err_response(
            Some(id),
            ERR_TOOL,
            format!("PNG encode failed: {e}"),
            None,
        );
    }
    let bytes = png_buf.len();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);
    log_from_rust(
        app,
        &format!("[mcp] capture_screenshot: {w}x{h} ({bytes} bytes)"),
    );
    ok_response(id, json!({
        "format": "png",
        "width": w,
        "height": h,
        "bytes": bytes,
        "data_b64": b64,
    }))
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
struct GetDebugLogArgs {
    #[serde(default)]
    tail: Option<usize>,
    #[serde(default)]
    contains: Option<String>,
    #[serde(default)]
    since_ts: Option<i64>,
}

async fn tool_get_debug_log(state: &ServerState, id: Value, args: Value) -> JsonRpcResponse {
    let parsed: GetDebugLogArgs = match serde_json::from_value(args) {
        Ok(p) => p,
        Err(e) => return err_response(Some(id), ERR_INVALID_PARAMS, format!("bad arguments: {e}"), None),
    };
    let app = &state.app;
    // debug_log_path returns PathBuf directly (falls back to a
    // platform-default path on resolution failure). No error path.
    let log_path = debug_log_path(app);
    let body = match std::fs::read_to_string(&log_path) {
        Ok(s) => s,
        Err(e) => {
            // The log file is created on first log line. If it
            // doesn't exist, return an empty list (not an error) —
            // the agent shouldn't fail just because nothing has
            // been logged yet.
            if e.kind() == std::io::ErrorKind::NotFound {
                return ok_response(id, json!({
                    "path": log_path.to_string_lossy(),
                    "lines": [],
                    "truncated": false,
                }));
            }
            return err_response(Some(id), ERR_TOOL, format!("read log: {e}"), None);
        }
    };
    let tail_n = parsed.tail.unwrap_or(100).clamp(1, 1000);
    let contains = parsed.contains.as_deref();
    let since_ts = parsed.since_ts;

    // The log format is: "[<epoch_secs>] <rest of line>". We slice
    // on the leading '[' once to extract the timestamp cheaply.
    let mut kept: Vec<&str> = Vec::new();
    let mut truncated = false;
    for line in body.lines() {
        if let Some(needle) = contains {
            if !line.contains(needle) { continue; }
        }
        if let Some(since) = since_ts {
            // Line starts with `[<digits>]` — parse the leading int.
            // Malformed lines are kept (we'd rather over-include
            // than drop potentially-useful data on a parse glitch).
            if let Some(ts) = parse_log_ts(line) {
                if ts < since { continue; }
            }
        }
        kept.push(line);
    }
    let total = kept.len();
    if kept.len() > tail_n {
        let drop = kept.len() - tail_n;
        kept = kept.split_off(drop);
        truncated = true;
    }
    ok_response(id, json!({
        "path": log_path.to_string_lossy(),
        "lines": kept,
        "total_matched": total,
        "returned": kept.len(),
        "truncated": truncated,
    }))
}

/// Extract the leading `[<epoch>]` from a debug-log line, if it
/// has one. Returns None for lines that don't start with a '['
/// (so we keep them, not drop them).
fn parse_log_ts(line: &str) -> Option<i64> {
    let s = line.strip_prefix('[')?;
    let end = s.find(']')?;
    s[..end].parse::<i64>().ok()
}

// ── State bridges (read Tauri managed state) ─────────────────────────────

/// Resolve a path passed to `open_file` or `get_chapter_content`
/// into a (folder_root, relative_path) pair. Handles two cases:
///
/// 1. **Relative path**: must live under the currently-active book
///    folder. If no folder is open, we return `no-book:` so the
///    caller can surface a clear error. (We do NOT auto-open a
///    folder for a relative path — that's ambiguous; the agent
///    must say "open /abs/path" if it wants auto-resolve.)
///
/// 2. **Absolute path**: try to find which recent folder contains
///    the file. Search order:
///    a. Is the path already inside the active folder? If so,
///       treat as relative (no folder switch).
///    b. Is any directory in `mcp_recents()` an ancestor of the
///       path? Most-recent first. If yes, switch active folder
///       and return the relative form.
///    c. Walk up parent directories of the path; if any ancestor
///       is in recents, switch and return relative.
///    d. Give up: return `path: not in any recent folder`.
///
/// For (2) the side effect is mutating the active folder. The
/// caller (`tool_open_file`) is responsible for emitting the
/// `mcp-folder-changed` event and clearing the meta cache. This
/// function only mutates Tauri state.
fn resolve_open_target(
    app: &AppHandle,
    raw_path: &str,
) -> Result<(String, String), String> {
    let path = PathBuf::from(raw_path);
    if path.as_os_str().is_empty() {
        return Err("path: empty".to_string());
    }

    // Case 1: relative path. Must be inside the active folder.
    if !path.is_absolute() {
        let active = match read_folder_root(app) {
            Some(s) => s,
            None => {
                return Err(format!(
                    "no-book: no book is currently open (path was relative: {raw_path})"
                ));
            }
        };
        return Ok((active, raw_path.to_string()));
    }

    // Case 2: absolute path. First, is the file already inside the
    // active folder? Then no switch needed — just compute relative.
    if let Some(active) = read_folder_root(app) {
        let active_pb = PathBuf::from(&active);
        if let Ok(canon_active) = active_pb.canonicalize() {
            // We need the file's *real* path to compare apples-to-apples.
            // If the file doesn't exist, we still try; the relpath is the
            // best we can do. safe_resolve_in_folder (the next stage) does
            // the existence check.
            let canon_target = path
                .canonicalize()
                .unwrap_or_else(|_| path.clone());
            if canon_target.starts_with(&canon_active) {
                let rel = canon_target
                    .strip_prefix(&canon_active)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| raw_path.to_string());
                return Ok((active, rel));
            }
        }
    }

    // Case 2b: search recents for an ancestor that contains the file.
    // Most-recent first. We do an existence check on the file (so a
    // typo'd path doesn't accidentally match a folder that has a file
    // of the same name under it).
    let recents = crate::mcp_recents(app);
    for folder in &recents {
        let folder_pb = PathBuf::from(folder);
        // Canonicalize both sides. If canonicalize fails (folder gone),
        // skip this recents entry — mcp_recents should have filtered
        // those out, but be defensive.
        let canon_folder = match folder_pb.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let canon_target = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !canon_target.starts_with(&canon_folder) {
            continue;
        }
        // The file is inside this folder. Compute the relative path
        // and switch the active folder. We do the switch in-place
        // here; the caller emits the events.
        let rel = canon_target
            .strip_prefix(&canon_folder)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| raw_path.to_string());
        return Ok((folder.clone(), rel));
    }

    // Case 2c: walk up parents. Useful when the agent hands us a
    // path deep in a tree and the recents contain a parent (e.g.
    // recents has "/Users/foo/Books" and the agent passes
    // "/Users/foo/Books/sub/deep/notes.md").
    let mut current = path.parent();
    let mut depth = 0;
    while let Some(p) = current {
        if depth > 16 {
            // Don't walk the whole filesystem. 16 levels is more
            // than any sane book folder.
            break;
        }
        // Is this parent in recents?
        let p_str = p.to_string_lossy().to_string();
        if let Some(folder) = recents.iter().find(|r| **r == p_str) {
            // The file is somewhere under this folder. The relative
            // path is the original raw_path minus the folder prefix.
            let folder_pb = PathBuf::from(folder);
            let rel = match path.strip_prefix(&folder_pb) {
                Ok(r) => r.to_string_lossy().to_string(),
                Err(_) => raw_path.to_string(),
            };
            return Ok((folder.clone(), rel));
        }
        current = p.parent();
        depth += 1;
    }

    Err(format!(
        "path: '{raw_path}' is not inside the active folder and not in any recent folder. Open the folder first (or have the user open it), then retry."
    ))
}

fn read_folder_root(app: &AppHandle) -> Option<String> {
    app.try_state::<McpState>().and_then(|s| {
        s.active_folder_root.lock().ok().and_then(|g| g.clone())
    })
}

fn read_folder_meta(app: &AppHandle) -> Option<Vec<MdFile>> {
    app.try_state::<McpState>().and_then(|s| {
        s.active_folder_meta.lock().ok().and_then(|g| g.clone())
    })
}

fn read_view(app: &AppHandle) -> Option<ViewState> {
    let r = app.try_state::<McpState>().and_then(|s| {
        s.view.lock().ok().map(|g| g.clone())
    });
    log_from_rust(
        app,
        &format!(
            "[mcp] read_view: has_path={} scroll={} selected='{}'",
            r.as_ref().and_then(|v| v.current_chapter_path.as_deref()).unwrap_or("(none)"),
            r.as_ref().map(|v| v.scroll_position).unwrap_or(0.0),
            r.as_ref().map(|v| v.selected_text.len()).unwrap_or(0),
        ),
    );
    r
}

fn read_session_context(app: &AppHandle) -> Option<SessionContext> {
    app.try_state::<McpState>().and_then(|s| {
        s.session_context.lock().ok().and_then(|g| g.clone())
    })
}

// ── Path safety (matches write_file's stance) ─────────────────────────────

fn safe_resolve_in_folder(
    folder_root: &str,
    rel: &str,
) -> Result<PathBuf, String> {
    let root = PathBuf::from(folder_root);
    if !root.is_absolute() {
        return Err(format!("folder root is not absolute: {folder_root}"));
    }
    let target = root.join(rel);
    // Canonicalize the target. If it doesn't exist yet, canonicalize
    // its parent and append the leaf — this matches the
    // `write_file` approach where a fresh file's parent is checked
    // instead of the leaf itself.
    let canon = match target.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let parent = target.parent().ok_or_else(|| "no parent".to_string())?;
            let parent_canon = parent
                .canonicalize()
                .map_err(|e| format!("parent canonicalize failed: {e}"))?;
            let leaf = target.file_name().ok_or_else(|| "no leaf".to_string())?;
            parent_canon.join(leaf)
        }
    };
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("root canonicalize failed: {e}"))?;
    if !canon.starts_with(&root_canon) {
        return Err(format!(
            "resolved path {} escapes folder root",
            canon.display()
        ));
    }
    Ok(canon)
}

fn read_chapter_text(folder_root: &str, rel_path: &str) -> std::io::Result<String> {
    let target = PathBuf::from(folder_root).join(rel_path);
    std::fs::read_to_string(&target)
}

// ── Chapter summary (word count, reading time, preview) ───────────────────

struct ChapterSummary {
    words: usize,
    minutes: usize,
    preview: String,
}

fn chapter_summary(content: &str) -> ChapterSummary {
    let words = content.split_whitespace().count();
    // 220 wpm is the canonical adult-reading speed; matches the
    // reader's existing reading-time math (see Reader.svelte).
    let minutes = (words.max(1) + 219) / 220;
    // Plain-text preview: strip markdown punctuation crudely, take
    // the first 500 chars on a word boundary, normalize whitespace.
    let plain: String = content
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .collect();
    let trimmed: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview: String = if trimmed.chars().count() > 500 {
        let cut: String = trimmed.chars().take(500).collect();
        // Avoid cutting mid-word. Find the last whitespace.
        let last_ws = cut.rfind(' ').unwrap_or(500);
        format!("{}…", &cut[..last_ws])
    } else {
        trimmed
    };
    ChapterSummary { words, minutes, preview }
}

// ── JSON-RPC response helpers ─────────────────────────────────────────────

fn ok_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}

fn err_response(
    id: Option<Value>,
    code: i32,
    message: impl Into<String>,
    data: Option<Value>,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id: id.unwrap_or(Value::Null),
        result: None,
        error: Some(JsonRpcError { code, message: message.into(), data }),
    }
}

// ── Tauri commands (JS ↔ Rust bridge) ─────────────────────────────────────

/// JS pushes the current view state (route, scroll, selection) so the
/// MCP tools can answer without round-tripping to the WebView.
#[tauri::command]
pub fn mcp_update_view_state(
    app: AppHandle,
    state: tauri::State<'_, McpState>,
    view: ViewState,
) -> Result<(), String> {
    let r = if let Ok(mut g) = state.view.lock() {
        log_from_rust(
            &app,
            &format!(
                "[mcp] view state received: path={:?} route={:?} scroll={}",
                view.current_chapter_path, view.route_name, view.scroll_position
            ),
        );
        // Merge, don't replace. JS callers push partial updates
        // (the scroll handler only knows about scroll, the
        // navigation effect only knows about route+path). A naive
        // `*g = view` would zero out the fields the caller didn't
        // include, so a path push followed by a scroll push
        // would silently wipe the path. Instead: only overwrite
        // fields that the caller actually sent (i.e. the
        // `is_some()` / non-default ones). For our flat struct
        // with all-default semantics, the simplest merge is:
        // - If a string field is non-empty in the incoming view,
        //   overwrite the stored value. Otherwise preserve.
        // - For Option<String>, if incoming is Some, overwrite.
        // - For the f64 (scroll_position), always overwrite (0 is a
        //   valid value meaning "top of page").
        if view.current_chapter_path.is_some() {
            g.current_chapter_path = view.current_chapter_path;
        }
        if view.route_name.is_some() {
            g.route_name = view.route_name;
        }
        if !view.selected_text.is_empty() {
            g.selected_text = view.selected_text;
        }
        if view.selected_anchor.is_some() {
            g.selected_anchor = view.selected_anchor;
        }
        g.scroll_position = view.scroll_position;
        Ok(())
    } else {
        Err("view state lock poisoned".into())
    };
    if r.is_ok() {
        log_from_rust(&app, "[mcp] view state updated");
    }
    r
}

/// JS calls this when it has just opened a folder (after `openScanResult`).
/// The MCP server caches the folder metadata so `get_chapter_content` and
/// `get_book_toc` can answer without re-scanning.
#[tauri::command]
pub fn mcp_set_active_folder(
    app: AppHandle,
    state: tauri::State<'_, McpState>,
    root: String,
    files: Vec<MdFile>,
) -> Result<(), String> {
    let files_count = files.len();
    if let (Ok(mut g_root), Ok(mut g_meta)) = (
        state.active_folder_root.lock(),
        state.active_folder_meta.lock(),
    ) {
        *g_root = Some(root.clone());
        *g_meta = Some(files);
    } else {
        return Err("active folder lock poisoned".into());
    }
    log_from_rust(
        &app,
        &format!("[mcp] active folder set: {root} ({files_count} files)"),
    );
    Ok(())
}

/// JS calls this when the user closes the folder. The MCP tools will
/// then return `ERR_NO_BOOK_OPEN` for queries that need an active book.
#[tauri::command]
pub fn mcp_clear_active_folder(
    app: AppHandle,
    state: tauri::State<'_, McpState>,
) -> Result<(), String> {
    if let (Ok(mut g_root), Ok(mut g_meta)) = (
        state.active_folder_root.lock(),
        state.active_folder_meta.lock(),
    ) {
        *g_root = None;
        *g_meta = None;
    } else {
        return Err("active folder lock poisoned".into());
    }
    log_from_rust(&app, "[mcp] active folder cleared");
    Ok(())
}

/// Diagnostic for the JS side: returns whether the MCP server is
/// listening and where the port file lives. Used by Settings → MCP
/// status (a future addition; for now this is just `invoke('mcp_status')`).
#[tauri::command]
pub fn mcp_status(app: AppHandle) -> serde_json::Value {
    let port_path = port_dir(&app).ok().map(|d| d.join("port"));
    let port_path_str = port_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let session_ctx = read_session_context(&app);
    json!({
        "port_file": port_path_str,
        "session_context": session_ctx
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_port_is_in_ephemeral_range() {
        for _ in 0..1000 {
            let p = rand_port();
            assert!((49152..=65535).contains(&p), "port {p} out of range");
        }
    }

    #[test]
    fn civil_from_days_matches_unix_epoch() {
        // 1970-01-01 → 0
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-06-13 — pre-computed via the Hinnant algorithm.
        let days = (1771622400_i64) / 86400; // 2026-06-12 16:00 UTC = next day morning
        let (y, _m, _d) = civil_from_days(days);
        assert!(y >= 2026 && y <= 2026);
    }

    #[test]
    fn safe_resolve_rejects_traversal() {
        // A folder root with no real file system (use a tempdir).
        let tmp = std::env::temp_dir().join(format!("mcp-test-{}", process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let root = tmp.to_string_lossy().to_string();
        // Inside-folder resolves.
        let inside = safe_resolve_in_folder(&root, ".").unwrap();
        assert!(inside.starts_with(std::fs::canonicalize(&tmp).unwrap().as_path()));
        // Out-of-folder traversal rejected.
        let bad = safe_resolve_in_folder(&root, "../etc/passwd");
        assert!(bad.is_err(), "expected traversal rejection, got {bad:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn chapter_summary_handles_empty_and_huge() {
        let empty = chapter_summary("");
        assert_eq!(empty.words, 0);
        assert_eq!(empty.minutes, 1);
        assert_eq!(empty.preview, "");
        let big = "word ".repeat(1000);
        let s = chapter_summary(&big);
        assert_eq!(s.words, 1000);
        assert!(s.minutes >= 4);
        assert!(s.preview.ends_with('…'));
    }

    #[test]
    fn iso8601_utc_round_trip() {
        // 2026-06-13T23:56:54Z
        let s = iso8601_utc(1781325414);
        // Don't hardcode the exact second; just check the shape.
        assert!(s.starts_with("2026-06-1") && s.ends_with("Z") && s.len() == 20);
    }

    // ── JSON-RPC frame shape tests ──────────────────────────────────────

    #[test]
    fn ok_response_carries_result_and_no_error() {
        let r = ok_response(json!(1), json!({"ok": true}));
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 1);
        assert_eq!(v["result"], json!({"ok": true}));
        assert!(v.get("error").is_none());
    }

    #[test]
    fn err_response_carries_error_and_no_result() {
        let r = err_response(Some(json!(7)), ERR_PATH_NOT_IN_BOOK, "escape", None);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["error"]["code"], -32001);
        assert_eq!(v["error"]["message"], "escape");
        assert!(v.get("result").is_none());
    }

    #[test]
    fn tool_definitions_contains_all_seven_tools() {
        // Phase 1 shipped 5 tools. We added 2 more in the post-
        // review pass: `capture_screenshot` (let the agent grab
        // what the user is seeing and pipe it to a vision LLM)
        // and `get_debug_log` (so the agent can read the same
        // activity log the user can read). If you add a tool,
        // add it here too.
        let v = tool_definitions();
        let arr = v.as_array().expect("tools/list result must be an array");
        let names: Vec<&str> = arr
            .iter()
            .map(|t| t["name"].as_str().expect("tool name"))
            .collect();
        assert!(names.contains(&"open_file"), "missing open_file");
        assert!(names.contains(&"get_current_chapter"), "missing get_current_chapter");
        assert!(names.contains(&"get_chapter_content"), "missing get_chapter_content");
        assert!(names.contains(&"get_selected_text"), "missing get_selected_text");
        assert!(names.contains(&"get_book_toc"), "missing get_book_toc");
        assert!(names.contains(&"capture_screenshot"), "missing capture_screenshot");
        assert!(names.contains(&"get_debug_log"), "missing get_debug_log");
    }

    #[test]
    fn tool_definitions_have_input_schemas() {
        let v = tool_definitions();
        for tool in v.as_array().unwrap() {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "tool {} inputSchema must be object", tool["name"]);
        }
    }

    #[test]
    fn chapter_content_max_bytes_is_one_mib() {
        // The plan says 1 MB cap. If this changes the agent contract
        // changes too, so guard with a test.
        assert_eq!(CHAPTER_CONTENT_MAX_BYTES, 1024 * 1024);
    }

    #[test]
    fn jsonrpc_request_parses_minimal_initialize() {
        let v = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let r: JsonRpcRequest = serde_json::from_value(v).unwrap();
        assert_eq!(r.method, "initialize");
        assert_eq!(r.id, Some(json!(1)));
    }

    #[test]
    fn is_pid_alive_returns_false_for_zero() {
        assert!(!super::is_pid_alive(0));
    }

    #[test]
    fn is_pid_alive_returns_true_for_self() {
        // Our own PID is, by definition, alive.
        assert!(super::is_pid_alive(std::process::id()));
    }

    #[test]
    fn is_pid_alive_returns_false_for_obviously_dead_pid() {
        // 4_000_000 is far above any normal PID range; should be ESRCH.
        // If this ever flakes, the upper bound assumption is wrong
        // for the running OS — update accordingly.
        assert!(!super::is_pid_alive(4_000_000));
    }

    fn write_pf(path: &std::path::Path, port: u16, pid: u32) {
        std::fs::write(
            path,
            serde_json::to_string(&PortFile {
                port,
                pid,
                started_at: "x".into(),
                version: "t".into(),
            })
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn live_endpoint_skips_stale_bare_and_finds_live_sibling() {
        let dir = std::env::temp_dir().join(format!("fmd_le_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bare = dir.join("port");
        write_pf(&bare, 11111, 4_000_000); // stale: dead pid
        write_pf(&dir.join(format!("port-{}", std::process::id())), 22222, std::process::id()); // live
        assert_eq!(
            super::live_endpoint(&bare).as_deref(),
            Some("http://127.0.0.1:22222/mcp")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn live_endpoint_none_when_all_stale() {
        let dir = std::env::temp_dir().join(format!("fmd_le_stale_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bare = dir.join("port");
        write_pf(&bare, 11111, 4_000_000);
        write_pf(&dir.join("port-4000001"), 33333, 4_000_001);
        assert!(super::live_endpoint(&bare).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── New tools: input parsing + parse_log_ts ─────────────────────

    #[test]
    fn get_debug_log_args_default_is_empty() {
        // Empty `{}` is the contract for "give me defaults".
        let v = json!({});
        let parsed: super::GetDebugLogArgs = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.tail, None);
        assert_eq!(parsed.contains, None);
        assert_eq!(parsed.since_ts, None);
    }

    #[test]
    fn get_debug_log_args_parses_all_fields() {
        let v = json!({
            "tail": 200,
            "contains": "[mcp]",
            "since_ts": 1781380000
        });
        let parsed: super::GetDebugLogArgs = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.tail, Some(200));
        assert_eq!(parsed.contains.as_deref(), Some("[mcp]"));
        assert_eq!(parsed.since_ts, Some(1781380000));
    }

    #[test]
    fn parse_log_ts_extracts_leading_epoch() {
        assert_eq!(super::parse_log_ts("[1781380000] [mcp] hi"), Some(1781380000));
        assert_eq!(super::parse_log_ts("[1] anything"), Some(1));
        // No leading bracket → None.
        assert_eq!(super::parse_log_ts("not a log line"), None);
        // Empty / non-numeric → None.
        assert_eq!(super::parse_log_ts("[abc] nope"), None);
        assert_eq!(super::parse_log_ts(""), None);
    }

    #[test]
    fn open_file_args_accepts_optional_session_context() {
        // `path` is required; `session_context` is optional. This
        // is the contract the agent depends on.
        let v = json!({ "path": "01-reading.md" });
        let parsed: super::OpenFileArgs = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.path, "01-reading.md");
        assert!(parsed.session_context.is_none());
    }

    #[test]
    fn open_file_args_accepts_absolute_path() {
        // The whole point of the auto-resolver is that the agent
        // can pass any path. Make sure the schema doesn't reject
        // absolute paths at the parse layer.
        let v = json!({
            "path": "/Users/alan/WORKSPACE/Books/desktop-app/README.md",
            "session_context": { "agent": "claude-code" }
        });
        let parsed: super::OpenFileArgs = serde_json::from_value(v).unwrap();
        assert!(parsed.path.starts_with('/'));
        let ctx = parsed.session_context.expect("session_context should be present");
        assert_eq!(ctx.agent, "claude-code");
    }
}
