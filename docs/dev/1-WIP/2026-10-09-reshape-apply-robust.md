# Changeset: `restructure apply` refuses before it half-moves a file, checks the destination first, and never waits on a silent server

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Bug fix (engine robustness: preflight, compile gate, readiness bounds, spawn record, refusal text)
**Stack**: `#reshape` 10/19, branch `feature/reshape/apply-robust`, PR [#607](https://github.com/uppin/tddy-coder/pull/607), wave 1. PR title:
`fix(code-restructuring): apply never half-moves, gates destinations, bounds silent waits (#reshape 10/19)`.
Base in the linear stack: `feature/reshape/new-crate` (K=9). **Real edges**: none. Nothing this node does consumes another node's behaviour, and no node consumes this one's.
**Textual collisions** (not edges): node 1 (`widen-same-crate`) adds emptied-directory removal beside `git_move` in `src/apply.rs`, and nodes 1–12 share `backends/rust.rs` wiring. Resolve these at rebase; the behaviour is independent.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-apply-robust-initial-discovery.md). Exploration 1 is the whole-work discovery. Exploration 2 is this node's, with file:line evidence for every claim below.

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `tddy-code-restructuring`, `tddy-lsp` and `tddy-index-daemon` code issues finds one file (`broken-restructure-anchors-empty-outline.md`), with value `none` and owned by node 12. **No 🚧 claimed issue is in this node's path.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md](../todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md) | ✅ **RESOLVED HERE** | Reproduces and records the shape (M1). Every type-inference wait gets the ready-silence bound. A loading server silent for the stall bound is refused as `ServerStalled`. Deleted at wrap, with the reproduction result recorded in the change history |
| [2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md) (its only open item: the pre-apply gate omits the destination) | ✅ **RESOLVED HERE** | The baseline covers each cross-crate move's `to` package (M3). Deleted at wrap |
| [2026-10-05-restructure-no-record-of-what-an-apply-executes.md](../todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md) (items 1–2 delivered by #590; item 3 open) | ✅ **RESOLVED HERE** | Spawn start lines carry `op` / `op_id` / `group`, and the resolving progress line names the op id (M5). Deleted at wrap |
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) — claimed by node 7 (`move-widen`) | ⚠ partial (item 2) | The untracked-source preflight (M2). Node 7 wraps first and narrows the entry to items 1–2; this node's wrap removes item 2 and leaves item 1 (`move_items_to_crate`, deferred) |
| [2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md](../todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) — claimed by node 3 (`tidy-facades`) | ⚠ partial (item 4) | The `StaleOperation` refusal says a run of the plan wrote it (M6). The wrap removes item 4 from whatever node 3's wrap left |
| [2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) — claimed by node 4 (`extract-method-clean`) | ⚠ partial (item R, the generic wait only) | Discovery shows R's *hang* is already a 30 s refusal for extractions since #542 (`rust.rs:1547`). This node bounds every *other* `await_answer` caller. Making a range that starts on `{` or a comment succeed stays node 4's job. The wrap adds one line to R saying so |
| [2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md) | — Unrelated | v1 range anchors. This node changes only the stale refusal's text |

## Affected Packages

- **`tddy-code-restructuring`**: `src/apply.rs`, `src/runner.rs` (`commit_operation`), `src/runner/entry_points/store_run.rs`, `src/runner/rehearsal.rs`, `src/runner/entry_points/check_entry_points.rs` (one chained call), `src/runner/compile_gate.rs`, `src/runner/resume.rs`, `src/runner/group_gate.rs`, `src/overlay.rs`, `src/backends/rust.rs` (builder plus seven call sites switched to the bounded wait), `src/backends/rust/readiness.rs`, `src/backends/rust/wait.rs`, `src/spawn_record.rs`, `src/spawn_record/jsonl.rs`, `src/lib.rs`; tests listed below; `docs/readiness-and-gates.md`, `docs/test-binary-moves.md` at wrap.
- **`tddy-lsp`** (developer consent 2026-10-09): `src/spawn_observer.rs` (`ProcessStart::operation`, `OperationContext`), `src/server_body.rs:129` (`operation: None`), `src/lib.rs` (re-export), `tests/spawn_observer_test.rs` (constructor), `tests/bin/fake_lsp.rs` (`--narrates-while-loading`).
- **`tddy-index-daemon`** (developer consent 2026-10-09): `src/status.rs` `status_of` (two new arms plus unit tests). Its apply loop needs no change because it commits through `commit_operation`.

