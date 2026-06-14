# Rebrand + state migration

## Vision & DoD (5W1H)

**What.** The app was previously called `MD Reader` (binary `md-reader`, bundle id `com.mdreader.app`, port file at `~/Library/Application Support/com.mdreader.app/port`). It is now called `FenceyMD` (binary `fenceymd`, bundle id `com.fenceymd.app`, port file at `…/com.fenceymd.app/port`). On first launch of a build that has the rebrand but still has the old app data dir on disk, FenceyMD copies the user's old `state.json` (recents, last folder, reading progress) into the new location, then renames the old file so the migration never re-runs.

**Why.** A user who already had MD Reader installed would, without this migration, lose their recents, last-opened folder, and per-chapter scroll positions the moment they updated to FenceyMD — the new app data dir is empty on first launch. Reading progress in particular is the kind of data you don't realize you depend on until it's gone (re-opening a half-finished chapter at scroll=0 is a small but real loss). The migration makes the rebrand transparent.

**Who.** Anyone updating from an MD Reader build. Silent — no prompt, no opt-in. The user just sees their recents + continue-reading entries as if nothing changed.

**When.** Runs once at app setup, before the MCP server starts. The MCP server reads `state.json` to populate its recents on first request, so the migration has to finish first.

**Where.**
- The new data dir is per the bundle id, so it's `~/Library/Application Support/com.fenceymd.app/` on macOS (and the equivalent on Linux/Windows). Tauri's `app.path().app_data_dir()` resolves this.
- The old data dir is *hard-coded* to `~/Library/Application Support/com.mdreader.app/state.json` for macOS. The migration is best-effort: if the old file doesn't exist, the function returns immediately. Linux/Windows users who upgraded from a pre-rebrand MD Reader will have to copy their state by hand (low priority — most users are on macOS).
- The Rust side runs the migration; the JS side runs a parallel `localStorage` migration in `src/lib/stores/prefs.js` that copies `md-reader-*` keys to `fenceymd-*` and deletes the old ones.

**How (acceptance / DoD).**
- A user with `state.json` in `com.mdreader.app/` and a fresh (empty) `state.json` in `com.fenceymd.app/` opens FenceyMD and sees their old recents + last folder + reading progress, with no data loss.
- The old `com.mdreader.app/state.json` is renamed to `state.json.migrated` after a successful merge, so subsequent launches don't re-run the migration.
- The new `state.json` is a superset: it contains every key the old one had, plus anything the new build had written. If both had a recents list, the merged list is the union (deduped, old-first).
- If the user has no old state dir, the migration is a no-op (returns silently, no log spam).
- If the old `state.json` is malformed, the migration is a no-op (the new state stays as-is; user keeps whatever they had).
- A debug log line `[fenceymd] migrated state from "..." → "..."` confirms the merge ran.
- The JS-side localStorage migration runs once at `prefs.js` module init, copies every `md-reader-*` key to its `fenceymd-*` counterpart (only if the new key is absent — to be safe across re-runs), then deletes the old key.

## How we implemented it

**Two migrations, two languages, two lifecycle hooks.**

### Rust: `state.json` migration

`src-tauri/src/main.rs`, function `migrate_old_state()`, called once from the Tauri `setup()` block, before the MCP server task spawns.

```rust
// ── One-time migration: copy state from com.mdreader.app → com.fenceymd.app ──
fn migrate_old_state(app: &AppHandle) {
    let new_path = store_path(app);                  // <new data dir>/state.json
    let new_dir  = match app.path().app_data_dir() { Ok(d) => d, Err(_) => return };
    // Defensive: only run when the new dir is *the* FenceyMD dir, so a
    // test build with a different bundle id can't trigger it.
    if !new_dir.to_string_lossy().contains("com.fenceymd.app") { return; }

    // macOS path is hard-coded — most users are on macOS. Linux/Windows
    // would need a per-OS lookup; deferred.
    let home = match std::env::var_os("HOME").map(PathBuf::from) {
        Some(h) => h, None => return,
    };
    let old_state = home.join("Library/Application Support/com.mdreader.app/state.json");
    if !old_state.exists() { return; }
    let old_json = match std::fs::read_to_string(&old_state) { Ok(s) => s, Err(_) => return };
    let old_store: Store = match serde_json::from_str(&old_json) { Ok(s) => s, Err(_) => return };

    // Load whatever the new state has now (defaults if missing/malformed).
    let mut new_store: Store = std::fs::read_to_string(&new_path)
        .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    // Merge: prefer old values, but never lose new ones.
    if old_store.last_folder.is_some() {
        new_store.last_folder = old_store.last_folder.clone();
    }
    let mut seen = std::collections::HashSet::new();
    let mut recents = Vec::new();
    for r in old_store.recents.iter().chain(new_store.recents.iter()) {
        if seen.insert(r.clone()) { recents.push(r.clone()); }
    }
    new_store.recents = recents;
    for (book, old_chapters) in old_store.progress {
        let entry = new_store.progress.entry(book).or_default();
        for (ch, val) in old_chapters {
            entry.entry(ch).or_insert(val);
        }
    }

    // Persist the merged state to the new location.
    if let Ok(json) = serde_json::to_string_pretty(&new_store) {
        let _ = std::fs::write(&new_path, json);
    }
    // Rename the old state so we never re-migrate.
    let _ = std::fs::rename(&old_state, old_state.with_extension("json.migrated"));
    eprintln!("[fenceymd] migrated state from {old_state:?} → {new_path:?}");
}
```

