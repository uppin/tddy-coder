# RPC playground package (`tddy-rpc-playground-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 9/16 — branch `feature/web-split/playground`

## Affected Features

- **Primary**: the RPC playground screen (reflection-driven RPC invocation; `rpc-playground/`).

## Summary

Move the RPC playground — `RpcPlaygroundScreen`, its invoke/registry/JSON helpers and their tests —
into `packages/tddy-rpc-playground-web`. The page that places it inside the app shell
(`RpcPlaygroundAppPage`: shell, sign-in, presence) is wiring and stays in `tddy-web`. Its Cypress specs
move with it.

## Background

The smallest feature, and a self-contained one: it uses reflection and a raw transport from the
clients port. It proves the package pattern end to end on a small surface.

## Proposed Changes

### What's Changing

- New package exporting `RpcPlaygroundScreen`; `bun test` suites for the helpers and its Cypress specs
  move; its PR check.

### What's Staying the Same

- Playground behaviour, including URL state.

## Acceptance Criteria

- [ ] The playground screen lives in `tddy-rpc-playground-web`, importing nothing from `tddy-web` or LiveKit.
- [ ] Its unit and Cypress specs pass from its own PR check.
- [ ] The playground page works as before.

## References

- Predecessors (real): `clients`, `ui`.