## Related Feature Documentation

- PRD: [PRD-2026-10-09-reshape-apply-robust.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-apply-robust.md)
- Feature: [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md). Sections updated at wrap: Refusal classes, the readiness section ("A wait that lasts says so" — **"it adds no deadline" is rewritten**), the compile-gate text, the spawn record.

## Summary

`apply` gets five refusals or facts it was missing:

1. An untracked `Rename` source is refused before its operation writes anything.
2. The baseline `cargo check` includes each cross-crate move's destination.
3. No wait for type inference outlasts a ready server's silence (30 s) or a loading server's silence (10 min). Both bounds are injectable.
4. Spawn records name the operation or group that started them.
5. A `StaleOperation` refusal says when a run of the same plan wrote the plan back, and whether that run's edits were undone.

## Background

See the PRD's Background. The defects came from `#unbundle` 4 (`git mv` of an uncommitted file), `#carve` 15 (destination test binary broken on `HEAD`), `#carve` 17 stage B2 (a rename apply stuck for 10+ min at 0% CPU) and extract item R (same signature, daemon-wide queueing), `#carve` 17 (daemon killed, no op in the record) and `#live-plan` 7 ("item changed" after a rollback).

## Responsibility

This PR owns **apply-path robustness** in `tddy-code-restructuring`, as five independent fixes on the run's commit, gate, wait, record and refusal paths:

- **R1** The untracked-move preflight: `apply::untracked_move_sources`, the `RestructureError::UntrackedMoveSource` variant, and its three call paths (commit, dry run, rehearsal) plus the static anchor check.
- **R2** Destination packages in `refuse_a_broken_baseline`.
- **R3** Silence bounds on every `await_answer` caller: `SilenceBounds`, `RestructureError::ServerStalled`, the heartbeat's ready wording, and the B2 reproduction record.
- **R4** Operation context on spawn records (`tddy_lsp::OperationContext`, `SpawnRecorder::for_operation` / `for_group`) and the op id in the resolving progress line.
- **R5** `WrittenByRun` context on `StaleOperation`.

### The rules (the contract)

- **P1 (preflight timing).** For each operation, `untracked_move_sources` runs on the resolved edit **before** `GroupRun::enter` and before the journal's `in_flight` record. A refusal therefore leaves the operation's files and the journal exactly as the previous operation left them. The next run of the same plan (fresh or `--resume`) is not refused as indeterminate.
- **P2 (what is tracked).** A path is tracked when `git ls-files --error-unmatch -- <path>` succeeds (an intent-to-add or a staged rename counts), when it is a `Create` path of the same edit, or when the run's `Overlay` introduced it (dry run and rehearsal, where nothing reached disk).
- **P3 (refusal).** `UntrackedMoveSource { op, paths }` reads: `operation {op} moves {paths}, which git does not track — a move goes through \`git mv\` so history survives. \`git add\` it and apply again. Nothing of operation {op} was written.` Over the wire it is `FailedPrecondition`.
- **P4 (destination).** `move_module_to_crate`, `move_cluster_to_crate` and `move_test_binary_to_crate` add the `[package] name` of `<to>/Cargo.toml` to the baseline's packages. A `to` with no readable manifest adds nothing and raises nothing here. Other kinds' `to` is ignored.
- **P5 (ready silence).** Every `await_answer` caller passes `bounds.ready` (default `READY_HOVER_BOUND`, 30 s). When the index is ready (`!loading`) and the hover stays `null` past it, with no `inactive-code` / `unlinked-file` diagnostic, the refusal is the existing `ServerDefect` "… cannot type this position", naming `file:line`. The unbounded `wait_until_resolved` / `wait_until_answerable` variants are removed.
- **P6 (loading silence).** While `chatter.loading()`, the wait computes how long the server's last words have been unchanged (`chatter` already tracks this for the heartbeat). Past `bounds.loading` (default `LOADING_SILENCE_BOUND`, 10 min) the wait is refused as `ServerStalled { stage, server, last, quiet_seconds, furthest }`, over the wire `DeadlineExceeded`. Any new word from the server resets the clock. No bound applies to total wait time.
- **P7 (heartbeat wording).** A beat for a ready server with no answer says `is ready and gives no answer here`. `has not said it is ready` is reserved for a server that never reported a status.
- **P8 (operation context).** `commit_operation` starts its `git` processes through `spawns.for_operation(index, id)`. `GroupRun::settle` starts the group's gate through `spawns.for_group(name)`. The JSONL `start` line gains `"op"`, `"op_id"` and `"group"` only when present; existing keys are unchanged. Run-level processes carry none of them.
- **P9 (progress line).** The command line's `op {index} of {total}: resolving …` (`store_run.rs:300-304`) becomes `op {index} ({id}) of {total}: resolving …` when the op has an id. The index daemon reports operations as structured events that already carry the op (`tddy-index-daemon/src/apply.rs` `operation_event`), so it has no such line to change. The account line is unchanged.
- **P10 (stale context).** `refuse_a_stale_pending_op` reads `resume::written_by_run(&journal, root)`. When the journal has a completed operation followed by a `PlanSynced` record, `StaleOperation.written_by` is `Some(WrittenByRun { last_applied, last_applied_id, journal, undone })`. `undone` is true when every completed record's touched files hash to that record's `pre`. Its text appends: `— this plan file was last written by a run of it, which applied operation {n} ({id}) (journal: {dir}){; its edits have since been undone}. Regenerate the plan with \`restructure anchors\` rather than re-anchoring by hand.` With no journal, `written_by` is `None` and the text is unchanged.

