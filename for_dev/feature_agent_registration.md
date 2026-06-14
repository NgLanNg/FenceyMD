# Agent auto-registration

## Vision & DoD (5W1H)

**What.** A toggle in Settings → "AI agent control" that, when enabled, registers FenceyMD as an MCP server in the user's agent config (Claude Code, Antigravity, OpenCode, Gemini CLI, Codex). When disabled, it removes the entry. The toggle is non-destructive: every other key in the agent's config file is preserved.

**Why.** The alternative is "edit five config files by hand, one per agent, in five different formats, with no rollback if you get it wrong." That's a 30-minute setup with a high chance of breaking the user's existing config. The toggle is a one-click setup that just works.

**Who.** Any user with one of the supported agents. Power users can still edit configs by hand; the toggle is a convenience.

**When.** Whenever the user toggles the setting. The action is immediate; on the next agent launch, the agent picks up the new config.

**Where.** Settings → AI agent control. The toggle calls `agents_register(agent_id)` or `agents_unregister(agent_id)` Tauri commands. The Rust side reads the agent's config file, merges or removes the entry, writes it back.

**How (acceptance / DoD).**
- The toggle lists each supported agent with its current state ("registered" / "not registered").
- Clicking "register" inserts the right entry into the agent's config, idempotently.
- Clicking "unregister" removes only FenceyMD's entry; other config keys are untouched.
- Comments and formatting in JSON files are preserved (to the extent possible without a full comment-aware parser).
- For TOML files (Codex), the same is true.
- The user has to restart the agent for the change to take effect (agents don't hot-reload MCP configs); the toggle shows this hint.
- On Mac/Linux/Windows, the right config-file path is resolved for the user (we check `$XDG_CONFIG_HOME`, `~/.config/`, `%APPDATA%`, `~/Library/Application Support/`).

---

## How we implemented it

**What.** A Rust module (`agents.rs`) that:
1. Holds a table of 4 agent descriptors (Claude Code, Antigravity, OpenCode, Codex + Gemini CLI as a sub-descriptor of Antigravity).
2. Each descriptor knows: its config-file path (with env-var fallback), the file format (JSON or TOML), the merge key, the entry shape, the "is registered?" check.
3. `agents_detect()` returns the list of agents with their current state.
4. `agents_register(agent_id)` reads the config, merges in the FenceyMD entry, writes it back.
5. `agents_unregister(agent_id)` reads the config, removes the FenceyMD entry, writes it back.
6. `refresh_registrations()` runs at app launch: detects drift (e.g. user moved the .app, hand-edited the config) and repairs it.

**Why this shape.** Each agent's config has a different shape (JSON vs TOML, different key names, different schemas). A table-driven design lets us add a new agent by adding one descriptor — no need to touch the merge/remove code. The merge/remove core is generic and uses `serde_json` for JSON and `toml_edit` for TOML (the latter preserves comments).

**When.**
- `agents_register` / `agents_unregister` are called by the toggle handler.
- `refresh_registrations` runs on app launch, asynchronously, non-blocking.
- Drift detection: if the user moved the .app, the old config points to a non-existent path. We update it to the new path.

**Where.**
- `src-tauri/src/agents.rs` — the module (~700 lines, 18 unit tests).
- `src-tauri/src/main.rs` — `mod agents;` and the 3 Tauri commands + `refresh_registrations` call in `setup`.
- `src/lib/tauri.js` — `agentsDetect`, `agentsRegister`, `agentsUnregister` wrappers.
- `src/components/Settings.svelte` — the toggle UI.

**How (tech).**
- **Descriptors**: a static array of `AgentDescriptor` structs, each with a `name`, `id`, `configPath(home)`, `format` (json | toml), `entryKey`, `entryShape`, `isRegistered(content)` predicate.
- **JSON merge**: pure Rust `serde_json::Value` manipulation. We read the file, walk to the `entryKey` (e.g. `mcpServers`), set/remove the `fenceymd` sub-entry, write back. We don't touch other keys.
- **TOML merge**: `toml_edit 0.22` — preserves comments, formatting, sibling keys. We set/remove the `fenceymd` table.
- **Self-heal**: `refresh_registrations` walks the registered agents, checks if the binary path in the config still exists; if not, updates it to the current `.app` location. This handles the case where the user updates FenceyMD and the .app moves.
- **Atomic writes**: each config update is written to a temp file then renamed, so a partial write doesn't corrupt the user's config.
- **Idempotency**: `register` is safe to call multiple times; `unregister` is safe to call when not registered.

**Gotchas.**
- We caught 4 wrong agent-config schemas during the planning phase by reading each agent's actual docs. The Claude Code root-level `mcpServers` (user scope) lives in `~/.claude.json`, not the project-level `.claude/settings.json`. The OpenCode key is `mcp` (not `mcpServers`), the command is a single array, and the type is `local`. Gemini shares the Antigravity schema. Codex is TOML.
- The Codex path uses `toml_edit` because plain `toml` would lose comments. We tested with a real Codex config that had inline comments and sibling tables; the round-trip preserved them.
- The `refresh_registrations` runs on every app launch and could in principle race with a user editing the config. We use a short-lived lock; if a user has the file open, we skip the refresh and try again next launch.
- The "hand-edited" case: if the user manually edited the config and broke something, we don't try to fix it. The next successful register/unregister will overwrite their hand-edit.