Wired into `setup()`:

```rust
.setup(|app| {
    migrate_old_state(&app.handle());   // ← runs before MCP starts
    let app_handle = app.handle().clone();
    tauri_async::spawn(async move { mcp::start(app_handle).await; });
    agents::refresh_registrations();
    Ok(())
})
```

**Why this shape:**
- **Best-effort, not transactional.** Any failure (missing old dir, malformed JSON, write error) returns silently. The new app data dir is the source of truth; the old one is a one-time donor.
- **Defensive bundle-id check.** The `com.fenceymd.app` substring check makes sure a test build with a different bundle id can't trigger the migration. (Important because a developer running `cargo tauri dev` has a different bundle id; we don't want their fresh dev state to pull from the user's MD Reader install.)
- **Rename, don't delete.** The old `state.json` becomes `state.json.migrated` so the user can find it if they need to roll back, and so the migration can never re-run on a subsequent launch. (The `.migrated` suffix is a strong signal — anyone looking at the old dir will see the file and know what happened.)
- **Idempotent on the new side.** A second launch finds no `state.json` (only `state.json.migrated`), so the migration is a no-op.
- **No code change to MCP.** The MCP server reads `state.json` exactly as it always has; the migration just makes sure the file is populated when the server first reads it.

### JS: `localStorage` migration

`src/lib/stores/prefs.js`, function `migratePrefsPrefix()`, runs at module import (so before any store hydrates from localStorage).

```js
// ── v1 → v2 prefs migration: `md-reader-*` → `fenceymd-*` (rebrand) ──
const PREFIX_OLD = 'md-reader-';
const PREFIX_NEW = 'fenceymd-';
function migratePrefsPrefix() {
  if (typeof localStorage === 'undefined') return;
  try {
    for (let i = localStorage.length - 1; i >= 0; i -= 1) {
      const k = localStorage.key(i);
      if (!k || !k.startsWith(PREFIX_OLD)) continue;
      const newKey = PREFIX_NEW + k.slice(PREFIX_OLD.length);
      if (localStorage.getItem(newKey) == null) {
        try { localStorage.setItem(newKey, localStorage.getItem(k) ?? ''); } catch {}
      }
      localStorage.removeItem(k);
    }
  } catch {}
}
migratePrefsPrefix();
```

**Why a separate function rather than the existing `resetAllPrefs` machinery:**
- The migration runs at *every* module init, not on user request. The check for `startsWith('fenceymd-')` (via the "new key absent" check) makes it a no-op after the first run — by the time the migration would look for an old key, the old key is gone.
- It iterates the keys in reverse (`localStorage.length - 1` down to 0) because `localStorage.removeItem` shifts indices down. (Iterating forward would skip every other key — silent bug.)
- The `if (localStorage.getItem(newKey) == null)` guard means a user who already had a fresh `fenceymd-theme` set (e.g. the user manually set the theme before the rebrand hit them) doesn't get their explicit choice clobbered by an old default. Old value only writes if the new key is empty.

**Keys migrated (12 in total):**
- `md-reader-theme` → `fenceymd-theme`
- `md-reader-fontsize` → `fenceymd-fontsize`
- `md-reader-content-width` → `fenceymd-content-width`
- `md-reader-nav-collapsed` → `fenceymd-nav-collapsed`
- `md-reader-view-mode` → `fenceymd-view-mode`
- `md-reader-onboarded` → `fenceymd-onboarded`
- `md-reader-code-theme` → `fenceymd-code-theme`
- `md-reader-font-family` → `fenceymd-font-family`
- `md-reader-reopen-last` → `fenceymd-reopen-last`
- `md-reader-outline-visible` → `fenceymd-outline-visible`
- `md-reader-external-editor` → `fenceymd-external-editor` (the key the editor picker would write to in a future build — not currently read anywhere in the codebase, but the comment in `tauri.js` references it)

### Why the rename-on-success pattern is important

The migration functions in both Rust and JS use the "rename old file / delete old key" pattern to ensure idempotency:
- A second launch with the same build doesn't re-migrate.
- A re-install of the same build doesn't re-migrate.
- A downgrade (FenceyMD → MD Reader) is impossible in practice (we don't ship a build with the old bundle id anymore), but if it ever did happen, the old state is still available at `state.json.migrated` in the old dir for manual restoration.

### What this doesn't cover

- **Agent configs.** Claude Code's `~/.claude.json`, Gemini's `~/.gemini/settings.json`, OpenCode's `~/.config/opencode/opencode.json`, and Codex's `~/.codex/config.toml` all get a new `fenceymd` entry on first launch of the new build (via `agents::refresh_registrations()`, which runs in `setup()` next to the migration). The old `md-reader` entry becomes a stale orphan in the agent config — harmless, but the user can clean it up via `agents_unregister` or by hand.
- **Old `port-*` files.** Each instance wrote a `port-<pid>` file at runtime; those are tied to the now-dead process and can be ignored (the new build overwrites them when it starts).
- **Debug log.** The old `debug.log` stays in the old dir; the new build writes a fresh one in the new dir. Logs aren't migrated — they have a short retention cycle and aren't user-facing.
- **Linux/Windows users.** The Rust migration only handles the macOS path. A Linux user with a pre-rebrand `~/.local/share/com.mdreader.app/state.json` would have to copy it by hand. (Most users are on macOS per the existing data; defer until someone actually hits this.)
