# Changeset: Split the screen-sharing service along its seams

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
**Type**: Refactor (restructure, no behaviour change)
**Stack**: `#host-desktops` 1/8. Base: `master`.

## Initial Discovery

[2026-10-18-host-desktops-service-split-initial-discovery.md](./2026-10-18-host-desktops-service-split-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| **#626** `refactor(daemon-auth,screen-sharing,daemon-livekit): room tokens through an extension port (#satellite 4/13)` | ⛔ **BLOCKING — external sequencing gate** | #626 edits `prepare_bridge` / `build_bridge_spawn_config` in the file this node splits (adds a `RoomTokenMinter` parameter and `with_room_tokens`). **Developer decision, 2026-10-18: this node's `/green` must not start until #626 has merged into `master`.** `/green` checks `gh pr view 626 --json state` first; if it is not `MERGED`, stop and report. Then `/pr-stack-rebase` onto `origin/master` so the split moves #626's final shape. |
| `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` | ✅ RESOLVED HERE (or narrowed) | The point of this node. If the split ends above 500 lines, the record is **narrowed** with the new measurement, not deleted. |
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) | ⚠ DURING | Records engine defects seen moving this same file across crates. This node moves within the crate; any engine refusal is raised with the developer before a manual move (memory: restructure refusal needs consent). |

## Responsibility

The module layout of `packages/tddy-screen-sharing/src/`: `screen_sharing_service.rs` split into
the bridge-lifecycle module (seam A: `terminate_bridge`, `prompt_for_desktop_password`,
`try_spawn_bridge`, `spawn_bridge`), the bridge-config-and-naming module (seam B: `PreparedBridge`,
`prepare_bridge`, `build_bridge_spawn_config`, the `*_bridge_key` / identity / track-name helpers,
`host_livekit_room`, `desktop_prompt_subject`, `decrypt_desktop_password`, target-to-proto
conversions — as #626 leaves them), and the host-target RPC bodies (seam C) behind delegating
stubs on the `ScreenSharingService` impl.

## Boundaries

- No behaviour change, no new symbol beyond module paths, no test semantics changed.
- Does not split the 1,209-line inline test module (beyond moving tests that belong to a moved seam, if the engine carries them).
- Does not touch `host_tooling.rs` (nodes 4 and 7), the bridges, the proto or the web.
- Every move is executed by `tddy-tools restructure`; only post-move build fixes by hand, each marked.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `master` + **#626** (external, `#satellite 4/13`) | `prepare_bridge` / `build_bridge_spawn_config` on the `RoomTokenMinter` port | moves them, as merged, into the seam-B module | edit the minter threading, or start before #626 merges |

## Draft PR contract

A restructure has no surface to publish ahead of its own moves — the move *is* the implementation.
Wave 2 therefore publishes the **restructure plan** (the JSONL intents for
`tddy-tools restructure`, under `docs/dev/1-WIP/2026-10-18-host-desktops-service-split.plan.jsonl`)
and the module names, so nodes 6–8 can name their target modules in their documents. Its "tests"
are the existing `tddy-screen-sharing` suite, which must stay green: the baseline is recorded in
wave 2.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** **no — gated externally**: only after #626 merges (developer decision). Nothing inside this stack is needed.
**Concurrent with:** nodes 2, 3, 4, 5 (once #626 has merged)
**Blocks:** node 6 `bridge-ready`, node 7 `x11-channels`, node 8 `sign-in-form` — each edits code this node moves; greening them first means re-applying their edits across the move.

Real dependency edges, as opposed to the branch line:

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-screen-sharing` — [README](../../../packages/tddy-screen-sharing/README.md), [docs](../../../packages/tddy-screen-sharing/docs/)

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-service-split.md`; umbrella `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops.md`

## Summary

Move the oversized screen-sharing service into named modules once, before three feature nodes grow it.

## Scope

- [ ] Gate: #626 merged; branch rebased onto `origin/master`
- [ ] Prove seams with `restructure check --deep`
- [ ] Apply seams A, B, C
- [ ] Code issue resolved or narrowed with the final measurement

## Technical Changes

### State A
`packages/tddy-screen-sharing/src/screen_sharing_service.rs`: 967 production lines (2,250 total; tests
from L963). Seams per the code issue: A L179–374, B L377–622, C L632–773 (line numbers before #626).

### State B
`screen_sharing_service.rs` ≤ ~500 production lines; three sibling modules; public API unchanged.

### Delta
| Package | Change |
|---|---|
| `tddy-screen-sharing` | `src/screen_sharing_service.rs` split; new sibling modules; `lib.rs` `mod` lines |

## Implementation Milestones
- [ ] M1 #626 merged; rebased; baseline green recorded
- [ ] M2 Seams proven (`check --deep`)
- [ ] M3 Seams applied; `./test -p tddy-screen-sharing` green; clippy clean

## Testing Plan

No new tests. The existing `tddy-screen-sharing` suite (inline host-path tests in the service
module, `tests/screen_sharing_service_acceptance.rs`, `tests/session_vault_target_store_acceptance.rs`,
`tests/screen_sharing_records_unit.rs`, `tests/screen_sharing_record_propagation.rs`,
`tests/dead_vnc_service_removed.rs`) is the oracle. **Baseline:** green on `master` after #626.
This node has no planned red tests; any red test is a stop, and a test that changes state is a defect.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| the whole existing suite, unchanged | `packages/tddy-screen-sharing/**` | behaviour preserved |
| `tddy-daemon` `registers_the_screen_sharing_service_that_tddy_screen_sharing_now_owns` | `packages/tddy-daemon/tests/service_registration_acceptance.rs` | wiring unaffected |

## Decisions & Trade-offs
- **Restructure as node 1** rather than growing the file three times (developer, 2026-10-18).
- **Wait for #626** rather than narrowing to seams A + C (developer, 2026-10-18): the full split moves #626's final shape once.

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-service-split-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-service-split-initial-discovery.md`
- [ ] USER REVIEW — work complete
