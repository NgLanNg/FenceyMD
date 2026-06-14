# Settings panel

## Vision & DoD (5W1H)

**What.** A modal panel (gear icon, top-right) that exposes all the user-facing preferences in one place: theme, font family, font size, content width, code theme, "reopen last folder on launch" toggle, "reset all preferences" button, the AI agent control toggles.

**Why.** Toolbar space is limited. Settings that the user changes rarely (font family, code theme, reset) belong in a modal, not a toolbar. The toggle in the toolbar is for the most-frequent action (theme); the rest live in Settings.

**Who.** Any user who wants to customize the reading experience or wire up the agent.

**When.** Click the gear icon (top-right) or press the keyboard shortcut for Settings.

**Where.** `src/components/Settings.svelte`. Mounted as an overlay; Esc closes it.

**How (acceptance / DoD).**
- The Settings panel is a modal that opens on gear click and closes on Esc.
- Every preference is labeled and has a control (toggle, dropdown, button).
- Changes apply live (the user sees the result without closing the panel).
- A "Reset all preferences" button returns everything to the defaults.
- The AI agent control section lists each supported agent with its current state and a toggle.
- The panel traps focus while open.
- The panel is keyboard-accessible (Tab cycles, Enter activates, Esc closes).

---

## How we implemented it

**What.** A Svelte 5 component that reads from the `prefs` store and writes back via the store's setters. Each control is a thin wrapper that calls the appropriate setter.

**Why this shape.** Svelte stores are the right level of indirection: a setter in the store can write to localStorage AND notify subscribers AND trigger side effects (like re-rendering mermaid diagrams on theme flip) in one place. The Settings panel is just a UI on top of the store.

**When.** Mounted on gear click or keyboard shortcut. Unmounts on Esc or backdrop click.

**Where.**
- `src/components/Settings.svelte` — the panel.
- `src/lib/stores/prefs.js` — the `prefs` store with all setters.
- `src/lib/tauri.js` — the Tauri command wrappers (for `agentsDetect` etc.).

**How (tech).**
- **Svelte 5 runes**: each setting is a `$derived` for the current value and a setter function for updates.
- **localStorage**: every preference persists to localStorage. The store hydrates from localStorage on first read.
- **Reactivity**: changing a theme preference immediately sets `data-theme` on `<html>`, triggering the CSS-variable swap.
- **Reset**: a single function that deletes the localStorage keys and re-seeds the defaults.
- **Agent control**: the panel calls `agentsDetect()` on mount to get the current state of each agent. The toggles call `agentsRegister(id)` / `agentsUnregister(id)`.

**Gotchas.**
- Some settings have side effects (theme → mermaid re-render; font family → code block re-render). The store's setter wraps the side effect; the UI just calls the setter.
- A "Reset" confirmation is shown before wiping (the user might have spent time configuring).
- The "Reopen last folder" toggle interacts with the file picker: if the user has a folder open and toggles it off, the folder is *not* closed — only the next launch is affected.
