# Changeset: Split the screen-sharing service along its seams

**Date**: 2026-10-18
**Status**: 🚧 In Progress — seams A and B applied; seam C deferred (engine gap)
**PR**: [#653](https://github.com/uppin/tddy-coder/pull/653)
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

## Seam plan (published in wave 2)

The module layout nodes 6–8 target. Line numbers were `master` @ `515c00149`, before #626;
the `tddy-tools restructure` JSONL is generated at green, **after #626 merges**, against the shape it
leaves, and proven with `restructure check --deep` before it is applied.

| Seam | New module | Items | How |
|---|---|---|---|
| A | `src/screen_sharing_service/bridge_lifecycle.rs` (**nested** under the service module — the engine's `extract_module … to_file` places it there) | `ScreenSharingServiceImpl::{terminate_bridge, require_target_store, prompt_for_desktop_password, try_spawn_bridge, spawn_bridge}` — `require_target_store` **travels with A**: it sits between `terminate_bridge` and the rest, and one `extract_module` needs one contiguous run | `extract_module` (`to_file`, `reexport: none`). **Not `retarget_impl`**: that changes an `impl`'s self type and refuses the same type (`there is nothing to retarget`) |
| B | `src/bridge_config.rs` | `BridgeSpawnConfig`, `PreparedBridge`, `prepare_bridge`, `build_bridge_spawn_config`, `BRIDGE_TOKEN_TTL`, `session_bridge_key`, `host_bridge_key`, `host_bridge_identity`, `screenshare_track_name`, `browser_livekit_url`, `host_livekit_room`, `desktop_prompt_subject`, `decrypt_desktop_password`, `host_target_as_bridge_target`, `host_target_to_proto` | `move_item`, two plans (the first carries `name` and creates the module; `check` refuses a `to` that does not exist yet, so `BridgeSpawnConfig` moved in a second plan) |
| C | `src/host_targets.rs` — **deferred, not done** | bodies of `list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream` | `extract_method` is refused inside the `#[async_trait]` impl (`offered: Extract into variable`; a plain `let` probe too; cause unconfirmed). Filed as `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md` |
| — | stays | struct, builders, `require_{user,host_scope,config}`, `resolve_session_dir`, `BridgeTarget`, `HostScope`, `target_error_to_status`, session (vault) RPCs, and (until C) the host-target RPC bodies | — |

Expected result was ~440 production lines. **Achieved: 537** (983 − A − B; C not done), recorded in the code issue, which is narrowed, not closed.

**Baseline (wave 2, 2026-10-18, before #626):** see `## Validation Results` → *Baseline*.

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

- [x] Gate: #626 merged; branch rebased onto `origin/master`
- [x] Prove seams with `restructure check --deep` (A and B; C refused)
- [ ] Apply seams A, B, C — A and B applied; **C deferred** (engine gap)
- [x] Code issue narrowed with the final measurement (537)

## Technical Changes

### State A
`packages/tddy-screen-sharing/src/screen_sharing_service.rs`: 983 production lines after #626 (967 before; tests from L984). Seams per the code issue: A L179–374, B L377–622, C L632–773 (line numbers before #626).

### State B
`screen_sharing_service.rs` 537 production lines (target ≤ 500, missed by 37 — seam C); two new modules; public API unchanged.

### Delta
| Package | Change |
|---|---|
| `tddy-screen-sharing` | `src/screen_sharing_service.rs` split; new sibling modules; `lib.rs` `mod` lines |

## Implementation Milestones
- [x] M1 #626 merged; rebased; baseline green recorded
- [ ] M2 Seams proven (`check --deep`) — A and B proven, C refused
- [ ] M3 Seams applied — A and B applied and green; C not applied

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
### Baseline

2026-10-18, wave 2, on `master` @ `515c00149` (**before #626**): `./test -p tddy-screen-sharing` — 7 test binaries, **48 passed, 0 failed, 0 ignored** (10 + 18 + 3 + 3 + 6 + 8 + 0). **Re-recorded 2026-10-19 on the rebased tree (#626 merged): 7 binaries, 49 passed, 0 failed, 0 ignored** (the +1 over 48 is #626's test) — the restructure's baseline.

### Result (2026-10-19)

Seams A and B applied by `tddy-tools restructure`; seam C refused. Commits: `fe274c0c0` (A), `b66d583ec` (B), `b63a398b8` (`HostScope` widening).

| Gate (scoped to `tddy-screen-sharing`) | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy -p tddy-screen-sharing --all-targets -- -D warnings` | exit 0 (after the `HostScope` fix; before it, `private_interfaces` on `HostScope`) |
| `./test -p tddy-screen-sharing` (`.verify-result.txt`) | 19 + 3 + 3 + 10 + 8 + 6 + 0 = **49 passed, 0 failed, 0 ignored** — equals the baseline |
| `restructure verify --against HEAD` after each plan | "every statement accounted for" (A: 5 visibility-normalised before the hand edit; B: 38) |
| `restructure verify --against 60b895053` | 0 statements lost, 5 gained: the lines of the two hand-edit TODO comments; non-zero exit for that reason alone |
| comment-line multiset (`//` lines, trimmed, `screen_sharing_service.rs` + `lib.rs` before vs `screen_sharing_service.rs` + `bridge_config.rs` + `bridge_lifecycle.rs` + `lib.rs` after) | 514 before, 519 after; **none lost**, 5 gained (the same two TODO comments) |
| `cargo check -p tddy-daemon --all-targets` | not run: no public item moved or changed |

Line counts: `screen_sharing_service.rs` 537 production (1,890 total), `bridge_config.rs` 264, `bridge_lifecycle.rs` 243.

**Hand edits** (each marked `TODO(restructure)`):
1. `bridge_lifecycle.rs`: `use tokio::io::AsyncWriteExt;` — `extract_module` does not carry a trait import used only for its methods; the apply's compile gate failed (`write_all`) and stopped before its tidy and format.
2. `screen_sharing_service.rs`: `struct HostScope` → `pub(crate) struct HostScope` (clippy `private_interfaces`; the engine widens moved items only).
3. Tool-driven, outside `apply`: `cargo fix --lib --tests -p tddy-screen-sharing` (unused imports) and `rustfmt` over the two files, because the failed apply never ran its own tidy; `apply --resume` was a no-op.

**`pub(crate)` widenings by the engine:** seam A — `terminate_bridge`, `require_target_store`, `prompt_for_desktop_password`, `try_spawn_bridge`, `spawn_bridge`. Seam B — `BridgeSpawnConfig` and its 13 fields, `BridgeTarget` and its 4 fields, `PreparedBridge::{binary, spawn_config, for_password}`, `DEFAULT_STREAM_FPS`.

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-service-split-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests — N/A: a restructure adds no tests; the existing suite is the oracle
- [x] Run acceptance tests (verify they fail) — N/A; baseline recorded green instead
- [x] USER REVIEW — acceptance tests — N/A (seam plan reviewed with the wave-1 batch)
- [x] TDD Red — write failing unit/integration tests — N/A for a restructure
- [ ] TDD Green — implement with quality code (A and B done; C deferred)
- [x] Update documentation with progress
- [x] Run scoped tests for touched packages (full-workspace result from CI still to come)
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt --check`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-service-split-initial-discovery.md`
- [ ] USER REVIEW — work complete
