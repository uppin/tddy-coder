# Workspace screens package (`tddy-workspace-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 12/16 — branch `feature/web-split/workspace`

## Affected Features

- **Primary**: [accounts-screen.md](../accounts-screen.md), [projects-screen-multi-host.md](../projects-screen-multi-host.md), [models-and-agents.md](../models-and-agents.md), [tasks-ui-realtime.md](../tasks-ui-realtime.md), [worktrees.md](../worktrees.md), [worktree-disk-usage-streaming.md](../worktree-disk-usage-streaming.md), [session-worktree-inspector.md](../session-worktree-inspector.md), daemon settings, sign-in dialogs ([codex-oauth-web-relay.md](../codex-oauth-web-relay.md)).

## Summary

Move the workspace administration screens — accounts, projects, settings (daemon settings), models and agents, tasks, worktrees (with `SessionWorktreeTab` and `worktreeStatsFormat`), and the sign-in dialogs (`CodexOAuthDialog`, `DeviceLoginPanel`, `CredentialVaultPrompt`, `DaemonLoginScreen`, `GitHubLoginButton`, `UserAvatar`) — into `packages/tddy-workspace-web`, with ~56 specs. `*AppPage` wrappers and the auth callback route stay as wiring.

## Background

These screens share a shape (a list or form over one daemon service) and are each too small for a package of their own. Models consumes chat, so this node follows `chat`; moving `SessionWorktreeTab` here breaks the worktrees → sessions cycle.

## Proposed Changes

### What's Changing

- New package; its PR check; the oversized-file record for `useModelRegistryFanOut` moves with its file.

### What's Staying the Same

- Every screen's behaviour and URL state.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] The screens live in `tddy-workspace-web` with no `tddy-web`/LiveKit import.
- [ ] Its specs pass from its own PR check.
- [ ] The app routes render them unchanged.

## References

- Predecessors (real): `chat` (models uses it), `clients`, `ui`. Successor: `session` (worktree tab).
