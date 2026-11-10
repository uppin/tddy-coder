# 2026-11-08 — Web terminals render with xterm.js (#xterm 3/4, PR #868)

**Type:** Feature

- `src/engine/xtermEngine.ts` implements `TerminalEngine` on `@xterm/xterm` with `@xterm/addon-fit`,
  `@xterm/addon-unicode11` (Unicode 11 widths) and `@xterm/addon-webgl`. The WebGL renderer is
  used where the browser offers a context; otherwise, and after a WebGL context is lost for good,
  xterm's DOM renderer draws. `data-terminal-renderer` names the active one.
  `createXtermEngine(options, createWebgl)` takes the WebGL addon's factory. Options left out keep
  xterm's defaults.
- `useTerminalLifecycle` constructs `xtermEngine`. `ghosttyEngine` stays, constructed by nothing.
  The `getBufferText` memo revision moves on `onWriteParsed`, and `onReady` waits until
  `initialContent` is parsed. The handle gains `onWriteParsed` and loses `sendWheelSgr`.
- Deleted, because xterm.js does it itself: `useTerminalMouseForwarding` (hand SGR mouse, wheel and
  touch press/release) and `useLiveWheelGate` (the capture-phase wheel gate). `useTapAsClick` no
  longer samples mouse tracking, since xterm reports the synthesised click. A trackpad pinch stops
  propagation, so it never reaches xterm's wheel handling.
- Kept: the OSC 52 write tap (no `@xterm/addon-clipboard`), the IME Escape forward,
  `lib/terminalDprWatch.ts` (used by `ghosttyEngine`) and `useFeedStream` buffering.
- `useHistoryDoubleBuffer` re-reads the live pane's DEC modes on parse, and disposes that watch on
  unmount.
- Geometry measures `.xterm-screen` (`measureTerminalScreenCssWidth` / `…Height`). The canvas specs
  are rewritten: the DPR spec is deleted and `GhosttyTerminalIsolation` is kept as a behavioural
  guarantee. New specs: `TerminalMouseReporting`, `TerminalUnicodeWidth`, `TerminalRenderer`. The
  contract suite runs against both engines.
- `scripts/vite-prebundle.ts` pre-bundles the four `@xterm/*` packages.
- Backlog: `2026-08-29-mobile-terminal-touch-two-gaps-left-open` is narrowed (a drag no longer
  reports a stray press), and `2026-07-28-terminal-native-scrolling-model` records that xterm.js
  has no prepend API either.
- Docs: [xterm-engine.md](../xterm-engine.md).
