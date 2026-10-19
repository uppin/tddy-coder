# Changeset: Split the screen-sharing service along its seams

**Date**: 2026-10-18
**Status**: 🚧 In Progress — seams A, B (engine) and C (hand move, developer consent) applied
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
| C | `src/screen_sharing_service/host_targets.rs` (nested, beside `bridge_lifecycle`) — **done by hand, developer consent 2026-10-19** | bodies of `list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream` as inherent `pub(crate) async fn *_impl` in `impl ScreenSharingServiceImpl`; each trait method is a delegator `self.<name>_impl(request).await` | `extract_method` is refused inside the `#[async_trait]` impl (probes P1–P5 in the backlog entry; no other operation reaches a trait-impl body), so the move was hand-made: the 117 body lines are byte-identical (script-compared). Engine gap stays open: `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md` |
| — | stays | struct, builders, `require_{user,host_scope,config}`, `resolve_session_dir`, `BridgeTarget`, `HostScope`, `target_error_to_status`, session (vault) RPCs | — |

Expected result was ~440 production lines. **Achieved: 421** (983 → 537 after A and B → 421 after C), recorded in the code issue.

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
- [x] Apply seams A, B, C — A and B by the engine; C by hand with consent
- [x] Code issue: final measurement 421, resolved by this PR (delete at wrap)

## Technical Changes

### State A
`packages/tddy-screen-sharing/src/screen_sharing_service.rs`: 983 production lines after #626 (967 before; tests from L984). Seams per the code issue: A L179–374, B L377–622, C L632–773 (line numbers before #626).

### State B
`screen_sharing_service.rs` 421 production lines (≤ 500); three new modules (`bridge_config`, `bridge_lifecycle`, `host_targets`); public API unchanged.

### Delta
| Package | Change |
|---|---|
| `tddy-screen-sharing` | `src/screen_sharing_service.rs` split; new sibling modules; `lib.rs` `mod` lines |

## Implementation Milestones
- [x] M1 #626 merged; rebased; baseline green recorded
- [x] M2 Seams proven (`check --deep`) — A and B proven; C refused by the engine, hand-moved with evidence below
- [x] M3 Seams applied; `./test -p tddy-screen-sharing` green; clippy clean

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

### Seam C — hand move (2026-10-19)

Commit `2a251393f`. Hand-made with the developer's consent after `check --deep` refused every engine route (see the backlog entry).

| Evidence | Result |
|---|---|
| Moved bodies, before (`HEAD` trait impl) vs after (`*_impl`), script-compared | list 12, add 32, start 62, stop 11 = **117 lines, exact-identical** (and identical ignoring whitespace) |
| `restructure verify --against HEAD` | 0 statements lost, 16 gained: 4 `*_impl` signatures, 4 delegator bodies, 8 comment lines (the TODO markers and module header). Non-zero exit for that reason alone; `94 cfg(test) gate line(s) excused` |
| comment-line multiset (`//` lines, trimmed; `screen_sharing_service.rs` before vs `screen_sharing_service.rs` + `host_targets.rs` after) | 361 before, 369 after, **none lost**, 8 gained (4 delegator TODOs + 4 module-header lines) |
| `cargo fmt --check` | exit 0 |
| `cargo clippy -p tddy-screen-sharing --all-targets -- -D warnings` | exit 0 |
| `./test -p tddy-screen-sharing` (`.verify-result.txt`) | 19 + 3 + 3 + 10 + 8 + 6 + 0 = **49 passed, 0 failed, 0 ignored** |
| `cargo check -p tddy-daemon --all-targets` | not run: only `pub(crate)` methods were added, no public item changed |

Line counts after C: `screen_sharing_service.rs` **421 production**, `bridge_config.rs` 264, `bridge_lifecycle.rs` 243, `host_targets.rs` 160.

Hand edits for C (each marked `TODO(restructure): hand-moved`): the four bodies cut out of the trait impl into `host_targets.rs` (signature renamed `<name>_impl`, `pub(crate)`); the four trait methods reduced to `self.<name>_impl(request).await`; `mod host_targets;`; the explicit `use` list of the new module; removal of the seven imports the parent no longer needs by `cargo fix --lib --tests`, then `rustfmt` on `screen_sharing_service.rs`.

### /validate-changes