## Boundaries

- Does **not** auto-`git add`, fall back to `fs::rename`, or reorder creates, changes and renames (F2).
- Does **not** add a total deadline. A narrating load is still waited on until cancellation, and `a_wait_has_no_deadline_however_many_beats_pass` passes unchanged.
- Does **not** give rust-analyzer a separate `cargo.targetDir`. If M1 finds `target/` lock contention, that goes to the developer (F5).
- Does **not** extend the baseline to crates a move re-points (todo `2026-10-09-restructure-baseline-omits-crates-a-move-re-points.md`).
- Does **not** change `check`'s stale finding or `ItemChanged`'s text (todo `2026-10-09-restructure-check-does-not-say-a-run-wrote-a-stale-plan.md`), and does **not** keep a pre-run copy of the plan (todo `2026-10-09-restructure-keep-the-plan-as-it-was-before-a-run.md`).
- Does **not** make extract-method ranges that start on `{` or a comment succeed (node 4).
- Does **not** grow functions on the >60-line list (`apply_held_plan` 159, `check_plan` 115, `await_answer` 60, `request` 64). New logic goes into new functions, and the touched blocks are replaced by calls (see Technical changes).

## Dependencies

This node has no parent: its base `feature/reshape/new-crate` is a line predecessor only, and nothing here consumes node 1–9 behaviour. Node 9's new-crate case is met by P4's "no manifest adds nothing", which holds with or without node 9.

## Draft PR contract

The first push of this PR (wave 2 of planning) publishes the owned surface with `todo!()`-free stubs that compile, plus the failing tests below. It is not the deliverable.

Owned surface:

