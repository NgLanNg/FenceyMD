<script>
  // App.svelte — root component / application shell.
  //
  // Single responsibility: decide WHICH top-level view is on screen and host
  // the always-on, view-independent chrome around it. It does not own any
  // document/library data — that lives in the stores under ./lib/stores.
  //
  // Two-state shell, gated on the `ready` store:
  //   • !ready  → <Picker> (no folder open: open-folder / recents screen)
  //   • ready   → app shell: <Sidebar> + (<Reader> | <Library>),
  //               plus the singleton <Settings> and <CrossSearchPanel>.
  //
  // Collaborators:
  //   • stores.js / stores/* — folder lifecycle (openLast, openScanResult),
  //     routing (route), nav layout (navCollapsed/navOpen), the filesystem
  //     watcher (setupWatcherListener), and cross-search state.
  //   • Child views (Picker, Sidebar, Library, Reader, Settings) own their
  //     own data + focus; App only routes between them.
  //
  // Invariants / assumptions a maintainer must know:
  //   • Global keyboard shortcuts that must fire from ANY view are owned
  //     here (one listener), not duplicated per child — see onAppKey.
  //   • `isMobile` is local UI state derived from viewport width; the
  //     mobile nav drawer (navOpen) is force-closed when leaving mobile.
  import { onMount } from 'svelte';
  import { TAURI } from './lib/tauri.js';
  import {
    ready, route, navCollapsed, navOpen, reopenLast,
    openLast, openScanResult, setupWatcherListener,
  } from './lib/stores.js';
  import Picker from './components/Picker.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Library from './components/Library.svelte';
  import Reader from './components/Reader.svelte';
  import Settings from './components/Settings.svelte';
  import CrossSearchPanel from './components/CrossSearchPanel.svelte';
  // ROADMAP v1.1 #2 — cross-chapter search. The panel itself manages
  // its own focus / input; App.svelte owns the global ⌘⇧F / Ctrl+⇧F
  // shortcut so it works regardless of which view is mounted (Home,
  // Library, Reader, Settings).
  import { crossSearchOpen } from './lib/stores/state.js';

  let isMobile = $state(false);

  /**
   * Recompute `isMobile` from the current viewport width (<=768px).
   * Called on mount and on every matchMedia/resize event. Only mutates
   * state when the breakpoint actually crosses, to avoid redundant
   * reactive churn. Leaving mobile force-closes the mobile nav drawer so
   * it can't linger off-screen on the desktop layout.
   */
  function syncMobile() {
    const next = window.innerWidth <= 768;
    if (next !== isMobile) {
      isMobile = next;
      if (!isMobile) navOpen.set(false);
    }
  }

  // Boot sequence: wire viewport listeners, start the filesystem watcher,
  // then decide the initial view (test fixture / reopen-last / Home). The
  // returned function is the onMount cleanup — it tears down the listeners.
  onMount(async () => {
    syncMobile();
    // Use both matchMedia and resize so it works across browsers + emulation.
    const mq = window.matchMedia('(max-width: 768px)');
    mq.addEventListener('change', syncMobile);
    window.addEventListener('resize', syncMobile);

    await setupWatcherListener();
    // ROADMAP integration: listen for the `mcp-navigate` event from
    // the Rust MCP server. When an agent calls `open_file("path")`
    // over JSON-RPC, Rust emits this with the relative path; we
    // route to the chapter the same way the user clicking a link does.
    // Tauri-only — in browser mode the listener is a no-op.
    if (TAURI) {
      const { listen } = await import('./lib/tauri.js');
      const { goChapter, mcpSessionContext, openScanResult } = await import('./lib/stores.js');
      const unlisten = await listen('mcp-navigate', (e) => {
        const path = e?.payload;
        if (typeof path === 'string' && path.length) {
          try { goChapter(path); }
          catch (err) { console.warn('[mcp] goChapter failed:', err); }
        }
      });
      // Listen for `mcp:session-context` so the Svelte store mirrors
      // whatever the Rust side stashed. Phase 2's sidebar chat will
      // read this store to know which agent+session to spawn.
      const unlistenCtx = await listen('mcp:session-context', (e) => {
        const ctx = e?.payload;
        if (ctx && typeof ctx === 'object' && typeof ctx.agent === 'string') {
          mcpSessionContext.set(ctx);
        }
      });
      // Listen for `mcp-folder-changed` so the auto-resolver can
      // swap the user's book to a recent folder that contains an
      // absolute path the agent asked for. Without this, the user
      // would see "Content not available" in the reader — the file
      // is on disk, the active folder switched in Rust, but the JS
      // `folderMeta` store still holds the old folder's file list.
      // We re-scan the new folder, populate the stores, THEN call
      // goChapter with the navigate path. The combined event
      // payload is { root, nav_path } so we don't race the navigate
      // against the async rescan.
      const unlistenFolder = await listen('mcp-folder-changed', async (e) => {
        const p = e?.payload;
        if (!p || typeof p !== 'object') return;
        const { root, nav_path } = p;
        if (typeof root !== 'string' || !root.length) return;
        try {
          const { invoke } = await import('./lib/tauri.js');
          const { get } = await import('svelte/store');
          const { folderMeta } = await import('./lib/stores.js');
          const scan = await invoke('scan_path', { path: root });
          await openScanResult(scan);
          // openScanResult sets route to 'home'; now navigate to
          // the chapter the agent asked for. The Rust-side
          // `nav_path` is the full relative path; the renderer's
          // item.path is group-stripped (e.g. `README.md` not
          // `desktop-app/README.md`), so we have to look up the
          // matching folderMeta item by `diskPath` and use its
          // `path` for routing. Without this, the user lands on
          // the chapter's title (because the route is set) but
          // the body lookup misses and the reader shows
          // "Content not available."
          if (typeof nav_path === 'string' && nav_path.length) {
            const meta = get(folderMeta);
            const item = meta.find((f) => f.diskPath === nav_path);
            const routePath = item?.path || nav_path;
            goChapter(routePath);
          }
        } catch (err) {
          console.warn('[mcp] folder-change rescan failed:', err);
        }
      });
      // Cleanup is handled by the Svelte runtime when the component
      // unmounts; Tauri listeners are tied to the window lifetime
      // so leaking is fine here.
      void unlisten; void unlistenCtx; void unlistenFolder;
    }

    const params = new URLSearchParams(location.search);
    if (params.get('test') === '1') {
      await loadTestData();
    } else if (TAURI && $reopenLast) {
      // ROADMAP v1.1 #10 — gated on the "Reopen last folder on launch"
      // preference (default: on). First launch (nothing remembered) still
      // falls through to the Home screen. The sidebar Home button
      // returns here.
      await openLast();
    }

    return () => {
      mq.removeEventListener('change', syncMobile);
      window.removeEventListener('resize', syncMobile);
    };
  });

  // Load an in-memory sample book (no filesystem) when launched with
  // ?test=1. Each `with*` helper returns a markdown string exercising one
  // renderer feature (slides, mermaid, html/svg fences, math, code, csv,
  // excalidraw); openScanResult feeds them through the normal open path so
  // every view renders real content for screenshots / manual QA.
  async function loadTestData() {
    const sample = (t, b) => `# ${t}\n\n${b}\n\n## A subsection\n\nSome _italic_ and **bold** text, a [link](https://example.com), and code:\n\n\`\`\`js\nconst x = 42;\nconsole.log(x);\n\`\`\`\n\n- one\n- two\n- three\n`;
    const withSlides = (t, b) => `# ${t}\n\n${b}\n\n## A subsection\n\nSome _italic_ and **bold** text, a [link](https://example.com), and code:\n\n\`\`\`js\nconst x = 42;\nconsole.log(x);\n\`\`\`\n\n---\n\n## Key Takeaways\n\n- one\n- two\n- three\n\n---\n\n## What's Next\n\nThis chapter has \`---\` separators, so the slide view is available.\n\n---\n\n## Live SVG\n\nBelow is a live SVG. The reader renders the graphic itself.\n\n\`\`\`svg\n<svg viewBox=\"0 0 200 80\" xmlns=\"http://www.w3.org/2000/svg\">\n  <rect x=\"2\" y=\"2\" width=\"196\" height=\"76\" rx=\"8\" fill=\"#f0f1f0\" stroke=\"#9a9aa0\"/>\n  <circle cx=\"40\" cy=\"40\" r=\"22\" fill=\"#c25c4a\"/>\n  <text x=\"100\" y=\"46\" font-family=\"serif\" font-size=\"20\" fill=\"#242428\">live svg</text>\n</svg>\n\`\`\`\n\n---\n\n## Live HTML\n\nBelow is a live HTML block. Real DOM, not source.\n\n\`\`\`html\n<div style=\"display:flex;gap:12px;align-items:center;font-family:sans-serif\">\n  <span style=\"width:36px;height:36px;border-radius:50%;background:#c25c4a\"></span>\n  <strong>HTML block</strong>\n  <em style=\"color:#54545c\">rendered, not shown</em>\n</div>\n\`\`\`\n`;
    const withDiagram = (t) => `# ${t}\n\nA chapter with a diagram:\n\n\`\`\`mermaid\ngraph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C[Do it]\n  B -->|no| D[Skip]\n\`\`\`\n\nText after the diagram.\n`;
    const withHtml = (t, b) => `# ${t}\n\n${b}\n\nAn \`html\` fence renders as real DOM, not source — the reader doesn't care if the LLM hands you markdown or HTML.\n\n\`\`\`html\n<div style="display:flex;gap:12px;align-items:center;font-family:sans-serif">\n  <span style="width:36px;height:36px;border-radius:50%;background:#c25c4a"></span>\n  <strong>HTML block</strong>\n  <em style="color:#54545c">rendered, not shown</em>\n</div>\n\`\`\`\n\nA self-contained card example:\n\n\`\`\`html\n<div style="border:1px solid #e3e2e1;border-radius:8px;padding:16px;\n            font-family:Inter,system-ui,sans-serif;max-width:340px">\n  <div style="font-size:12px;letter-spacing:0.05em;color:#8a716e;\n              text-transform:uppercase;font-weight:600">HTML fence</div>\n  <div style="font-size:18px;font-weight:500;margin:4px 0 8px">\n    Anything a blog post can do, the chapter can do.\n  </div>\n  <div style="display:flex;gap:8px">\n    <button style="background:#83271f;color:#fff;border:0;\n                   border-radius:4px;padding:6px 12px;font:inherit">\n      Primary\n    </button>\n    <button style="background:#eeeeed;color:#1a1c1c;border:0;\n                   border-radius:4px;padding:6px 12px;font:inherit">\n      Secondary\n    </button>\n  </div>\n</div>\n\`\`\`\n`;
    const withSvg = (t, b) => `# ${t}\n\n${b}\n\nA \`svg\` fence renders as the graphic itself, not the source.\n\n\`\`\`svg\n<svg viewBox="0 0 200 80" xmlns="http://www.w3.org/2000/svg">\n  <rect x="2" y="2" width="196" height="76" rx="8" fill="#f0f1f0" stroke="#9a9aa0"/>\n  <circle cx="40" cy="40" r="22" fill="#c25c4a"/>\n  <text x="100" y="46" font-family="serif" font-size="20" fill="#242428">live svg</text>\n</svg>\n\`\`\`\n\nA composed example with three shapes:\n\n\`\`\`svg\n<svg viewBox="0 0 240 100" xmlns="http://www.w3.org/2000/svg" style="font-family:Inter,system-ui,sans-serif">\n  <line x1="20" y1="50" x2="220" y2="50" stroke="#9a9aa0" stroke-width="1.5"/>\n  <circle cx="40"  cy="50" r="6" fill="#83271f"/>\n  <circle cx="120" cy="50" r="6" fill="#83271f"/>\n  <circle cx="200" cy="50" r="6" fill="#83271f"/>\n  <text x="40"  y="78" text-anchor="middle" font-size="11" fill="#56423f">draft</text>\n  <text x="120" y="78" text-anchor="middle" font-size="11" fill="#56423f">review</text>\n  <text x="200" y="78" text-anchor="middle" font-size="11" fill="#56423f">ship</text>\n  <text x="40"  y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">1</text>\n  <text x="120" y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">2</text>\n  <text x="200" y="22" text-anchor="middle" font-size="11" font-weight="600" fill="#1a1c1c">3</text>\n</svg>\n\`\`\`\n`;
    const withMath = (t, b) => `# ${t}\n\n${b}\n\nInline math renders right in the paragraph: $E = mc^2$, $a^2 + b^2 = c^2$. And $O(n \\log n)$ is a familiar friend.\n\nA block equation stands on its own line and centers:\n\n$$\n\\int_0^1 x^2 \\, dx = \\frac{1}{3}\n$$\n\nA matrix renders cleanly too:\n\n$$\nA = \\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}, \\quad \\det(A) = ad - bc\n$$\n`;
    const withCode = (t, b) => `# ${t}\n\n${b}\n\nFenced code blocks render with **shiki** — the same highlighter VS Code uses. Theme follows the app (github-light / github-dark).\n\n## JavaScript\n\n\`\`\`js\nconst fib = (n) => (n < 2 ? n : fib(n - 1) + fib(n - 2));\nconsole.log(fib(10)); // 55\n\`\`\`\n\n## TypeScript\n\n\`\`\`ts\ntype Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };\nconst ok = <T,>(value: T): Result<T> => ({ ok: true, value });\n\`\`\`\n\n## Python\n\n\`\`\`py\nfrom dataclasses import dataclass\n\n@dataclass(frozen=True)\nclass Point:\n    x: float\n    y: float\n\n    def distance_to(self, other: "Point") -> float:\n        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5\n\`\`\`\n\n## Rust\n\n\`\`\`rust\nfn main() {\n    let nums: Vec<i32> = (1..=10).collect();\n    let sum: i32 = nums.iter().sum();\n    println!("sum = {sum}");\n}\n\`\`\`\n\n## SQL\n\n\`\`\`sql\nSELECT u.id, u.email, COUNT(o.id) AS orders\nFROM users u\nLEFT JOIN orders o ON o.user_id = u.id\nWHERE u.created_at >= NOW() - INTERVAL '30 days'\nGROUP BY u.id, u.email\nORDER BY orders DESC\nLIMIT 20;\n\`\`\`\n`;
    const withCsv = (t, b) => `# ${t}\n\n${b}\n\nInline CSV becomes a real \`<table>\` — same calm-reading treatment as the rest of the editorials. First row is the header.\n\n\`\`\`csv\nlanguage,year,paradigm\nJavaScript,1995,multi-paradigm\nPython,1991,multi-paradigm\nRust,2010,systems\nGo,2009,systems\nHaskell,1990,pure functional\n\`\`\`\n\nA second block shows the row counter:\n\n\`\`\`csv\nmetric,Q1,Q2,Q3,total\nreaders,12480,13102,14009,39591\nchapters read,42811,46392,50118,139321\n\`\`\`\n`;
    const withExcalidraw = (t) => `# ${t}\n\nA chapter with an Excalidraw scene:\n\n\`\`\`excalidraw\n{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [
    {
      "id": "rect1",
      "type": "rectangle",
      "x": 100, "y": 100, "width": 200, "height": 80,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "#a5d8ff",
      "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid",
      "roughness": 0, "opacity": 100, "groupIds": [], "frameId": null,
      "roundness": { "type": 3 }, "seed": 12345, "version": 1,
      "versionNonce": 1, "isDeleted": false, "boundElements": [],
      "updated": 1, "link": null, "locked": false
    },
    {
      "id": "text1",
      "type": "text",
      "x": 140, "y": 130, "width": 120, "height": 25,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "solid", "strokeWidth": 1, "strokeStyle": "solid",
      "roughness": 0, "opacity": 100, "groupIds": [], "frameId": null,
      "roundness": null, "seed": 23456, "version": 1, "versionNonce": 2,
      "isDeleted": false, "boundElements": [], "updated": 1, "link": null,
      "locked": false, "text": "Hello Excalidraw", "fontSize": 20,
      "fontFamily": 1, "textAlign": "center", "verticalAlign": "middle",
      "containerId": "rect1", "originalText": "Hello Excalidraw",
      "lineHeight": 1.25
    }
  ],
  "appState": { "gridSize": null, "viewBackgroundColor": "#ffffff" }
}
\`\`\`\n\nEnd of scene.\n`;
    await openScanResult({
      folder_name: 'sample-book',
      root: 'sample-book',
      files: [
        { path: 'README.md', name: 'README.md', content: `# FenceyMD — Tour Book\n\nThis folder is a tour of the app. Open it in FenceyMD and the\nsidebar will show eight chapters. Read them in order, or jump to\nthe one you need.\n\nStart with **00-welcome.md** — it walks you through the rest.\n` },
        { path: 'part-i/00-welcome.md', name: '00-welcome.md', content: sample('Welcome', 'The opening chapter of the sample book.') },
        { path: 'part-i/01-reading.md', name: '01-reading.md', content: sample('Reading', 'The reading experience — typography, fonts, dark mode.') },
        { path: 'part-i/02-navigation.md', name: '02-navigation.md', content: sample('Navigation', 'Sidebar, library, recents, keyboard shortcuts.') },
        { path: 'part-i/03-editing.md', name: '03-editing.md', content: sample('Editing', 'Inline editor, rename, bookmark.') },
        { path: 'part-i/04-html.md', name: '04-html.md', content: withHtml('Live HTML', 'The markdown-vs-HTML argument, resolved — `html` fences render as real DOM.') },
        { path: 'part-i/05-svg.md', name: '05-svg.md', content: withSvg('Live SVG', 'A `svg` fence renders as the graphic itself, not the source.') },
        { path: 'part-ii/06-slides.md', name: '06-slides.md', content: withSlides('Slide View', 'When and how to use deck mode.') },
        { path: 'part-ii/07-mermaid.md', name: '07-mermaid.md', content: withDiagram('Diagrams') },
        { path: 'part-ii/08-excalidraw.md', name: '08-excalidraw.md', content: withExcalidraw('Drawing with Excalidraw') },
        { path: 'part-iii/09-pdf.md', name: '09-pdf.md', content: sample('Exporting to PDF', 'One-click chapter PDF via headless Chrome.') },
        { path: 'part-iv/10-math.md', name: '10-math.md', content: withMath('Math', 'Katex renders `$…$` inline and `$$…$$` block, theme-neutral.') },
        { path: 'part-iv/11-code.md', name: '11-code.md', content: withCode('Code', 'Shiki highlights fenced code blocks, dual-theme.') },
        { path: 'part-iv/12-csv.md', name: '12-csv.md', content: withCsv('CSV', 'Inline CSV renders as a styled table, no interactive grid.') },
      ],
    });
  }

  // Build the shell's class list from layout state: `mobile` swaps to the
  // drawer layout, `nav-collapsed` (desktop) hides the sidebar, `nav-open`
  // (mobile) reveals the drawer. The desktop/mobile flags are mutually
  // exclusive so collapsed and open never apply together.
  function shellClass() {
    let c = 'app-shell';
    if (isMobile) c += ' mobile';
    if (!isMobile && $navCollapsed) c += ' nav-collapsed';
    if (isMobile && $navOpen) c += ' nav-open';
    return c;
  }

  // ROADMAP v1.1 #2 — ⌘⇧F (mac) / Ctrl+⇧F (others) toggles the
  // cross-chapter search panel. We mount this on App.svelte so the
  // shortcut works from any view (Picker / Library / Reader / Settings)
  // and is a single listener, not one per child.
  // Bound to <svelte:window onkeydown>; preventDefault stops the browser's
  // own ⌘⇧F so only the panel toggle fires.
  function onAppKey(e) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.shiftKey && (e.key === 'F' || e.key === 'f')) {
      e.preventDefault();
      crossSearchOpen.update((v) => !v);
    }
  }
</script>

<svelte:window onkeydown={onAppKey} />

{#if !$ready}
  <Picker />
{:else}
  <div class={shellClass()}>
    <button
      class="nav-reopen"
      onclick={() => (isMobile ? navOpen.set(true) : navCollapsed.set(false))}
      title="Show navigation"
      aria-label="Show navigation"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
    </button>

    <div class="nav-backdrop" onclick={() => navOpen.set(false)} aria-hidden="true"></div>

    <Sidebar {isMobile} />

    <main class="main-content">
      <div class="content-area-wrapper" id="content-area-wrapper">
        <div class="content-area" id="content-area">
          {#if $route.name === 'chapter'}
            <Reader path={$route.path} />
          {:else}
            <Library />
          {/if}
        </div>
      </div>
    </main>
  </div>
  <Settings />
  <CrossSearchPanel />
{/if}
