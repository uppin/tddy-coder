# 2026-10-24 — `GhosttyTerminal.tsx` and `GhosttyTerminalSession.tsx` are over the 500-line budget

**Category:** Deferred from Claude fullscreen renderer (`claude-cli-fullscreen`)
**Source:** `packages/tddy-web/src/components/GhosttyTerminal.tsx` (1,354 lines),
`packages/tddy-web/src/components/GhosttyTerminalSession.tsx` (971 lines)

## What happens

Both components were already far over the 500-line budget at base (1,351 and 947). The
`claude-cli-fullscreen` branch grew them by +3 and +24 lines: the mouse-listener effect stopped
re-attaching on every render, and the `tddy:term:modes` / `tddy:term:chrome` traces were added to
diagnose why Claude could not be wheel-scrolled.

## Why deferred

The developer consented to deferral at `/pr-wrap`. The growth is diagnostic tracing for a daemon-side
fix (the fullscreen renderer), and a hand split of two ~1k-line React components is a separate,
behaviour-preserving change with its own Cypress baseline.

## What would close it

The two `oversized-file-ghostty-terminal*` records in `packages/tddy-web/docs/code-issues/` name the
seams. Split each by hand: the green baseline of the terminal component specs before, mechanical moves
into hooks only, and the same green after.