- `tddy_code_restructuring::apply::untracked_move_sources(root: &Path, edit: &WorkspaceEdit, overlay: &Overlay, spawns: &SpawnRecorder) -> Result<Vec<String>>`
- `tddy_code_restructuring::Overlay::introduced(&self, path: &str) -> bool`
- `RestructureError::UntrackedMoveSource { op: usize, paths: Vec<String> }`
- `RestructureError::ServerStalled { stage: String, server: String, last: String, quiet_seconds: u64, furthest: String }`
- `RestructureError::StaleOperation { plan, op, reason, written_by: Option<Box<WrittenByRun>> }` (boxed: `RestructureError` is at clippy's `result_large_err` limit) and `pub struct WrittenByRun { pub last_applied: usize, pub last_applied_id: Option<String>, pub journal: String, pub undone: bool }` (in `lib.rs`, with `Display` producing P10's text)
- `tddy_code_restructuring::backends::rust::{SilenceBounds, READY_HOVER_BOUND, LOADING_SILENCE_BOUND}` with `pub struct SilenceBounds { pub ready: Duration, pub loading: Duration }` and `impl Default`, plus `RustBackend::with_silence_bounds(self, bounds: SilenceBounds) -> Self`
- `tddy_lsp::OperationContext { pub op: Option<usize>, pub op_id: Option<String>, pub group: Option<String> }` and `ProcessStart::operation: Option<OperationContext>`
- `SpawnRecorder::for_operation(&self, op: usize, op_id: Option<&OpId>) -> SpawnRecorder` and `SpawnRecorder::for_group(&self, group: &str) -> SpawnRecorder`
- `runner::resume::written_by_run(journal: &Journal, root: &Path, paths: &StatePaths) -> Result<Option<WrittenByRun>>` (`pub(super)`)
- `runner::compile_gate::baseline_packages(root: &Path, plan: &Plan) -> Result<BTreeSet<String>>` (`pub(super)`)
- `fake_lsp --narrates-while-loading`
- Published beyond the list above (commit 2): `backends::rust::readiness::{Silence, silence_verdict(loading: bool, ready_silent_for: Option<Duration>, words_unchanged_for: Duration, bounds: &SilenceBounds) -> Silence}` (`pub(super)`, the pure decision tests 17–18 pin), and the `RustBackend.silence_bounds` field. `tddy-index-daemon`'s `status_of` arms were implemented with the variants, because the match is exhaustive and a stub arm would be a fallback.
- Uncalled surface carries `#[allow(dead_code, reason = "TODO(reshape-apply-robust)…")]` (`silence_bounds`, `Silence`, `silence_verdict`, `written_by_run`, `baseline_packages`). Green removes each when it wires the caller.

Failing tests: acceptance tests 1–28 below. Each is red on `master` for the reason given against its group.

## Green wave

**Wave 1 of 4.** It can go green independently, concurrently with nodes 1–12 (textual collisions only: node 1 in `apply.rs`, and shared `backends/rust.rs` wiring). It blocks nothing; there are no real edges in or out.

## Successor PRs

None depend on this node. The line successor is `feature/reshape/move-item-paths` (K=11), sequential only.

## Scope

- [ ] M1: reproduce the B2 hang and record its shape
- [ ] M2: untracked-source preflight (commit, dry run, rehearsal, static)
- [ ] M3: destination packages in the baseline
- [ ] M4: silence bounds, `ServerStalled`, heartbeat wording, fake mode, daemon status
- [ ] M5: operation context on spawn records, op id in the resolving line
- [ ] M6: `WrittenByRun` on `StaleOperation`
- [ ] Docs at wrap (feature doc sections above, `readiness-and-gates.md`, `test-binary-moves.md`, skill § Testing cadence)

## Technical changes

### State A (Current)

- `apply.rs:18-39` applies creates, then text, then `git mv`. An untracked source fails after the text is on disk (`apply.rs:158-163`, reported as `MalformedPlan`, `:196-208`). `runner.rs:77-83` has already journalled `in_flight`, so the next open refuses `IndeterminateJournal` (`journal.rs:323-340`, `runner.rs:186-188`). There is no `ls-files` on the apply path; the dry run (`store_run.rs:322-335`) and rehearsal (`rehearsal.rs:30-97`) never check.
- `compile_gate.rs:61-65` builds the baseline from snapshot keys and anchor files only; `op.to` is unread.
- `readiness.rs:147-206` `await_answer` is bounded only when called through `wait_until_resolved_within_bound` (`rust.rs:1547`). It is unbounded at `rust.rs:1814, 1960, 2151, 2196, 2295` and `signature.rs:146`. In the loading state it is never bounded (`readiness.rs:187-201`). `wait.rs:9` says "adds no deadline"; `wait.rs:134-138` words a ready server as "has not said it is ready".
- `spawn_record/jsonl.rs:128-140` start lines have no operation. `ProcessStart` (`tddy-lsp/src/spawn_observer.rs:24-37`) has no field for one.
- `store_run.rs:417-451` raises `StaleOperation { reason: "item changed" }` with no journal context, though it already loads the journal (`:432-434`).

