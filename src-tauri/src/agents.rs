//! Agent registration — wire FenceyMD's local MCP server into each AI agent's
//! own config file, one agent at a time, driven by the Settings panel.
//!
//! ## Why this module exists
//!
//! FenceyMD's MCP server (see `mcp.rs`) starts automatically and is *up* the
//! moment the app launches. But an agent only talks to MCP servers listed in
//! **its own config file**. This module writes the `fenceymd` entry into those
//! files so the user can enable agent control with a single Settings toggle —
//! no hand-editing of `~/.claude.json` / `config.toml` / etc.
//!
//! ## What gets registered
//!
//! Every agent is pointed at the **native bridge**: the app's own binary run
//! with the `--mcp-bridge` flag (`mcp::run_bridge`). That subcommand discovers
//! the running server's random port from the port file on each connection, so a
//! single static config entry survives every app restart — unlike a hardcoded
//! `http://127.0.0.1:<port>` URL, which goes stale when the port changes.
//!
//! ## The load-bearing detail: each agent's schema differs
//!
//! These shapes were verified against current agent docs; getting one wrong
//! means the toggle silently does nothing. See [`json_entry`] / [`merge_toml`]:
//!
//! | Agent | File | Container | Entry quirk |
//! |-------|------|-----------|-------------|
//! | Claude Code | `~/.claude.json` | `mcpServers` | needs `"type":"stdio"` |
//! | Gemini / Antigravity | `~/.gemini/settings.json` | `mcpServers` | **no** `type` (inferred) |
//! | OpenCode | `~/.config/opencode/opencode.json` | `mcp` | `type:"local"`, command is an **array** |
//! | Codex | `~/.codex/config.toml` | `[mcp_servers.*]` | TOML, edited via `toml_edit` |
//!
//! ## Invariants a maintainer MUST keep
//!
//! - **Non-destructive.** Register/unregister is a read-modify-write keyed by
//!   the server name `fenceymd`; every other key in the file MUST survive
//!   (critical for `~/.claude.json`, which also holds history/auth/projects).
//! - **Toggle-only.** Nothing here writes a config unless the user explicitly
//!   registers an agent. [`refresh_registrations`] only rewrites agents that are
//!   *already* registered — it never opts one in.
//! - **Pure core.** The merge/remove functions take a string and return a
//!   string (no filesystem, no `AppHandle`) so they're exhaustively unit-tested.
//!   Path resolution honors `$FENCEYMD_AGENT_HOME` for hermetic tests, mirroring
//!   `mcp.rs`'s `$FENCEYMD_PORT_DIR`.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::mcp::atomic_write;

/// The MCP server name we own in every agent's config (idempotency key).
const SERVER_KEY: &str = "fenceymd";
/// The flag the agent passes so the app binary runs as a stdio↔HTTP bridge.
const BRIDGE_FLAG: &str = "--mcp-bridge";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toml,
}

/// Static description of one supported agent. The per-agent *entry shape*
/// quirks live in [`json_entry`] (JSON agents) and [`merge_toml`] (Codex).
struct Descriptor {
    id: &'static str,
    display_name: &'static str,
    format: Format,
    /// For JSON agents: the top-level object key holding the server map.
    /// Empty for TOML (Codex always uses the `mcp_servers` table).
    json_container: &'static str,
}

const AGENTS: &[Descriptor] = &[
    Descriptor {
        id: "claude-code",
        display_name: "Claude Code",
        format: Format::Json,
        json_container: "mcpServers",
    },
    Descriptor {
        id: "gemini",
        display_name: "Gemini CLI / Antigravity",
        format: Format::Json,
        json_container: "mcpServers",
    },
    Descriptor {
        id: "opencode",
        display_name: "OpenCode",
        format: Format::Json,
        json_container: "mcp",
    },
    Descriptor {
        id: "codex",
        display_name: "Codex",
        format: Format::Toml,
        json_container: "",
    },
];

fn descriptor(id: &str) -> Option<&'static Descriptor> {
    AGENTS.iter().find(|d| d.id == id)
}

// ── Environment / path resolution ───────────────────────────────────────────

