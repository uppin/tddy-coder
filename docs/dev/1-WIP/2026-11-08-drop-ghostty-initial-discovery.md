# Initial Discovery: Remove ghostty-web and rename the terminal components

**Changeset**: [2026-11-08-drop-ghostty.md](./2026-11-08-drop-ghostty.md)

**Date**: 2026-11-08
**Stack**: `#xterm`, stacked on `feature/web-split/wiring` (#853)
**Passes**: 2

## Combined Conclusions

- **Only the web terminal is coupled to ghostty-web.** Two files import it, both in `tddy-terminal-web`:
  `src/GhosttyTerminal.tsx` (`Terminal`, `FitAddon`) and `src/lib/ghosttyInstance.ts` (`Ghostty` WASM instance).
  In Rust, "ghostty" appears only in comments and in two e2e test names (`grpc_ghostty_virtual_terminal_e2e`,
  `livekit_ghostty_virtual_terminal_e2e`), which test the PTY over RPC, not the component.
- **Consumers:** `tddy-session-web` (`GrpcSessionTerminal`, `DormantAgentTerminal`, `SessionRuntime`,
  `useAgentTerminalFocus`), `tddy-livekit-web` (`useDirectRoomTerminal` — comment), `tddy-client-web`
  (`terminalHistoryLoader` — comment), `tddy-storybook` (stories), `tddy-web` (`StandaloneConnectionAppPage`, specs, e2e).
- **The hard parts:** (1) ghostty counts scroll positions from the bottom, xterm.js from the top; (2) ghostty
  draws one canvas, xterm.js renders DOM by default, so canvas-geometry code and specs change; (3) xterm.js
  reports mouse and alternate-screen wheel itself, so the hand SGR mouse code and the capture-phase wheel gate
  must go in the same change or input double-reports; (4) unicode widths need `@xterm/addon-unicode11`;
  (5) DOM renderer is slower on heavy TUI repaints, so production uses `@xterm/addon-webgl` with the DOM
  renderer when WebGL is unavailable (developer consented to that fallback, 2026-11-08).
- **Deleted under xterm.js:** per-terminal WASM instance, DPR watch, most hand SGR mouse code, IME Escape
  forward (verify), write buffering before WASM ready, `wasm-unsafe-eval` in Tauri CSP, the storybook render
  exclusion for the terminal story.
- **Kept:** the append-only second terminal for older history (xterm.js cannot insert at the top either),
  OSC 52 (ported to xterm's OSC handler / clipboard addon), file drop, shortcut drawer, zoom, snapshot on reconnect,
  touch drag routes (retargeted from the canvas to `.xterm-screen`).
- **Size:** `GhosttyTerminal.tsx` 1491 lines, `GhosttyTerminalSession.tsx` 1004; both carry open oversized-file
  code issues whose suggested seams are the ones this migration touches.

## Exploration 1: ghostty-web usage map — 2026-11-08

**Agent**: Explore subagent, read at `origin/feature/web-split/wiring`.
GT = `packages/tddy-terminal-web/src/GhosttyTerminal.tsx`, GTS = `.../src/GhosttyTerminalSession.tsx`.

### API surface used

- Construction (GT:322-341): `new Terminal({ ghostty, cols, rows, fontSize, fontFamily, theme, scrollback })`.
  `ghostty` (per-terminal WASM instance, `lib/ghosttyInstance.ts:37-52`) has no xterm equivalent and is not needed.
- Direct equivalents: `onData` (347), `onResize` (369), `onBell` (376), `onTitleChange` (379), `open` (388),
  `loadAddon` (414), `resize` (298, 442, 541), `write` (481, 1382), `clear` (1387), `focus` (1403), `paste` (1346),
  `dispose` (501), `options.fontSize` (296, 540, 588), `cols`/`rows`/`element` (59, 534, 782), `textarea` (991, 1215, 1392),
  `buffer.active.length/getLine().translateToString()/getCell().isInverse()` (1407-1450; xterm's `isInverse()` returns a number).
- Scroll, via `as unknown` casts: `scrollToTop`, `scrollToBottom`, `scrollLines`, `scrollToLine` (1259-1302, 350).
  ghostty's `scrollToLine`/`viewportY` count from the bottom (0 = bottom, comment at 1289); xterm counts from the top.
- Renamed in xterm: `getViewportY()` → `buffer.active.viewportY`/`baseY`; `getScrollbackLength()` → `buffer.active.baseY`;
  `onScroll(vy)` (top-based in xterm); `hasMouseTracking()` (175, 862, 1031-1082, 1159, 1323) → `term.modes.mouseTrackingMode !== 'none'`;
  private `wasmTerm.isAlternateScreen()` (866, 1328-1331) → `buffer.active.type === 'alternate'` + `onBufferChange`.
- Private internals with no equivalent (deleted): `term.renderer`, `renderer.devicePixelRatio`, `renderer.render(...)`,
  `term.wasmTerm`, `term.viewportY` (400-406).
- FitAddon (GT:413-450): `fit()`, `proposeDimensions()`, `dispose()` direct in `@xterm/addon-fit`; `observeResize()` has no
  equivalent (the component already has its own ResizeObserver, 433-457).
- DOM: `element.querySelector("canvas")` (GT:61, 70, 782, 1012) → `.xterm-screen` (only the WebGL addon draws a canvas).

### Workarounds for ghostty-web quirks

| Workaround | Where | Under xterm.js |
|---|---|---|
| Per-terminal WASM instance | `lib/ghosttyInstance.ts` (52) | Delete |
| DPR watch | `lib/terminalDprWatch.ts` (45) + GT:390-411 | Delete |
| 15px scrollbar reserve | inside ghostty FitAddon `proposeDimensions` | xterm FitAddon also subtracts scrollbar width when scrollback > 0 — re-check layouts |
| `fit()` latch bypass | GT:421-457 (RO + `proposeDimensions` + `resize`, ≤30 frames) | Simplify, keep the RO shape |
| Scrollbar coords | `lib/terminalScrollbar.ts` (24), GT:1316, GTS:351 | Rewrite top-based |
| Mouse cell coords | `lib/terminalMouseCellCoords.ts` (34), GT:775-790, 1006-1100 | Mostly delete; keep touch→wheel route |
| `hasMouseTracking` gating | GTS:608-630 wheel gate; GTS:69 `tuiOwnsNavigation`; GTS:319-327 re-poll after DEC-mode bytes | Rewrite / likely delete wheel gate; polling or `onBufferChange` |
| Append-only second terminal | GTS:305-317, 491-575; `tddy-client-web/src/lib/terminalHistoryLoader.ts` | Keep |
| OSC 52 | `lib/osc52.ts` (152), GT:265, 482, 514, 1383 | Port to `parser.registerOscHandler(52)` / `@xterm/addon-clipboard` |
| IME Escape | GT:1224-1247 | Probably delete (verify against xterm CompositionHelper) |
| Tap → synthetic mouse | GT:1128-1200 | Keep, retarget |
| Touch drag scroll (3 routes) | GT:792-990 | Keep |
| Zoom / font size | GT:280-310, 520-760; `lib/terminalZoom*.ts` | Keep; fixed-grid width fit measures the canvas (57-75, 523-555) — rewrite |
| Write buffering before ready | GTS:447-452 | Simplify (xterm is synchronous after `open`) |
| File drop, shortcut drawer, snapshot on reconnect | — | Unaffected |

### Tests

- `tddy-terminal-web` (20 Cypress specs + unit): canvas DOM/pixels in `GhosttyTerminalDpr.cy.tsx` (delete),
  `ghosttyTerminalDriver.tsx:116`, `wheelTuiSessionDriver.tsx`; WASM in `GhosttyTerminalIsolation.cy.tsx`;
  ghostty mode/wheel quirks in `terminalHistoryPagingDriver.tsx:221, 263`; 7 files read text via `getBufferText()` or the
  library-neutral `terminal-buffer-text` divs (GTS:971-986, 200ms poll GTS:476-490). Unit tests tied to deleted libs:
  `terminalDprWatch.test.ts`, `terminalMouseCellCoords.test.ts`; `terminalScrollbar.test.ts` rewritten.
- `tddy-session-web`: ~63 terminal-related specs, 9 name/import ghostty; `GhosttyTerminalShrink.cy.tsx:119-162` reads
  `canvas.style.width`; page objects click `[data-testid=ghostty-terminal]` to focus.
- `tddy-web`: 8 storybook e2e `cypress/e2e/ghostty-*.cy.ts`, `App.cy.tsx`, `playwright/concurrent-sessions.pw.ts`;
  `ghostty-selection-persistence.cy.ts:54` takes a screenshot.
- `tddy-storybook`: `storiesRender.test.tsx:16-20` excludes the ghostty story (WASM needs a browser).

### Build / infra

- `packages/tddy-terminal-web/package.json:23` `"ghostty-web": "^0.4.0"`; WASM via `ghostty-web/ghostty-vt.wasm?url`.
- `scripts/vite-prebundle.ts:33` lists `ghostty-web`.
- `packages/tddy-desktop/src-tauri/tauri.conf.json:15` CSP `'wasm-unsafe-eval'`; xterm needs `@xterm/xterm/css/xterm.css`.
- `tddy-web/package.json:20` defaults `CYPRESS_SPEC` to `ghostty-terminal.cy.ts`; Electron runs `--disable-gpu`.

### Naming spread

Exports `GhosttyTerminal`, `GhosttyTerminalHandle`, `GhosttyTerminalProps`, `GhosttyTerminalSession`,
`GhosttyTerminalSessionProps` (`src/index.ts:1-2`); testIds `ghostty-terminal`, `ghostty-terminal-dormant`,
`TEST_IDS.ghosttyTerminal` (3 files); story ids `components-ghosttyterminal--*` + `cy.visitGhosttyStory`; file names; docs.

## Exploration 2: deferred-work cross-check — 2026-11-08

| Record | Verdict |
|---|---|
| `packages/tddy-terminal-web/docs/code-issues/oversized-file-ghostty-terminal.md` | ✅ fixed by n1 terminal-seams |
| `packages/tddy-terminal-web/docs/code-issues/oversized-file-ghostty-terminal-session.md` | ✅ fixed by n1 terminal-seams |
| `docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md` | ✅ fixed by n1 terminal-seams |
| `docs/dev/todo/2026-11-07-terminal-web-does-not-typecheck.md`, `2026-11-07-terminal-package-typecheck-errors.md` | Owned by the `#web-split` sweep node; not claimed here |
| `docs/dev/todo/2026-07-28-terminal-native-scrolling-model.md` | Answered for n3: xterm.js has no prepend either; the double buffer stays |
| `docs/dev/todo/2026-07-28-terminal-lazy-scroll-up-livekit-transport-unified-surface.md` | Unrelated (unified single surface still blocked on prepend) |
| `docs/dev/todo/2026-08-29-mobile-terminal-touch-two-gaps-left-open.md` | During n3 (it rewrites the tap→SGR route); behaviour change not claimed |
| `docs/dev/todo/2026-10-25-mobile-attach-types-paths-without-paste.md` | Unrelated |
| `docs/dev/todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md` | Unrelated (daemon replay) |
| `docs/dev/todo/2026-11-07-tddy-client-web-holds-ui-and-terminal-helpers.md` | Unrelated |