### State B (Target)

As in Responsibility rules P1–P10.

### Delta (What's Changing)

- `apply.rs`: new `untracked_move_sources` (one `git ls-files --error-unmatch -z -- <paths…>` per edit with renames, then per-path for the error case). It sits **above** `git_move`, away from node 1's addition below it.
- `runner.rs` `commit_operation`: calls nothing new. The preflight is called by the loops (P1) through a new `runner::preflight_moves(index, &resolved, root, &overlay, &spawns) -> Result<()>` placed beside `commit_operation`. `commit_operation` passes `spawns.for_operation(index, id)` to `apply_workspace_edit`.
- `store_run.rs` `apply_held_plan` (159 lines, on the list): the dry-run block (`:322-335`) is extracted to `rehearse_dry_run(...)`, which also calls `preflight_moves`. The commit path gains one `preflight_moves` call before `GroupRun::enter`. Net, the function shrinks. The resolving `format!` (`:300-304`) is edited in place for P9.
- `rehearsal.rs`: after resolution, `untracked_move_sources(.., &self.overlay, ..)` → `refusal: Some(UntrackedMoveSource…)`.
- `check_entry_points.rs` `check_plan` (115, on the list): the `crate_move::unrunnable(...)?` loop's iterator gains `.chain(untracked_move_anchors(root, &plan.ops, &options.spawns)?)`, with no added lines beyond formatting. The new `untracked_move_anchors` lives in `runner/rehearsal.rs` (the four kinds' anchor files, plus `also`).
- `compile_gate.rs`: `refuse_a_broken_baseline` calls new `baseline_packages`, which unions `owning_packages` with destination packages read via `Destination::read`, skipping a missing manifest.
- `readiness.rs`: `await_answer`'s silent-state block (`:187-201`) is replaced by a call to new `fn silence_verdict(&self, silent: &mut SilenceClock, uri, position) -> Result<Option<RestructureError>>`. That brings `await_answer` under 60 lines and puts P5 and P6 in one testable place. `wait_until_resolved` / `wait_until_answerable` take the backend's `bounds`; `wait_until_resolved_within_bound` is removed (redundant).
- `backends/rust.rs`: a `bounds: SilenceBounds` field plus `with_silence_bounds`. The call sites keep their names (now bounded).
- `wait.rs`: the module doc drops "It adds no deadline" for "It adds no *total* deadline; `readiness` bounds silence". There is a third `heartbeat_line` state (P7).
- `spawn_record.rs`: an `operation: Option<OperationContext>` field on `SpawnRecorder`, `for_operation` / `for_group`, and `process_start` filling it. `jsonl.rs` writes the three keys when present.
- `group_gate.rs` `settle`: its gate uses `spawns.for_group(name)`.
- `resume.rs`: `written_by_run`. `store_run.rs` `refuse_a_stale_pending_op` fills `written_by`.
- `lib.rs`: two new variants, the `StaleOperation` field, `WrittenByRun`.
- `tddy-lsp`: the field, the struct, `None` at `server_body.rs:129`, and the fake mode (narrates a new `$/progress` report every 100 ms while `serverStatus` stays not-quiescent).
- `tddy-index-daemon/src/status.rs`: `UntrackedMoveSource` → `failed_precondition`, `ServerStalled` → `deadline_exceeded`.

## Implementation milestones

- [ ] **M1 — reproduce B2 (manual, real workspace).** Warm `./run-index-daemon` on this checkout, hand-edit 7 files of one crate, and apply a one-op item-anchored `rename_symbol` whose header lists only the anchored file. Collect the heartbeat lines (stage / loading / unchanged-for), `<runtime>/tddy-index-<tag>.spawns.jsonl`, `ps -o pid,ppid,stat,etime,command` under the rust-analyzer pid, and `lsof target/debug/.cargo-lock`. Make at most three attempts. Record in **Validation Results**: ready-and-silent (P5 closes it), loading-and-silent (P6 bounds it; if `target/` contention shows, raise F5), or not reproduced. M2–M6 do not wait on M1's outcome.
- [ ] **M2** — `untracked_move_sources`, `Overlay::introduced`, `preflight_moves`, `rehearse_dry_run`, rehearsal and static parity, the variant. Tests 1–8.
- [ ] **M3** — `baseline_packages`. Tests 9–11.
- [ ] **M4** — `SilenceBounds`, `silence_verdict`, call sites, heartbeat wording, fake mode, `ServerStalled`, daemon `status_of`. Tests 12–20.
- [ ] **M5** — `OperationContext`, recorder scoping, JSONL keys, the resolving line. Tests 21–25.
- [ ] **M6** — `written_by_run`, `WrittenByRun`. Tests 26–28.

