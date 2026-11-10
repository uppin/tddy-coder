# 2026-11-08 — The terminal components split along their input, scroll and history seams (#xterm 1/4, PR #866)

**Type:** Refactor

`tddy-terminal-web`'s `GhosttyTerminal.tsx` (1491 production lines) and `GhosttyTerminalSession.tsx`
(1004) become 199 and 486 lines, composed from hooks under `src/terminal/` and `src/session/`, with no
behaviour change, so the engine swap that follows works on small, separately testable units.
`src/componentSize.test.ts` holds every production file in the package to 500 lines.

Closed: the two `oversized-file-ghostty-terminal*` code issues and the backlog entry
`2026-10-24-ghostty-terminal-components-over-budget`. See
[tddy-terminal-web](../../../packages/tddy-terminal-web/docs/changesets/2026-11-08-terminal-component-seams.md).
