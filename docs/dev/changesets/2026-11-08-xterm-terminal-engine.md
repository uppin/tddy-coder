# 2026-11-08 — Web terminals render with xterm.js (#xterm 3/4, PR #868)

**Type:** Feature

Every web terminal constructs `xtermEngine`, which runs on xterm.js with a WebGL renderer, falls
back to the DOM renderer, and uses Unicode 11 widths. xterm.js reports the mouse and wheel to a
mouse-tracking TUI, so the hand-written SGR forwarding and the capture-phase wheel gate are deleted,
and each click or wheel step is reported once. `ghostty-web` and `ghosttyEngine` remain, unused by
the components. `tddy-session-web` and `tddy-web` specs measure `.xterm-screen` instead of a canvas.
`scripts/vite-prebundle.ts` lists the `@xterm/*` packages. See
[tddy-terminal-web](../../../packages/tddy-terminal-web/docs/changesets/2026-11-08-xterm-terminal-engine.md)
and [tddy-session-web](../../../packages/tddy-session-web/docs/changesets/2026-11-08-xterm-terminal-engine.md).