## Testing plan

### Testing Strategy

- Library level throughout. The git fixture is `tests/harness/mod.rs`'s `a_workspace_with_a_test_binary` (+ `.tracked_by_git()`), driven by `moving_the_test_binary_recording` over `fake_lsp`: a test-binary move asks the server nothing, so the preflight, the gate and the spawn record need no rust-analyzer. New harness helpers (as written in commit 2): `a_workspace_whose_test_binary_is_untracked()` (the builder's `untracked(path)` takes the file back out of the index), `DESTINATION_LIB` (tests 9–10 break it with `rewriting`), `checking_a_move_of_the_test_binary(fixture, deep) -> Result<Vec<String>, String>`, `moving_the_test_binary_in_a_group_recording(fixture, group, spawns)`, and the shared plan writer `the_test_binary_move_plan(root, group)`.
- Wait tests use the `RustBackend` built over `fake_lsp` with injected `SilenceBounds` and heartbeat (as `wait_heartbeat_acceptance.rs` does): `--cold-hovers 1000000 --loads-crate-graph` for ready-silent, `--goes-busy-after-hovers 1` for loading-silent at the type-inference stage (`--never-quiescent` stalls in the warm-up instead), `--narrates-while-loading` for the guard. No test branches on being a test.
- No new live rust-analyzer suite (F3), so there are no `.config/rust-e2e.filterset` / nextest-group changes.
- Fluent-tests style: Given/When/Then, no conditionals, one behaviour per test.

### Coverage Requirements

- Every new refusal is proved through the library entry point (`runner::apply` / `check`), not only at unit level.
- Each bound gets a positive test (refused) and a guard (not refused).
- Parity is shown across apply, dry run, `check --deep` and static `check` for the preflight.

## Acceptance tests

Names read as behaviour specifications. Items 1–28 are **red on `master`** except the green pins 15, 19, 20, 22, 25, 27 (see Validation Results); the reason is given per group. The guard `a_wait_has_no_deadline_however_many_beats_pass` (`tests/wait_heartbeat_acceptance.rs:343`) stays green, unchanged.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/apply_preflight_acceptance.rs` (new; `fake_lsp`, git fixture)

Red on `master`: the apply writes the text edit, then fails with `plan is malformed: git mv … not under version control`. The dry run and both checks report nothing.

1. `refuses_to_move_a_test_binary_git_does_not_track_naming_it_and_git_add`
2. `leaves_every_file_byte_identical_when_an_untracked_move_is_refused`
3. `applies_the_same_plan_once_the_file_is_added_without_resume_or_clean_up`
4. `a_dry_run_refuses_an_untracked_move_the_same_way`
5. `a_deep_check_reports_an_untracked_move_as_a_finding`
6. `a_static_check_reports_an_untracked_anchor_file`
7. `the_resolving_line_names_the_operations_id` (red: the line has the index only; this is P9, M5)

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/apply.rs` (unit, `#[cfg(test)] mod tests`)

Red on `master`: `untracked_move_sources` does not exist.

8. `counts_a_file_the_engine_created_in_the_same_run_as_tracked_and_names_one_git_has_never_seen`

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/apply_compile_gate_acceptance.rs` (extended)

Red on `master`: the baseline checks `-p origin` only and passes, then the post gate blames the plan.

9. `refuses_to_apply_a_move_whose_destination_did_not_compile_before_the_plan` — the refusal starts `the tree does not compile before the plan runs: \`cargo check --all-targets -p destination -p origin\` fails`.
10. `writes_nothing_when_the_destination_did_not_compile_before_the_plan` — no `.restructure`, test binary still in `crates/origin/tests/`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/runner/compile_gate.rs` (unit)

11. `a_destination_without_a_manifest_adds_no_package_to_the_baseline` (red: `baseline_packages` does not exist)

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/silent_server_acceptance.rs` (new; `fake_lsp`, injected bounds)