/// Resolved environment overrides, gathered once per command so the path
/// resolvers stay pure (and testable) given an explicit value.
#[derive(Default, Clone)]
struct ResolveEnv {
    /// `$FENCEYMD_AGENT_HOME` (test override) else `$HOME`.
    home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    opencode_config: Option<PathBuf>,
    codex_home: Option<PathBuf>,
}

impl ResolveEnv {
    fn from_process() -> Self {
        let home = std::env::var_os("FENCEYMD_AGENT_HOME")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from);
        ResolveEnv {
            home,
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            opencode_config: std::env::var_os("OPENCODE_CONFIG").map(PathBuf::from),
            codex_home: std::env::var_os("CODEX_HOME").map(PathBuf::from),
        }
    }
}

/// The config file we read/write for `id`. `None` if HOME is unresolvable.
fn config_path(id: &str, env: &ResolveEnv) -> Option<PathBuf> {
    let home = env.home.clone()?;
    Some(match id {
        "claude-code" => home.join(".claude.json"),
        "gemini" => home.join(".gemini").join("settings.json"),
        "opencode" => {
            if let Some(p) = &env.opencode_config {
                p.clone()
            } else if let Some(x) = &env.xdg_config_home {
                x.join("opencode").join("opencode.json")
            } else {
                home.join(".config").join("opencode").join("opencode.json")
            }
        }
        "codex" => {
            if let Some(c) = &env.codex_home {
                c.join("config.toml")
            } else {
                home.join(".codex").join("config.toml")
            }
        }
        _ => return None,
    })
}

/// Whether the agent looks installed: its config dir/file exists. Cheap and
/// reliable; PATH probing is skipped (npm-global / brew / .app installs aren't
/// consistently on the app process's PATH).
fn is_detected(id: &str, env: &ResolveEnv) -> bool {
    let home = match env.home.clone() {
        Some(h) => h,
        None => return false,
    };
    let exists = |p: PathBuf| p.exists();
    match id {
        // Claude Code may have the JSON file but not the dir (or vice-versa).
        "claude-code" => {
            config_path(id, env).map(exists).unwrap_or(false) || home.join(".claude").exists()
        }
        "gemini" => home.join(".gemini").exists(),
        "opencode" => env
            .xdg_config_home
            .as_ref()
            .map(|x| x.join("opencode"))
            .unwrap_or_else(|| home.join(".config").join("opencode"))
            .exists(),
        "codex" => env
            .codex_home
            .clone()
            .unwrap_or_else(|| home.join(".codex"))
            .exists(),
        _ => false,
    }
}

/// Absolute path of the running app binary — what agents will spawn with
/// `--mcp-bridge`. On macOS this is the inner Mach-O at
/// `FenceyMD.app/Contents/MacOS/fenceymd`.
fn current_exe_string() -> Result<String, String> {
    std::env::current_exe()
        .map_err(|e| format!("cannot resolve current executable: {e}"))
        .map(|p| p.to_string_lossy().to_string())
}

// ── Pure entry shapes + merge/remove (the unit-tested core) ──────────────────

/// The JSON entry for a JSON-config agent. Encodes the per-agent quirks.
fn json_entry(id: &str, exe: &str) -> Value {
    match id {
        // Claude Code requires an explicit transport type.
        "claude-code" => json!({ "type": "stdio", "command": exe, "args": [BRIDGE_FLAG] }),
        // Gemini infers transport from the presence of `command`; a `type`
        // field is wrong here and may be rejected.
        "gemini" => json!({ "command": exe, "args": [BRIDGE_FLAG] }),
        // OpenCode: type "local", command is a single array, explicit enable.
        "opencode" => json!({ "type": "local", "command": [exe, BRIDGE_FLAG], "enabled": true }),
        _ => json!({}),
    }
}

