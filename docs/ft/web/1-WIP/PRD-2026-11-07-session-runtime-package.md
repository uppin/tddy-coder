# Session runtime and terminals in `tddy-session-web` - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 16/20 — branch `feature/web-split/session-runtime`

## Affected Features

- **Primary**: [session-terminal-tabs.md](../session-terminal-tabs.md) — runtime, terminal tabs, dormant tabs, tmux.

## Summary

Move the session runtime and its terminal surfaces into `tddy-session-web`: `SessionRuntime`, `sessionRuntimeRegistry`, `sessionNotificationRegistry`, `GrpcSessionTerminal`, `SessionTerminalTabs`, `SessionMainPane`, `SessionChildRuntime`, `Dormant*`, `useSession*`, `useTerminal*`, `useTmuxSessions`, `attachClaim`.

## Background

Split out of `feature/web-split/session` (#851) on 2026-11-07. That node's green stopped on its boundary
warning (~19k lines, 150 files, 156 specs, ~40 unplanned reaches into tddy-web). The developer chose four
capability nodes: seams + helpers (#851), runtime + terminals, inspector + agents + code pane, drawer +
project home.

## Proposed Changes

### What's Changing

- Those modules and ~50 specs (`GrpcSession*`, `SessionRuntime*`, `SessionTerminal*`, `Dormant*`, `Durable*`, `FastSession*`, `ConnectedSession*`, `UseTmux*`, terminal-routed specs) move into the package.

### What's Staying the Same

- Every session behaviour. Specs keep their assertions and mount through the port doubles; specs that
  imported LiveKit testkits switch to `aPresenceDouble` / `aRoomRpcDouble` / `aMediaDouble`.

## Acceptance Criteria

- [ ] Runtime and terminal modules live in `tddy-session-web`; no tddy-web or LiveKit import.
- [ ] Their specs pass from the package.
- [ ] `SessionMainPane.tsx` does not grow (504 lines, recorded oversized).

## References

- Predecessor: `feature/web-split/session`. Package: `tddy-session-web` (created by #851).