Red on `master`: a rename waits until cancelled (`IndexingIncomplete`), there is no `ServerStalled`, and the beat says "has not said it is ready".

12. `a_rename_at_a_position_a_ready_index_never_types_is_refused_after_the_ready_bound_naming_the_position`
13. `a_beat_for_a_ready_server_that_gives_no_answer_says_it_is_ready_and_gives_no_answer`
14. `a_loading_server_silent_past_the_stall_bound_is_refused_as_stalled_naming_the_stage_and_its_last_words`
15. `a_loading_server_that_keeps_narrating_is_never_refused_as_stalled` (`--narrates-while-loading`, loading bound 300 ms, 3 s elapsed: still waiting; cancellation then ends it as `IndexingIncomplete`)
16. `a_reference_survey_at_an_untypable_position_is_refused_after_the_ready_bound` (`references_at`)

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/backends/rust/readiness.rs` (unit, over `silence_verdict`'s pure decision)

Red on `master`: no `silence_verdict`.

17. `a_loading_server_whose_words_are_unchanged_past_the_loading_bound_is_stalled`
18. `a_loading_server_with_new_words_inside_the_bound_is_waited_on`

### `tddy-index-daemon` — `packages/tddy-index-daemon/src/status.rs` (unit)

Red on `master`: the variants do not exist.

19. `an_untracked_move_source_is_a_failed_precondition`
20. `a_stalled_server_is_a_deadline_exceeded`

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/spawn_record_acceptance.rs` (extended)

Red on `master`: `ProcessStart` has no `operation`.

21. `an_apply_records_the_operation_each_git_process_ran_for` — `git mv` start carries `op: Some(0)` and the store-assigned `op_id`.
22. `run_level_processes_carry_no_operation` — the baseline and result `cargo check`, and `git rev-parse`.
23. `a_groups_compile_gate_is_recorded_with_its_group` — a one-member group `g`; its gate's `cargo check` carries `group: Some("g")`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/spawn_record/jsonl.rs` (unit)

24. `a_start_line_carries_op_op_id_and_group_when_the_process_was_started_for_one`
25. `a_start_line_for_a_run_level_process_has_no_op_keys`

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/runner/entry_points/store_run.rs` (unit, existing `a_workspace_holding` fixture plus a hand-written journal)

Red on `master`: `StaleOperation` has no `written_by`, and the text says only `(item changed) — re-anchor it`.

26. `a_stale_plan_a_run_wrote_back_and_whose_edits_were_undone_says_so_and_to_regenerate_it`
27. `a_stale_plan_with_no_journal_keeps_todays_refusal`

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/runner/resume.rs` (unit)

28. `written_by_run_reports_undone_only_when_every_touched_file_is_back_at_its_pre_hash`

## Technical Debt & Production Readiness

- **Bounds as constants.** `READY_HOVER_BOUND` and `LOADING_SILENCE_BOUND` are injected through the builder, but not configurable from the daemon's YAML or the CLI. That is deliberate: nothing asked for it, and a flag invites tuning around a defect. Raise it only if a real load exceeds 10 min of silence.
- **One extra `git` process per renaming operation.** This is negligible next to the `git mv` itself.
- **The operation context mostly tags `git` lines** and a group's gate, because those are the only processes started inside an operation. rust-analyzer's own children stay invisible (unchanged, documented).
- No `FIXME`/`TODO` markers planned in production code.

## Decisions & Trade-offs

All approved by the developer on 2026-10-09 (each was the recommendation).

