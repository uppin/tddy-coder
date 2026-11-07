# Create-session package (`tddy-session-create-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 7/16 — branch `feature/web-split/create-session`

## Affected Features

- **Primary**: [session-agent-catalog-fan-out.md](../session-agent-catalog-fan-out.md),
  [tool-session-model-selection.md](../tool-session-model-selection.md),
  [web-workspace-setup.md](../web-workspace-setup.md) (project home setup), session start progress.
- **Related**: PR-stack "start session" flows, which open the create-session dialog.

## Summary

Move the create-session experience into `packages/tddy-session-create-web`: `CreateSessionPane`,
`CreateSessionDialog`, every `CreateSession*` field and section, their hooks
(`useCreateSession*`, `useAvailableAgents`, `useProjectBranches`, …), the request builder and recipes,
codebase placement, start progress, the attachment staging UI (`attachments/`), and the modules it
borrows from other areas today (`prstack/baseBranchChoice`, `startProgress`, `pendingAttachment`).
Its ~44 Cypress component specs move into the package.

## Background

Create-session is the largest self-contained flow (~5k lines), with one inbound user (PR-stack
start-session) and no LiveKit reach. It uses only generated clients, the host fan-out hook and UI
primitives — all available from the packages below it.

## Proposed Changes

### What's Changing

- New package exporting `CreateSessionPane`, `CreateSessionDialog` and the hooks other packages call;
  its specs, page objects and fixtures move with it; its PR check.
- The oversized-file record for `CreateSessionPane.tsx` moves with the file.

### What's Staying the Same

- Every create-session behaviour, field and request.

## Acceptance Criteria

- [ ] Create-session lives in `tddy-session-create-web`, imports nothing from `tddy-web` or LiveKit.
- [ ] Its specs pass from the package's Cypress config and PR check.
- [ ] `tddy-web` renders it unchanged.

## References

- Predecessors (real): `clients`, `ui`. Successors: `prstack`, `session`.
