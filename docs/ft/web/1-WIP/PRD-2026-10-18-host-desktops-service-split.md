# Split the screen-sharing service along its seams — PRD

**Date**: 2026-10-18
**PRD Type**: Technical Improvement (restructure, no behaviour change)
**Stack**: `#host-desktops` 1/8. Umbrella: `PRD-2026-10-18-host-desktops.md` on this branch.

## Affected Features

- None user-visible. Technical: [`packages/tddy-screen-sharing/docs/screen-sharing-service.md`](../../../../packages/tddy-screen-sharing/docs/screen-sharing-service.md)

## Summary

`screen_sharing_service.rs` (967 production lines, budget 500) is split into modules along the seams
its code-issue record names. Bridge lifecycle (spawn, terminate, desktop-password prompt) moves to one
module, bridge config and naming to another, and the host-target RPC bodies to a third behind
delegating stubs. Everything is moved by `/code-restructuring`, never by hand.

## Background

Three nodes of this stack (6, 7 and 8) grow this file. Growing a file that is 1.9× over budget,
three times over, is the debt the code issue records. Moving it once before them gives each feature
node a reviewable diff in a named module.

**Sequencing:** #626 (`#satellite 4/13`) edits `prepare_bridge` / `build_bridge_spawn_config` in this
file. At the developer's decision (2026-10-18), **this node's implementation starts only after #626
has merged**, so the split moves #626's final shape instead of racing it.

## Proposed Changes

### What's Changing
- `screen_sharing_service.rs` keeps the struct, builders, `require_*` helpers and the session (vault) RPCs.
- New modules: bridge lifecycle (seam A), bridge config and naming (seam B, as #626 left it), host-target RPC bodies (seam C).

### What's Staying the Same
- Every public symbol's path, every RPC's behaviour, every test.

## Success Criteria
- [ ] `screen_sharing_service.rs` ≤ 500 production lines, or the remainder recorded in the code issue with the developer's consent
- [ ] `./test -p tddy-screen-sharing` passes unchanged; no test edited beyond import paths
- [ ] Moves performed by `tddy-tools restructure`; any refusal raised with the developer before a manual move

## Out of Scope
- Any behaviour change; the 1,209-line inline test module (separate question per the code issue).