- **F1 — loading-silence bound: approved, 10 min, injectable, refused as `ServerStalled`.** This **reverses `#sharpen` 4/8's "heartbeat, no deadline" decision, with the developer's explicit consent**, for *silence while loading* only. A total deadline stays rejected (the reasoning in the feature doc's readiness section stands: a large workspace legitimately indexes for 20+ minutes while narrating). The guard test stays green unchanged. The feature doc's "it adds no deadline" sentence is rewritten at wrap. Alternatives rejected: no bound (leaves a daemon-wide outage possible), a caller-stated deadline (no caller states one in practice).
- **F2 — untracked source: refuse.** An automatic `git add` would stage the author's file without consent, which is a fallback.
- **F3 — no live rust-analyzer test.** `fake_lsp` reproduces every shape the bounds act on. The real-workspace reproduction is M1, recorded rather than automated.
- **F4 — operation context as a `ProcessStart` field** (`tddy-lsp` edit consented), rather than a second observer event: two places construct it and the trait is unchanged.
- **F5 — separate `cargo.targetDir` for rust-analyzer: not decided here.** It goes to the developer only if M1 shows `target/` lock contention.
- **F6 — stale context on `StaleOperation` only.** `stale_findings` is public and called by the daemon's `check`, outside the consented daemon edit, and a re-run meets `StaleOperation` first. Deferred to `2026-10-09-restructure-check-does-not-say-a-run-wrote-a-stale-plan.md`.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
- (none yet)
### From @red (TDD Red Phase)
- (none yet)
### From @validate-changes (Change Validation)
- (none yet)
### From @validate-tests (Test Quality)
- (none yet)
### From @prod-ready (Production Readiness)
- (none yet)
### From @analyze-clean-code (Code Quality)
- (none yet)
### From @refactor (Completed Refactorings)
- (none yet)

## Validation Results

- M1 reproduction: _pending_ (green).
- **Commit 2 (draft-PR contract), 2026-10-09.** Scoped: `cargo check` / `cargo clippy --all-targets -D warnings` for `tddy-code-restructuring`, `tddy-lsp`, `tddy-index-daemon` clean; `cargo fmt --all --check` clean; `tddy-lsp` 66 passed; `wait_heartbeat_acceptance` 7 passed (the no-deadline guard unchanged).
- **Red, each because the implementation is missing:** 1–7 (the apply fails at `git mv` as `plan is malformed`, the journal keeps an in-flight record so the re-run is `JournalExists`, the dry run and both checks report nothing, the resolving line has no id); 8, 11, 17, 18, 28 (`todo!()` in the stub); 9–10 (the baseline checks `-p origin` only, the post gate blames the plan); 12, 16 (cancelled as `IndexingIncomplete` at `type inference at src/lib.rs:N`: no ready bound on those waits); 13 (the type-inference beat says `has not said it is ready`); 14 (no `ServerStalled`: the wait runs on until the fake's next poll and the rename ends `no edits`); 21, 23 (`ProcessStart.operation` is never set); 24 (the JSONL line has no op keys); 26 (the refusal has no run context).
- **Green pins** (specify what already holds and must keep holding): 15 (a narrating load is not refused), 19–20 (status classes, implemented with the variants), 22 and 25 (run-level processes carry no operation), 27 (no journal, unchanged text).
- **Implementation constraint found while writing 14:** `fake_lsp`'s busy report is followed, at the next 2 s poll, by the shared client folding the earlier `quiescent: true`. So the loading verdict has to be evaluated in the wait's sleep slices (where the heartbeat beats, every 100 ms), not only once per poll.
- Seen and not ours: `apply_compile_gate_acceptance::a_failed_apply_says_the_tidy_did_not_run_and_how_many_unused_imports_it_left` (node 3's contract) and `apply_tidy_acceptance::{removes_an_unused_mut_from_a_file_the_run_wrote, says_which_file_it_removed_an_unused_mut_from}` (node 4's contract) are red on this branch's base.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-apply-robust-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-apply-robust.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create failing acceptance tests (commit 2: owned surface + tests 1–28)
- [x] Run acceptance tests (verify they fail) — see Validation Results: 22 red, 6 green pins
- [ ] USER REVIEW — acceptance tests
- [ ] M1 — reproduce the B2 hang and record its shape
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring -p tddy-lsp -p tddy-index-daemon`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring -p tddy-lsp -p tddy-index-daemon --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review. Deletes the three ✅ entries (recording the M1 result in the change history), narrows the 09-09 entry (item 2), the leftovers entry (item 4) and item R, and deletes `2026-10-09-reshape-apply-robust-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