/// Insert `entry` at `root[container][key]`, creating `container` if absent and
/// preserving every other key. Empty input is treated as `{}`.
fn merge_json(existing: &str, container: &str, key: &str, entry: Value) -> Result<String, String> {
    let mut root: Value = if existing.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(existing).map_err(|e| format!("config is not valid JSON: {e}"))?
    };
    let obj = root
        .as_object_mut()
        .ok_or("config root is not a JSON object")?;
    let cont = obj.entry(container).or_insert_with(|| json!({}));
    let cont_obj = cont
        .as_object_mut()
        .ok_or_else(|| format!("'{container}' is not a JSON object"))?;
    cont_obj.insert(key.to_string(), entry);
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// Remove `root[container][key]` if present; leave everything else untouched.
/// Empty/absent is a no-op.
fn remove_json(existing: &str, container: &str, key: &str) -> Result<String, String> {
    if existing.trim().is_empty() {
        return Ok(existing.to_string());
    }
    let mut root: Value =
        serde_json::from_str(existing).map_err(|e| format!("config is not valid JSON: {e}"))?;
    if let Some(cont) = root.get_mut(container).and_then(|c| c.as_object_mut()) {
        cont.remove(key);
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

fn json_has_key(existing: &str, container: &str, key: &str) -> bool {
    serde_json::from_str::<Value>(existing)
        .ok()
        .and_then(|v| v.get(container).and_then(|c| c.get(key)).cloned())
        .is_some()
}

/// Insert/replace `[mcp_servers.fenceymd]` in Codex's TOML, preserving the
/// user's comments, formatting, and sibling servers (that's why this uses
/// `toml_edit` rather than a parse-and-reserialize).
fn merge_toml(existing: &str, exe: &str, args: &[&str]) -> Result<String, String> {
    use toml_edit::{value, Array, DocumentMut, Item, Table};
    let mut doc: DocumentMut = if existing.trim().is_empty() {
        DocumentMut::new()
    } else {
        existing
            .parse()
            .map_err(|e| format!("config is not valid TOML: {e}"))?
    };
    let servers = doc.entry("mcp_servers").or_insert(Item::Table(Table::new()));
    let servers_tbl = servers
        .as_table_mut()
        .ok_or("'mcp_servers' is not a TOML table")?;
    // Render sub-tables as `[mcp_servers.fenceymd]`, not an empty `[mcp_servers]`.
    servers_tbl.set_implicit(true);

    let mut entry = Table::new();
    entry.insert("command", value(exe));
    let mut arr = Array::new();
    for a in args {
        arr.push(*a);
    }
    entry.insert("args", value(arr));
    servers_tbl.insert(SERVER_KEY, Item::Table(entry));
    Ok(doc.to_string())
}

fn remove_toml(existing: &str, key: &str) -> Result<String, String> {
    use toml_edit::DocumentMut;
    if existing.trim().is_empty() {
        return Ok(existing.to_string());
    }
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|e| format!("config is not valid TOML: {e}"))?;
    if let Some(servers) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) {
        servers.remove(key);
    }
    Ok(doc.to_string())
}

fn toml_has_key(existing: &str, key: &str) -> bool {
    existing
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|doc| {
            doc.get("mcp_servers")
                .and_then(|i| i.as_table().map(|t| t.contains_key(key)))
        })
        .unwrap_or(false)
}

/// Is `fenceymd` already registered in this config's contents?
fn is_registered(d: &Descriptor, contents: &str) -> bool {
    match d.format {
        Format::Json => json_has_key(contents, d.json_container, SERVER_KEY),
        Format::Toml => toml_has_key(contents, SERVER_KEY),
    }
}

/// Does the registered `command` already point at `exe`? Used by
/// [`refresh_registrations`] to avoid rewriting a file that's already current.
fn registered_command_matches(d: &Descriptor, contents: &str, exe: &str) -> bool {
    match d.format {
        Format::Json => {
            let v: Value = match serde_json::from_str(contents) {
                Ok(v) => v,
                Err(_) => return false,
            };
            let entry = v.get(d.json_container).and_then(|c| c.get(SERVER_KEY));
            match entry.and_then(|e| e.get("command")) {
                // Claude / Gemini: command is a string.
                Some(Value::String(s)) => s == exe,
                // OpenCode: command is [exe, "--mcp-bridge"].
                Some(Value::Array(a)) => a.first().and_then(|x| x.as_str()) == Some(exe),
                _ => false,
            }
        }
        Format::Toml => contents
            .parse::<toml_edit::DocumentMut>()
            .ok()
            .and_then(|doc| {
                doc.get("mcp_servers")
                    .and_then(|i| i.as_table())
                    .and_then(|t| t.get(SERVER_KEY))
                    .and_then(|i| i.as_table())
                    .and_then(|t| t.get("command"))
                    .and_then(|i| i.as_str())
                    .map(|s| s == exe)
            })
            .unwrap_or(false),
    }
}

