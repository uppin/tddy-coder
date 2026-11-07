# PR-stack package (`tddy-prstack-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 13/16 — branch `feature/web-split/prstack`

## Affected Features

- **Primary**: the PR-stack screen, planned-PR panels, PR-stack chat and inspector, workflow chat (documented in `packages/tddy-web/docs/`).

## Summary

Move the PR-stack experience — `components/sessions/prstack/` (screen, planned-PR list/panel/rows, branch and base-sync status, reorder, repoint, start-session modal, PR-stack chat and inspector) and the workflow chat views — into `packages/tddy-prstack-web`, with ~40 specs. It opens create-session from `tddy-session-create-web` and renders chat from `tddy-chat-web`; its presenter room comes from `MediaPort`.

## Background

PR-stack is 6k lines with the most specs of any feature after sessions. Its three reaches into sessions are create-session (now a package), the presenter LiveKit room (now a port) and chat (now a package).

## Proposed Changes

### What's Changing

- New package; its PR check; the oversized-file record for `PrStackScreen` moves with it.

### What's Staying the Same

- PR-stack behaviour, URL state, chat streaming and elicitation.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] PR-stack lives in `tddy-prstack-web` with no `tddy-web`/LiveKit import.
- [ ] Its specs pass from its own PR check.
- [ ] The app renders it unchanged.

## References

- Predecessors (real): `livekit`, `create-session`, `chat`. Successor: `session`.
