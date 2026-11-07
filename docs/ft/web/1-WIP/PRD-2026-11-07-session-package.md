# Session package (`tddy-session-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 14/16 — branch `feature/web-split/session`

## Affected Features

- **Primary**: [session-drawer.md](../session-drawer.md), [session-terminal-tabs.md](../session-terminal-tabs.md), [session-files-inspector.md](../session-files-inspector.md), [session-usage-inspector.md](../session-usage-inspector.md), [session-code-pane.md](../session-code-pane.md), [url-state-routing.md](../url-state-routing.md) (session state).

## Summary

Move what remains of sessions — the sessions drawer and its groups/project-home entries, the session runtime and runtime registry, the main pane and terminal tabs (`GrpcSessionTerminal`, dormant agent/session tabs, tmux tabs), the inspector and its tabs (details, activity, files, tools, usage, worktree, screen sharing, VNC), the agent roster/tree/conversation layers, notifications, traffic strip, status bar, and the code pane (`components/session/`) — into `packages/tddy-session-web`, with ~90 specs. It composes the terminal, chat, hosts, workspace, prstack and create-session packages.

## Background

This is the core of the app and the last feature left in `tddy-web`. Every package it composes exists by now, so the move is an import rewrite plus cutting the last upward reaches from `rpc`/`hooks` into sessions.

## Proposed Changes

### What's Changing

- New package; its PR check; the oversized-file record for `SessionMainPane` moves with it.

### What's Staying the Same

- Every session behaviour.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] Sessions live in `tddy-session-web` with no `tddy-web`/LiveKit import.
- [ ] Its specs pass from its own PR check.
- [ ] `tddy-web` has no feature component left except shell/routing/auth wiring and leftovers named in the changeset.

## References

- Predecessors (real): `livekit`, `create-session`, `terminal`, `chat`, `hosts`, `workspace`, `prstack`. Successors: `storybook`, `wiring`.