/// Build the registered config for an agent given the current exe path.
fn build_registered(d: &Descriptor, existing: &str, exe: &str) -> Result<String, String> {
    match d.format {
        Format::Json => merge_json(existing, d.json_container, SERVER_KEY, json_entry(d.id, exe)),
        Format::Toml => merge_toml(existing, exe, &[BRIDGE_FLAG]),
    }
}

// ── Filesystem glue ──────────────────────────────────────────────────────────

fn write_config(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
    }
    atomic_write(path, contents.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// One row in the Settings "AI agent control" list.
#[derive(serde::Serialize)]
pub struct AgentStatus {
    id: String,
    display_name: String,
    /// The agent's config dir/file exists (i.e. it's probably installed).
    detected: bool,
    /// `fenceymd` is currently present in the agent's config.
    registered: bool,
    /// Absolute path of the config file we'd write (shown in the UI tooltip).
    config_path: String,
}

/// List every supported agent with its detected/registered state. Read-only.
#[tauri::command]
pub fn agents_detect() -> Vec<AgentStatus> {
    let env = ResolveEnv::from_process();
    AGENTS
        .iter()
        .map(|d| {
            let path = config_path(d.id, &env);
            let registered = path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|contents| is_registered(d, &contents))
                .unwrap_or(false);
            AgentStatus {
                id: d.id.to_string(),
                display_name: d.display_name.to_string(),
                detected: is_detected(d.id, &env),
                registered,
                config_path: path
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// Add the `fenceymd` MCP server to `id`'s config (idempotent, non-destructive).
#[tauri::command]
pub fn agents_register(id: String) -> Result<(), String> {
    let env = ResolveEnv::from_process();
    let d = descriptor(&id).ok_or_else(|| format!("unknown agent: {id}"))?;
    let path = config_path(&id, &env).ok_or("could not resolve config path (HOME unset?)")?;
    let exe = current_exe_string()?;
    // Read immediately before write to minimize the window for a concurrent
    // rewrite by the agent itself (e.g. Claude Code persisting its own state).
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = build_registered(d, &existing, &exe)?;
    write_config(&path, &updated)
}

/// Remove the `fenceymd` MCP server from `id`'s config. No-op if absent.
#[tauri::command]
pub fn agents_unregister(id: String) -> Result<(), String> {
    let env = ResolveEnv::from_process();
    let d = descriptor(&id).ok_or_else(|| format!("unknown agent: {id}"))?;
    let path = config_path(&id, &env).ok_or("could not resolve config path (HOME unset?)")?;
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Ok(()), // no file => nothing registered
    };
    let updated = match d.format {
        Format::Json => remove_json(&existing, d.json_container, SERVER_KEY)?,
        Format::Toml => remove_toml(&existing, SERVER_KEY)?,
    };
    write_config(&path, &updated)
}

/// On launch, self-heal registrations after the app binary moved (update / drag
/// to a new path): for each agent that is **already** registered, rewrite the
/// stored command to the current exe if it changed. Never opts an agent in.
/// Best-effort; logs to stderr and continues on any per-agent failure.
pub fn refresh_registrations() {
    let exe = match current_exe_string() {
        Ok(e) => e,
        Err(_) => return,
    };
    // Never rewrite a real registration from a dev / build-tree binary:
    // `cargo tauri dev` (a debug build) or running straight out of `target/`
    // resolves `current_exe()` to a transient path that would clobber a
    // registration pointing at the installed app. Only an installed release
    // build self-heals; the user re-toggles in Settings if they need to.
    if cfg!(debug_assertions) || exe.replace('\\', "/").contains("/target/") {
        return;
    }
    let env = ResolveEnv::from_process();
    for d in AGENTS {
        let path = match config_path(d.id, &env) {
            Some(p) => p,
            None => continue,
        };
        let existing = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue, // no config file
        };
        if !is_registered(d, &existing) {
            continue; // not opted in — leave it alone
        }
        if registered_command_matches(d, &existing, &exe) {
            continue; // already current
        }
        match build_registered(d, &existing, &exe).and_then(|u| write_config(&path, &u)) {
            Ok(()) => eprintln!("[agents] refreshed {} command path → {exe}", d.id),
            Err(e) => eprintln!("[agents] refresh {} failed: {e}", d.id),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Per-agent entry shapes (guards the verified schemas) ────────────────

    #[test]
    fn claude_entry_is_type_stdio() {
        let e = json_entry("claude-code", "/x/fenceymd");
        assert_eq!(e["type"], "stdio");
        assert_eq!(e["command"], "/x/fenceymd");
        assert_eq!(e["args"], json!(["--mcp-bridge"]));
    }

    #[test]
    fn gemini_entry_has_no_type_field() {
        let e = json_entry("gemini", "/x/fenceymd");
        assert!(e.get("type").is_none(), "gemini must NOT carry a type field");
        assert_eq!(e["command"], "/x/fenceymd");
        assert_eq!(e["args"], json!(["--mcp-bridge"]));
    }

    #[test]
    fn opencode_entry_uses_array_command_and_enabled() {
        let e = json_entry("opencode", "/x/fenceymd");
        assert_eq!(e["type"], "local");
        assert_eq!(e["command"], json!(["/x/fenceymd", "--mcp-bridge"]));
        assert_eq!(e["enabled"], true);
    }

    // ── JSON merge / remove ─────────────────────────────────────────────────

    #[test]
    fn merge_json_into_empty_creates_container() {
        let out = merge_json("", "mcpServers", "fenceymd", json!({"command":"x"})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["fenceymd"]["command"], "x");
    }

    #[test]
    fn merge_json_preserves_unrelated_root_keys_and_siblings() {
        // Simulate ~/.claude.json, which also holds non-MCP state.
        let existing = r#"{
            "numStartups": 42,
            "mcpServers": { "other-server": { "command": "foo" } },
            "projects": { "/a": { "x": 1 } }
        }"#;
        let out = merge_json(
            existing,
            "mcpServers",
            "fenceymd",
            json!({"type":"stdio","command":"x"}),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["numStartups"], 42);
        assert_eq!(v["projects"]["/a"]["x"], 1);
        assert_eq!(v["mcpServers"]["other-server"]["command"], "foo");
        assert_eq!(v["mcpServers"]["fenceymd"]["command"], "x");
    }

    #[test]
    fn merge_json_is_idempotent() {
        let a = merge_json("", "mcpServers", "fenceymd", json_entry("claude-code", "/x")).unwrap();
        let b = merge_json(&a, "mcpServers", "fenceymd", json_entry("claude-code", "/x")).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn remove_json_only_removes_fenceymd() {
        let existing =
            r#"{"mcpServers":{"fenceymd":{"command":"x"},"keep":{"command":"y"}},"top":1}"#;
        let out = remove_json(existing, "mcpServers", "fenceymd").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["mcpServers"].get("fenceymd").is_none());
        assert_eq!(v["mcpServers"]["keep"]["command"], "y");
        assert_eq!(v["top"], 1);
    }

    #[test]
    fn remove_json_empty_is_noop() {
        assert_eq!(remove_json("", "mcpServers", "fenceymd").unwrap(), "");
    }

    // ── TOML merge / remove (Codex) ─────────────────────────────────────────

    #[test]
    fn merge_toml_preserves_comments_and_siblings() {
        let existing = "# my codex config\nmodel = \"gpt-5\"\n\n[mcp_servers.other]\ncommand = \"foo\"\n";
        let out = merge_toml(existing, "/x/fenceymd", &["--mcp-bridge"]).unwrap();
        assert!(out.contains("# my codex config"), "comment lost:\n{out}");
        assert!(out.contains("model = \"gpt-5\""), "sibling key lost:\n{out}");
        assert!(out.contains("[mcp_servers.other]"), "sibling server lost:\n{out}");
        assert!(out.contains("[mcp_servers.fenceymd]"), "fenceymd not added:\n{out}");
        assert!(out.contains("--mcp-bridge"));
        assert!(toml_has_key(&out, "fenceymd"));
        assert!(toml_has_key(&out, "other"));
    }

    #[test]
    fn merge_toml_is_idempotent() {
        let a = merge_toml("", "/x", &["--mcp-bridge"]).unwrap();
        let b = merge_toml(&a, "/x", &["--mcp-bridge"]).unwrap();
        assert_eq!(a, b, "second merge should be a fixed point");
    }

    #[test]
    fn remove_toml_only_removes_fenceymd_table() {
        let existing =
            "[mcp_servers.fenceymd]\ncommand = \"x\"\n\n[mcp_servers.keep]\ncommand = \"y\"\n";
        let out = remove_toml(existing, "fenceymd").unwrap();
        assert!(!toml_has_key(&out, "fenceymd"));
        assert!(toml_has_key(&out, "keep"));
    }

    // ── Path resolution (hermetic — explicit ResolveEnv) ────────────────────

    #[test]
    fn config_paths_resolve_under_home() {
        let env = ResolveEnv {
            home: Some(PathBuf::from("/h")),
            ..Default::default()
        };
        assert_eq!(
            config_path("claude-code", &env).unwrap(),
            PathBuf::from("/h/.claude.json")
        );
        assert_eq!(
            config_path("gemini", &env).unwrap(),
            PathBuf::from("/h/.gemini/settings.json")
        );
        assert_eq!(
            config_path("opencode", &env).unwrap(),
            PathBuf::from("/h/.config/opencode/opencode.json")
        );
        assert_eq!(
            config_path("codex", &env).unwrap(),
            PathBuf::from("/h/.codex/config.toml")
        );
    }

    #[test]
    fn codex_home_override_wins() {
        let env = ResolveEnv {
            home: Some(PathBuf::from("/h")),
            codex_home: Some(PathBuf::from("/custom/codex")),
            ..Default::default()
        };
        assert_eq!(
            config_path("codex", &env).unwrap(),
            PathBuf::from("/custom/codex/config.toml")
        );
    }

    #[test]
    fn opencode_config_override_wins() {
        let env = ResolveEnv {
            home: Some(PathBuf::from("/h")),
            opencode_config: Some(PathBuf::from("/custom/oc.json")),
            ..Default::default()
        };
        assert_eq!(
            config_path("opencode", &env).unwrap(),
            PathBuf::from("/custom/oc.json")
        );
    }

    // ── refresh-on-launch stale detection ───────────────────────────────────

    #[test]
    fn refresh_detects_stale_command_path() {
        let cfg =
            merge_json("", "mcpServers", "fenceymd", json_entry("claude-code", "/old/path")).unwrap();
        let d = descriptor("claude-code").unwrap();
        assert!(registered_command_matches(d, &cfg, "/old/path"));
        assert!(!registered_command_matches(d, &cfg, "/new/path"));
    }

    #[test]
    fn refresh_matches_opencode_array_command() {
        let cfg = merge_json("", "mcp", "fenceymd", json_entry("opencode", "/old")).unwrap();
        let d = descriptor("opencode").unwrap();
        assert!(registered_command_matches(d, &cfg, "/old"));
        assert!(!registered_command_matches(d, &cfg, "/new"));
    }

    #[test]
    fn is_registered_reflects_presence() {
        let claude = descriptor("claude-code").unwrap();
        let empty = "{}";
        assert!(!is_registered(claude, empty));
        let with = merge_json(empty, "mcpServers", "fenceymd", json!({"command":"x"})).unwrap();
        assert!(is_registered(claude, &with));
    }

    #[test]
    fn unknown_agent_id_has_no_descriptor() {
        assert!(descriptor("nope").is_none());
    }
}