2026-10-19, after rebasing onto `origin/master` @ `c4a71571d` (leak check: `origin/master..HEAD` is this PR's 6 commits only). `./dev cargo build -p tddy-screen-sharing`: clean, no warnings.

| Check | Result |
|---|---|
| Changeset items implemented or deferred | ✅ A, B delivered; C delivered by hand after the engine refused it (see *Seam C*); the engine gap stays in `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md` — **this finding was written before C and is superseded** |
| `## Responsibility` delivered | ✅ A, B and C moved (C was outstanding when this check ran) |
| `## Dependencies` not implemented here | ✅ the #626 minter threading is moved as merged, not edited |
| `## Boundaries` respected | ✅ `host_tooling.rs`, bridges, proto and web untouched; no test semantics changed |
| Diff contains only this PR's files | ✅ |

**Findings**
- [WARNING] Layout drift against the published seam plan: the lifecycle module is `src/screen_sharing_service/bridge_lifecycle.rs` (nested), `require_target_store` moved with seam A, and `src/host_targets.rs` does not exist (superseded: C landed as `src/screen_sharing_service/host_targets.rs`, nested like the lifecycle module). Nodes 6–8 (#658, #659, #660) were told to name these modules; their own documents are on their own branches and were not checked from here — each must be reconciled when it is greened.
- [INFO] Two `TODO(restructure)` hand-edit markers stand in production code (`bridge_lifecycle.rs:1`, `screen_sharing_service.rs:85`), each describing an engine gap; both are listed under Result above.
- [INFO] Visibility widened to `pub(crate)` for the moved items (listed under Result); no public API changed.

### /validate-tests

2026-10-19, scoped to `git diff origin/master...HEAD -- packages/`. Status: no issues introduced.

- Tests analyzed: 18 inline tests in `screen_sharing_service.rs` (`mod tests`, line 423) plus the 5 integration suites of `tddy-screen-sharing`. The PR adds no test and changes no test body: its only edit in `mod tests` is one import, `use crate::bridge_config::build_bridge_spawn_config;` (the function moved). `bridge_config.rs`, `bridge_lifecycle.rs` and `host_targets.rs` hold no `#[cfg(test)]` code.
- Run: `./test -p tddy-screen-sharing` (scoped, output in `.verify-result.txt`): lib 19 passed, `dead_vnc_service_removed` 3, `screen_sharing_record_propagation` 3, `screen_sharing_records_unit` 10, `screen_sharing_service_acceptance` 8, `session_vault_target_store_acceptance` 6; 0 failed, 0 ignored. `cargo clippy -p tddy-screen-sharing --all-targets -- -D warnings` is clean.
- Anti-patterns introduced: 0 critical, 0 warning. No `#[ignore]`, `.skip` or commented-out test added. Description-body mismatches introduced: none.
- Coverage of the moved code: the four host-target RPCs now live in `host_targets.rs` behind delegators; the inline tests still reach them through the trait methods, so the delegation is exercised. I did not check per-test coverage of each moved body.
- [INFO] Pre-existing, not touched, not proposed for fixing: the inline tests do not follow the fluent-tests Given/When/Then page-object style throughout, and use short literal fixtures; I read the fluent-tests references only for calibration and did not audit the 18 pre-existing tests individually against them. Not a regression.

### /validate-prod-ready

2026-10-19, scoped to the PR diff. Status: Ready. Files checked: 5 production files (`bridge_config.rs`, `lib.rs`, `screen_sharing_service.rs`, `bridge_lifecycle.rs`, `host_targets.rs`); the inline `mod tests` is excluded. Blockers: 0. Warnings: 0 introduced. Verified: `cargo clippy -p tddy-screen-sharing --all-targets -- -D warnings` clean (no unused imports, dead code or unused variables; the gate for the `pub(crate)` widenings is also clean).

| Category | Count | Status |
|---|---|---|
| Mock code | 0 | ok |
| Dev fallbacks | 0 added | ok |
| TODO/FIXME | 7 (all this PR's own provenance markers) | acceptable, see below |
| Unused code | 0 | ok |
| Debug output | 0 (`println!`, `eprintln!`, `dbg!` none; `log::` calls moved unchanged) | ok |

- [INFO] TODO markers, 7 marker lines in all: `bridge_lifecycle.rs:1` (hand-added trait import `AsyncWriteExt`), `screen_sharing_service.rs:79` (`HostScope` hand-widened), `screen_sharing_service.rs:244,252,260,268` (four delegators) and `host_targets.rs:1` (module header). Each names what was hand-edited and why, and the three async_trait ones point at `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md`. They carry no issue number, but they are deliberate provenance markers with a tracking document; they delete when the engine gap closes. Acceptable. Note: the /validate-changes entry above cites `screen_sharing_service.rs:85` for the `HostScope` marker; it is now line 79.
- [INFO] `pub(crate)` widenings (engine for moved items, by hand for `HostScope`, `DEFAULT_STREAM_*`, `BridgeTarget` fields): no public API change, crate-internal only; recorded as the cost of the split.
- [INFO] No environment-conditional or silent-recovery code added. `spawn_bridge`'s `child.id().unwrap_or(0)` and `let _ = child.kill()` are moved byte for byte and pre-existing; not proposed for change.
- Not run: `cargo build`/`cargo test` workspace-wide (repo policy: CI's job); only the scoped runs above.

### /analyze-clean-code

2026-10-19, scoped to the PR diff, measured by hand (no tool run for metrics; function lengths counted from source). Findings below are all in moved-but-unchanged code and are INFO: the no-behaviour-change contract rules out editing them. **Introduced by this PR: none.**

**Overall score by the rubric: F (raw), B-to-C for what the PR changed.** The rubric counts the moved code's pre-existing "must refactor" items (below); none is new, and the PR shrank the file it came from.

| Metric | Excellent | Acceptable | Needs Attention | Must Refactor |
|---|---|---|---|---|
| File length (prod files) | - | 4 of 5 at or under 500 | 1 (`screen_sharing_service.rs`, see below) | - |
| Function length (moved fns) | most | `prompt_for_desktop_password` 34, `try_spawn_bridge` 33, `add_host_target_impl` 37, `build_bridge_spawn_config` 45 (needs attention) | `build_bridge_spawn_config` 45 | `spawn_bridge` 70, `start_host_stream_impl` 67 |
| Parameter count | - | - | - | `prepare_bridge` 7, `build_bridge_spawn_config` 7, `try_spawn_bridge` 9 incl. `&self` (8 without; carries `#[allow(clippy::too_many_arguments)]`) |

**Must refactor, all pre-existing (INFO, not to be fixed here)**
- `bridge_lifecycle.rs` `spawn_bridge`: 70 lines. Candidate later: separate the stdin hand-off and the reaper task.
- `host_targets.rs` `start_host_stream_impl`: 67 lines, moved byte-identical from the trait impl (it was 67 there too; the 116 lines moved out of the god-file are the gain).
- `prepare_bridge` / `build_bridge_spawn_config` / `try_spawn_bridge`: 7, 7 and 8 parameters. Candidate later: a `BridgeRequest` options struct (also removes the clippy allow).

**Oversized files (more than 500 lines)**
- `screen_sharing_service.rs` — 1,777 lines whole-file, **pre-existing and shrunk by this change (2,338 on `origin/master`, -561)**. 421 are production, within the 500 budget; the other ~1,350 are the inline `mod tests` (line 423 on). Responsibilities left: service struct and builders, session (vault) target RPCs, the four delegators, tests. Proposed split: `screen_sharing_service/tests.rs` or a `tests/` directory for the inline module. Cost: the tests use `super::*` private items (`ScreenSharingServiceImpl` fields, `HostScope`), so a child module `tests` in its own file keeps access and needs no widening. Recommendation: split later (out of scope for a behaviour-preserving restructure; the production budget, which is what the code issue tracks, is met).
- `bridge_config.rs` 264, `bridge_lifecycle.rs` 243, `host_targets.rs` 160, all under 500.

**Magic values**: none introduced. `BRIDGE_TOKEN_TTL` (6 x 3600) is a named constant; `DEFAULT_STREAM_WIDTH`/`HEIGHT`/`FPS` are named and shared.

**Duplication**: none introduced. The `Self::spawn_bridge` / `try_spawn_bridge` pair and the repeated "require user, require host scope" preamble across the four `*_impl` bodies are pre-existing (INFO).

**Naming**: the `*_impl` suffix on the four `host_targets.rs` methods is the PR's own, and is the usual delegator convention; the TODO markers explain it. `bridge_lifecycle.rs` has one `use` per line separated by blank lines (the engine's output style); a grouped `use` block would be tidier but was left, since `rustfmt` accepts it and rewriting engine output adds nothing. INFO.

Nothing needs a fix before merge. Not run: any metrics tool (none exists for this rubric); numbers above are counted by hand and may be off by a line or two.

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-service-split-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests — N/A: a restructure adds no tests; the existing suite is the oracle
- [x] Run acceptance tests (verify they fail) — N/A; baseline recorded green instead
- [x] USER REVIEW — acceptance tests — N/A (seam plan reviewed with the wave-1 batch)
- [x] TDD Red — write failing unit/integration tests — N/A for a restructure
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Run scoped tests for touched packages (full-workspace result from CI still to come)
- [x] Validate changes (/validate-changes)
- [x] Validate tests (/validate-tests)
- [x] Validate production readiness (/validate-prod-ready)
- [x] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt --check`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-service-split-initial-discovery.md`
- [ ] USER REVIEW — work complete
