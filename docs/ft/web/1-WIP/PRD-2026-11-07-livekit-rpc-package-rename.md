# LiveKit RPC transport package rename (`tddy-livekit-rpc-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 4/16 — branch `feature/web-split/livekit-rpc`

## Affected Features

- **Primary**: [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md) — RPC over a LiveKit
  data channel; the transport package is renamed, its behaviour is not.
- **Related**: `tddy-desktop` and `tddy-web`, which consume the transport.

## Summary

Rename `packages/tddy-livekit-web` — which today holds only the **LiveKit RPC transport** (chunking,
envelope transport over the data channel, transport factory) — to `packages/tddy-livekit-rpc-web`.
This frees the name `tddy-livekit-web` for the LiveKit **wiring** package created later in the stack
(`feature/web-split/livekit`), which will own every remaining `livekit-client` use, and keeps the RPC
transport a package of its own, as the developer asked.

## Background

The stack's rule is that only LiveKit packages may import `livekit-client`. Two concerns exist: the RPC
transport (already a package) and the room/presence/media code scattered through `tddy-web`. Keeping
them apart lets a package that needs only RPC over LiveKit avoid the whole media layer.

## Proposed Changes

### What's Changing

- Directory, package name, exports, Cypress config, docs and its PR check workflow renamed.
- Every consumer (`tddy-web` dependency, Vite/tsconfig source aliases, `prebuild`, `tddy-desktop`,
  docs, scripts naming it) updated to the new name.

### What's Staying the Same

- The transport's code, API, tests and behaviour are byte-for-byte the same apart from names.

## Impact Analysis

### Technical Impact

- Mechanical rename; `bun.lock` changes. The package's generated code paths move with it.

### User Impact

None.

## Acceptance Criteria

- [ ] `packages/tddy-livekit-rpc-web` exists with the same exports and passing specs from its own PR check.
- [ ] Nothing in the repository refers to the old package name as the transport.
- [ ] `tddy-web` and `tddy-desktop` build against the renamed package.

## References

- Successor: `feature/web-split/livekit` (new `tddy-livekit-web` wiring package).
- Backlog: [2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md](../../../dev/todo/2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md) — the testkit file moves; not fixed here.
