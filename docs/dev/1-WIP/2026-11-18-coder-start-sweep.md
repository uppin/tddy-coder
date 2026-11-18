# Changeset: The coder-start sweep pays down the stack's structural debt

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 3 — three packages: one `move_item` of a topic module (`tddy-remote-shell`), six `extract_method`s over one 184-line function in two plans, one of them a tail range that keeps its `return`s (`tddy-agent-launch`), one `extract_method` (`tddy-session-driver`); a recorded green baseline, no behaviour change, no new public surface
**Stack**: `#coder-start` 4/4 (sweep) — branch `feature/coder-start/sweep`, base `feature/coder-start/session-driver-cli` (#942)

## Initial Discovery

[2026-11-18-coder-start-sweep-initial-discovery.md](./2026-11-18-coder-start-sweep-initial-discovery.md)

## Prerequisites

None. Each of the three records in the backlog is `Open — unclaimed`; nothing in the touched code carries a `**Claimed by:**` line.

## Affected Packages

- **tddy-remote-shell** — the Coder one-shot transport (start marker, stdin lines, the wrapper command line, the marker-noting reader) moves from `src/coder.rs` into `src/coder/one_shot.rs`
  - `docs/architecture.md` — the Coder pty section names `coder.rs` for the one-shot wrapper → name `coder/one_shot.rs`
- **tddy-agent-launch** — `launch_on_target` in `src/remote_agent_launch.rs` is cut into its steps, each its own function in the same file
  - `no dev-doc change`: no doc names the function's body
- **tddy-session-driver** — `DriverSession::launch`'s local startup-window judgement becomes its own function
  - `no dev-doc change`

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

The `#coder-start` stack left three code issues, measured by the sweep-collect run on `6e88f90e6`
and recorded under each package's `docs/code-issues/`:

| Record | Measurement | Base (`2ed59a3c7`) |
|---|---|---|
| [`oversized-file-coder.md`](../../../packages/tddy-remote-shell/docs/code-issues/oversized-file-coder.md) | `coder.rs` 507 production lines, budget 500 | 340 |
| [`complexity-remote-agent-launch-launch-on-target.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md) | `launch_on_target` 184 lines, budget 60 | 180 (worsened by the stack) |
| [`complexity-session-launch.md`](../../../packages/tddy-session-driver/docs/code-issues/complexity-session-launch.md) | `DriverSession::launch` 61 lines, budget 60 | new |

This node closes all three by engine moves and extractions — no moved or extracted code written by
hand — and deletes each record in the commit that closes it, final measurement first. Everything
stays reachable as it is today: the moved one-shot items were private to `coder.rs` and are used
only by `CoderShell` and `coder.rs`'s own tests; the extracted functions are private helpers beside
the function they came from. No `pub` item moves, so no caller outside the three files changes.

## Background

- `coder.rs` grew 340 → 507 in #939: the pty-safe one-shot (counted base64 stdin, `stty -opost`,
  the marker-gated feed) is ~108 lines of free functions and constants (lines 80–187) with one
  caller, `CoderShell` — a ready-made seam.
- `launch_on_target` was 180 lines before the stack and #941 added the `PATH` copy (184). It reads
  as steps — prepare the pid directory, write the launch's files, compose the remote command,
  compose the local client's argv and environment, spawn the client, judge the start, record the
  owner. Everything from the start judgement on runs to the function's tail, so it is one
  tail-range extraction that keeps its `return Err(…)`s verbatim; the owner recording is then cut
  out of that new function.
- `DriverSession::launch` is one line over; its local arm's startup-window wait loop is a clean
  four-line range.
- The CRAP report scores `DriverSession::launch`/`stop`/`wait_for` 42–56 at 0% coverage. That is a
  blind spot, not a finding: `session_driver_acceptance` drives them through the spawned binary,
  which the coverage capture does not instrument (recorded in the `launch` record's "Verified by
  hand"). Not in this backlog.

## Scope

- [ ] **Plan**: `plan.jsonl` per seam, anchors emitted by `restructure anchors`, `check --deep` clean
- [ ] **Apply**: dry run clean, each plan applied with its compile gate, `verify --against` clean, one commit per plan
- [ ] **Baseline**: the four packages' suites back to the recorded numbers, failing set equal by name (empty)
- [ ] **Code Quality**: `./dev cargo clippy -p tddy-remote-shell -p tddy-agent-launch -p tddy-session-driver --all-targets -- -D warnings` and `cargo fmt --check` clean
- [ ] **Records**: the three records deleted by the commits that close them, final measurement recorded first
- [ ] **Documentation**: Final Checklist executed

## Responsibility

This PR owns **three mechanical restructures and nothing else**: the module move and the
extractions above, the `mod`/`use` wiring they need, and the record deletions. Any behaviour
change, signature change of an existing function or public-surface change is a stop.

## Boundaries

- **No behaviour change** — moved and extracted bodies keep their statements, up to visibility, the call that replaces a range, and import wiring.
- **No new public surface** — `pub(super)`/`pub(crate)` widening only where the move forces it (the one-shot items are read by `CoderShell` in the parent module).
- **No TODO** may be added; it either does the work or stops.
- **Not this node's:** every `docs/dev/todo/` entry the stack added (the TODO gate decides them — none is closable by a restructure); the pre-existing `schema_offering` record (unchanged at 192 by the stack); the `RegionArgs` → `GridRegion` field mapping duplicated between `tddy-session-driver/src/server.rs` and `tddy-tool-engine`'s terminal tools (below the record bar).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #939 (`fix/address-01a1ecee-findings`) | the Coder one-shot pty transport in `coder.rs` | moves it, unchanged | change it |
| #941 (`feature/coder-start/interactive-launch`) | `launch_on_target`'s `PATH` copy, the deadline-bounded probes | extracts them, unchanged | change them |
| #942 (`feature/coder-start/session-driver-cli`) | `DriverSession::launch` | extracts its local startup judgement | change it |

## Green baseline

Recorded on `6e88f90e6` (#942 before its test-only LiveKit fix `82fb5ad57`, which touches none of
these packages), scoped to the touched packages:

- `./test -p tddy-remote-shell -p tddy-tool-engine -p tddy-agent-launch -p tddy-session-driver` — exit 0, no failing test:
  - tddy-remote-shell: lib 27, coder_shell_acceptance 14, ssh_shell_acceptance 8, shell_contract 3, one_ssh_spawn_site 2
  - tddy-agent-launch: lib 282, 19 integration suites (coder_agent_launch_acceptance 6, remote_agent_launch_acceptance 6, coder_agent_launch_on_path_acceptance 1, …)
  - tddy-session-driver: lib 3, session_driver_acceptance 7
  - tddy-tool-engine: lib 67, ~35 integration suites (in the run, not touched here)
- `./dev cargo clippy -p <pkg> --all-targets -- -D warnings` — clean for tddy-remote-shell, tddy-agent-launch, tddy-session-driver (#942's [W])

The same suites are the after-gate; CI on the ready PR confirms the workspace.

## Draft PR contract

`Type: Refactor` — the contract is this plan plus the green baseline above, with `plan.jsonl`
files of `move_item`/`extract_method` intents per `code-restructuring`. No failing tests: the
recorded green baseline stands in for them.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** yes — it depends on no behaviour, only on the files existing.
**Concurrent with:** none
**Blocks:** none

## Technical Changes

### State A (on `feature/coder-start/session-driver-cli`, `82fb5ad57`)

| File | Production lines | Symbols in scope |
|---|---|---|
| `packages/tddy-remote-shell/src/coder.rs` | 507 (`#[cfg(test)]` at 508) | `START_MARKER`, `strip_start_marker`, `STDIN_LINE_WIDTH`, `STDIN_NOT_WHOLE_STATUS`, `stdin_lines`, `one_shot_command`, `holds_the_start_marker_line`, `read_noting_the_marker` (lines 80–187) |
| `packages/tddy-agent-launch/src/remote_agent_launch.rs` | 347 | `launch_on_target` 184 lines (163–346), 6 parameters |
| `packages/tddy-session-driver/src/session.rs` | 298 | `DriverSession::launch` 61 lines (99–159) |

### State B

| Module | Holds | Projected lines |
|---|---|---|
| `tddy-remote-shell/src/coder.rs` | credentials, workspace ref, `CoderStdin`, `CoderShell` | ~400 |
| `tddy-remote-shell/src/coder/one_shot.rs` (new) | the eight one-shot items above | ~110 |
| `remote_agent_launch.rs` `launch_on_target` | the pid directory call, the slot/plan bindings, the file-writing calls, the remote command, the client command call, the spawn, a call to `judge_the_start` as its tail | ~54 |
| `remote_agent_launch.rs`, new private fns | `prepare_pid_directory` (~22), `write_slot_files` (~10), `write_rendered_session_files` (~24), `client_command` (~30), `judge_the_start` (start wait, early exit, owner call, tail; ~50), `recorded_owner` (~25) | each < 60 |
| `session.rs` `DriverSession::launch` | as today, the local wait loop replaced by a call | ~58 |
| `session.rs`, new private fn | `wait_out_the_startup_window` | ~8 |

The names are proposals; the engine's assist names the function and the plan's `name` field sets it.

### Delta — seams in dependency order

1. `tddy-remote-shell` — `move_item` with `name: "one_shot"`, `to: crate::coder`, `reexport: "none"` — the eight items; `CoderShell`'s uses and the test module's `START_MARKER` re-pointed.
2. `tddy-agent-launch`, plan 2a — `extract_method` ×5 over `launch_on_target`, **bottom-up**: the tail from `let start_bound` to `Ok(RemoteLaunched { … })` (`judge_the_start`; a tail range, its `return Err`s kept verbatim) → client argv/env (`client_command`) → the session-dir file loop → the slot-file loop → the pid-directory `exec`.
3. `tddy-agent-launch`, plan 2b — `extract_method` ×1 over `judge_the_start`: `let owner = match remote.pointer { … };` → `recorded_owner`. Its anchor names `judge_the_start`, which exists only after 2a, so **[G] emits it** (`restructure anchors … --at`) after applying 2a and runs `check --deep` on it before applying.
4. `tddy-session-driver` — `extract_method` ×1: `let deadline …` and its `while` loop → `wait_out_the_startup_window`.

## Callers

| | |
|---|---|
| External importers | none — every moved or extracted item is private to its file today |
| Facade | none; `reexport: "none"` re-points `CoderShell`'s uses inside `coder.rs` |
| Consequence | no file outside the three changes, beyond the new `coder/one_shot.rs` |

## Visibility

Expected: the eight one-shot items widen from private to `pub(super)` (read by `CoderShell` in
`coder.rs`). The extracted functions are private. Recorded after the apply from the engine's
report.

## Plan

`docs/dev/1-WIP/2026-11-18-coder-start-sweep/`, anchors emitted by `restructure anchors` on `82fb5ad57` against the warm index daemon, each `check --deep` → `no findings`:

| Plan | Operations | `check --deep` |
|---|---|---|
| `plan-1-one-shot.jsonl` | 1 `move_item` (8 items → `tddy_remote_shell::coder::one_shot`, `reexport: none`) | no findings; reports 5 widenings private → `pub(super)` (`START_MARKER`, `strip_start_marker`, `stdin_lines`, `one_shot_command`, `read_noting_the_marker`) |
| `plan-2a-launch-on-target.jsonl` | 5 `extract_method` | no findings |
| plan 2b | 1 `extract_method` (`recorded_owner`) | written and checked by [G] after 2a |
| `plan-3-driver-launch.jsonl` | 1 `extract_method` | no findings |

Apply order: 1 → 2a → 2b → 3, one commit each.

## Decisions & Trade-offs

- **No guard lift.** The first plan lifted the early-exit `if exits_during_startup(…).await { …; return Err(…) }` and the driver's `if *handle.pty_done.borrow() { return Err(…) }` into `-> Result<()>` functions. `check --deep` refused both: rust-analyzer writes the extracted guard of an `async` function in a shape the engine's lift does not recognise (`expected if let Some(..) = f(..) { return ..; } or f(..)?;`). The tail-range extraction keeps those `return`s verbatim instead, and the driver's cut takes the wait loop only. The lift's gap with `async` guards is an engine limitation, recorded here; not this node's to fix.
- **The two file-writing loops are two extractions.** As one range, rust-analyzer dropped the three-line comment between them and the engine could not pair it back; split, the comment stays above the second call.
- **A range cannot start on a comment.** The tail range starts at `let start_bound`; the five-line comment explaining the startup window stays above the call in `launch_on_target`.
- **`launch_on_target`'s 6 parameters stay.** The record names parameter count 6 > 5 too; `RemoteLaunch`/`HostPtySession` already bundle the rest and a further bundle would change the public signature, which is outside a Refactor node. If the record's parameter threshold still reads as breached at wrap, the record is narrowed, not deleted, and that is put to the developer. ⚠
- **The session-driver CRAP scores are not findings** (coverage blind spot for binary-spawned tests).

## Final Checklist

- [ ] `packages/tddy-remote-shell/docs/changesets/2026-11-18-coder-start-sweep.md` — release-note file
- [ ] `packages/tddy-agent-launch/docs/changesets/2026-11-18-coder-start-sweep.md` — release-note file
- [ ] `packages/tddy-session-driver/docs/changesets/2026-11-18-coder-start-sweep.md` — release-note file
- [ ] `packages/tddy-remote-shell/docs/architecture.md` — where it names `coder.rs` as holding the one-shot wrapper → `coder/one_shot.rs`
- [ ] `docs/dev/changesets/2026-11-18-coder-start-sweep.md` — cross-package entry with the before/after numbers

## Successor PRs

None — the stack's tail.

## TODO

- [x] Record initial discovery (`2026-11-18-coder-start-sweep-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [ ] Create failing acceptance tests — N/A (`Type: Refactor`; green baseline recorded instead)
- [ ] USER REVIEW — the refactor plan (this changeset + plan files)
- [ ] TDD Green — apply the plans via the restructure engine
- [ ] Run the touched packages' tests — failing set equal to the baseline's (empty)
- [ ] Validate changes (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
