# `restructure apply` refuses before it half-moves a file, checks the destination first, and never waits on a silent server - PRD

**Date**: 2026-10-09
**PRD Type**: Bug fix (engine robustness)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `### Refusal classes` (two new refusals), the readiness section ("A wait that lasts says so, on a fixed heartbeat" / "Ready means quiescent and healthy": the wait gains two bounds), the compile-gate text (the baseline covers a move's destination), and the spawn-record section (start lines name their operation).

No other feature document changes. No new plan field, no new CLI flag, no new wire message.

## Summary

Five ways an `apply` goes wrong today without saying why, each closed with a refusal that names what it found:

1. A move whose source file git has never seen is refused **before the operation writes anything**, saying to `git add` it. Today the text edits land, `git mv` fails, and the journal can no longer be resumed.
2. The baseline `cargo check` that runs before a fresh apply includes the **destination crate** of every cross-crate move, so a destination that was already broken is reported as already broken.
3. A wait for rust-analyzer can no longer go on for ever when the server has gone silent. A ready index that will not type the position is refused after a bound, as extractions already are. A server that says it is loading and then says nothing new for a long bound is refused too, naming the stage, its last words and how long they have been unchanged.
4. Every process started for a plan operation is recorded with that operation's index and id.
5. A plan refused as stale after its own failed run was rolled back says so: a run of this plan rewrote it, and it has to be regenerated.

## Background

The defects come from real runs:

- `#unbundle` 4: `git mv … failed: fatal: not under version control` on a file written earlier in the same PR, with the destination's `lib.rs` already edited. Today the failure is also reported as `plan is malformed`, and the next run refuses with `IndeterminateJournal`.
- `#carve` 15: a destination test binary that did not compile on `HEAD` was missed by the pre-apply gate and then counted among the plan's errors by the post-apply gate.
- `#carve` 17, stage B2: a one-op `rename_symbol` sat at "waiting for type inference at the anchor" for more than ten minutes at 0% CPU, after `check --deep` of the same plan had answered. Extract item R (#524) had the same signature, and every later request to the daemon queued behind it. The heartbeat added afterwards (#591) shows such a wait but deliberately does not end it.
- `#carve` 17: the daemon died while endpoint protection blocked "a script". The spawn record (#590) now says which processes ran, but not which plan operation started them.
- `#live-plan` 7: a plan re-run after a rolled-back failure was refused with "item changed", which sends the author looking for an edit they never made.

The code confirms each of these on `master`. See the discovery's Exploration 2 for file and line references.

## Proposed Changes

### What's Changing

**1. Untracked move sources are refused up front.**
- Before an operation writes anything (no journal record, no text edit), every `Rename` source in its resolved edit is checked with git. A source that git does not track is refused: `UntrackedMoveSource`, naming each path, the operation, and the remedy (`git add` it, then apply again). The refusal says nothing of this operation was written. It is a tree problem, not a plan problem: the class is the tree-state one (`FailedPrecondition` over the wire).
- A file the engine created earlier in the run (`git add -N`) and a file an earlier operation moved both count as tracked. In a dry run and in `check --deep`, which write nothing, a path the run's overlay created or renamed counts as tracked.
- `apply --dry-run` and `check --deep` give the same refusal for the same operation. Static `check` reports it for the anchor files of the operations that rename files (`move_module_to_crate`, `move_cluster_to_crate` including `also`, `move_test_binary_to_crate`, `reparent_module`).
- Earlier operations of the run that already committed stay committed, as with every other per-operation refusal. A group is rolled back whole, as today.

**2. The baseline covers each cross-crate move's destination.**
- For `move_module_to_crate`, `move_cluster_to_crate` and `move_test_binary_to_crate`, the package that `to` declares joins the packages the pre-apply `cargo check --all-targets` covers. The refusal's command line lists it, for example `-p destination -p origin`.
- A `to` with no manifest yet adds nothing and is not an error here. That is the new-crate case, which the operation itself decides.

**3. A silent wait ends in a named refusal.**
- *Ready but untypable.* Every wait for type inference gets the ready-index bound (`READY_HOVER_BOUND`, 30 s) that extractions have had since #542. That covers renames, symbol anchors, the post-assist rename, `extract_module` with `to_file`, the reference survey and the signature rewrites. The refusal is the existing unusable-answer class and names the position. The heartbeat stops calling such a server "not said it is ready" and says it is ready but gives no answer here.
- *Loading and silent.* While the server says it is loading, a wait whose last words have not changed for a **stall bound** is refused with a new `ServerStalled`. It names the stage, the server (its pid or "behind a shared client"), its last words, how long they have been unchanged and the furthest phase. It goes over the wire as `DeadlineExceeded`, like the other "the index did not get there" refusals. A server that keeps narrating is waited on for as long as it keeps narrating; this is not a total deadline. The bound is injected the way the heartbeat cadence is, so tests shorten it without branching. Default: **10 minutes** of unchanged words. This reverses `#sharpen` 4/8's "heartbeat, no deadline" decision, with the developer's consent (2026-10-09), for silence only; a total deadline stays out.
- *Root cause.* The node reproduces the B2 hang on the real workspace before writing the bounds. It warms the daemon, edits several files of one crate, and applies a one-op item-anchored `rename_symbol`, collecting the heartbeat lines, the spawn record, rust-analyzer's process tree and the holder of `target/debug/.cargo-lock`. The changeset records which shape it was: ready-and-silent, loading-and-silent, or neither. If it was loading-and-silent because of contention on `target/` with the engine's own `cargo check`, the developer decides whether rust-analyzer gets a separate `cargo.targetDir`. That is a separate decision, not a quiet change.

**4. Spawn records name their operation.**
- A process started while an operation is committed (its `git` calls) carries `op` (the plan index) and `op_id` (when the plan has ids) on its `start` line. A group's end-of-group `cargo check` carries `group`. Run-level processes (the baseline, the result gate, the tidy and the language server) carry none of these. Both apply loops, the command line's and the daemon's, get this through the shared commit path.
- The command line's per-operation progress line "op N of M: resolving …" also names the operation id when the plan has one, so a progress line, a spawn record line and a journal record can be joined on a key that survives reordering the plan.

**5. A stale plan says a run wrote it.**
- When an apply is refused because an operation is stale (`StaleOperation`, raised before anything is read by both the command line's and the daemon's loop), and this plan's journal shows a run of it committed operations and wrote the plan back, the refusal adds three things. It says the plan file was last written by that run, naming the last operation it applied and the journal directory. If every file that run touched is back at its pre-run content, it says the run's edits have been undone. And it gives the remedy: regenerate the plan from `restructure anchors` / `snapshot` instead of re-anchoring by hand.
- `check`'s stale finding and the later `ItemChanged` raised while lowering anchors keep their wording. A re-run hits `StaleOperation` first, and `stale_findings` is shared with the daemon's `check` (deferred, see the todo `2026-10-09-restructure-check-does-not-say-a-run-wrote-a-stale-plan.md`).
- With no journal (it was removed), the message stays as it is today.

### What's Staying the Same

- `git mv` stays mandatory. There is no fallback to `fs::rename` and no automatic `git add` of the author's file (Decision F2).
- The order inside one edit stays creates, then text changes, then renames.
- The heartbeat cadence and its line format, apart from the ready-but-untypable wording. Cancellation still ends any wait at once. A wait on a loading server that keeps reporting progress is still unbounded, and `a_wait_has_no_deadline_however_many_beats_pass` stays green unchanged.
- The post-apply gate, the tidy, groups and their rollback. The baseline is still skipped for dry runs and resumed runs.
- The account line (`[i/n] op k: Kind -> f file(s) applied`), which every front end renders through one function.
- Stale detection itself. Only the message gains context.

## Impact Analysis

### Technical Impact
- `tddy-code-restructuring`: `apply.rs`/`runner.rs` (preflight), `runner/compile_gate.rs` (destination packages), `backends/rust/readiness.rs`, `wait.rs` and the backend's builder (bounds, injected), `spawn_record*` (operation context), the stale-op refusal path, `lib.rs` (two variants), and static/deep `check` parity.
- `tddy-lsp`: `ProcessStart` gains an optional operation context. The language server's own start leaves it empty. `fake_lsp` gains a mode that keeps narrating while loading.
- `tddy-index-daemon`: `status_of` classifies the two new variants. Its loop needs nothing else.
- Tests run against `fake_lsp` and a git fixture. A real rust-analyzer is used only for the manual reproduction recorded in the changeset; no new live binary is planned (Decision F3).

### User Impact
- An author who moves a file they just wrote gets one sentence telling them to `git add` it, and the tree is not edited. Before, they had to clean up a half-edited tree and remove the journal by hand.
- A broken destination is reported before the plan runs, not blamed on it afterwards.
- A wedged apply, and with it a wedged daemon, ends with a refusal that says what it waited for. The longest silent wait is now 30 s once ready and the stall bound while loading, instead of until somebody kills it.
- No breaking change for a plan that worked before. Two message texts change (the heartbeat's ready wording, and the stale refusal gaining context), and spawn record lines gain fields.

## Implementation Plan

1. Reproduce the B2 hang on the real workspace and record the shape (ready-and-silent, loading-and-silent, or neither) in the changeset.
2. Untracked-source preflight with its refusal variant, used in the commit path, the dry-run path and rehearsal, plus the static check of anchor files.
3. Destination packages in the baseline gate.
4. Bounds: the ready bound on every inference wait, made injectable; the stall bound and `ServerStalled`; the heartbeat wording; the `fake_lsp` narrating mode; daemon status classes.
5. Operation context on `ProcessStart` and the JSONL start line; the op id in the resolving progress line.
6. Journal-aware stale refusal.
7. Docs at wrap: the feature doc sections listed above, the `readiness-and-gates.md` and `test-binary-moves.md` package docs, and the skill's testing-cadence note.

## Acceptance Criteria

- [ ] Applying a test-binary move whose source file is untracked is refused with `UntrackedMoveSource` naming the path and `git add`. Every file is byte-identical afterwards and the journal holds no in-flight record. After `git add`, the same plan applies 1 of 1 with no `--resume` or journal clean-up ([Rust code restructuring](../rust-code-restructuring.md)).
- [ ] The same plan refuses identically under `apply --dry-run` and `check --deep`, and static `check` reports the untracked anchor file.
- [ ] A file the engine created earlier in the same run (`git add -N`) is moved without refusal.
- [ ] A fresh apply of a cross-crate move whose destination does not compile is refused before writing, and the refusal's command includes `-p destination`. A move to a `to` with no manifest is not refused by the gate.
- [ ] A rename at a position a ready index never types is refused after the injected ready bound with the unusable-answer class, naming the position. The heartbeat never says "has not said it is ready" about a ready server.
- [ ] A wait on a server that says it is loading and then says nothing new is refused after the injected stall bound with `ServerStalled`, naming the stage and last words. A server that keeps narrating is not refused within many stall bounds. `a_wait_has_no_deadline_however_many_beats_pass` passes unchanged.
- [ ] The daemon maps `UntrackedMoveSource` to `FailedPrecondition` and `ServerStalled` to `DeadlineExceeded`.
- [ ] In the spawn record of an applied test-binary move, the `git` start lines carry `"op":0` (and `op_id` when the plan has one), and the baseline and result `cargo check` lines carry neither.
- [ ] An apply of a plan whose run's edits were rolled back is refused with a `StaleOperation` message that says a run of this plan wrote it, names the last applied operation and says to regenerate the plan. Without a journal the message is unchanged.
- [ ] The B2 reproduction's outcome (shape found, or not reproduced after the recorded attempts) is in the changeset.
- [ ] Tests pass for `tddy-code-restructuring`, `tddy-lsp` and `tddy-index-daemon` (scoped; CI for the rest).

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-apply-robust.md` (decisions F1–F6)
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-apply-robust-initial-discovery.md`
- Backlog entries this resolves: [apply did not return after a clean deep check](../../../dev/todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md), [pre-apply gate omits the destination](../../../dev/todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md), [no record of what an apply executes](../../../dev/todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md) (item 3)
- Partly resolves (claimed elsewhere in `#reshape`): [first cross-crate move defects](../../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) item 2; [live-plan carve leftovers](../../../dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) item 4; [extract drops comments](../../../dev/todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) item R (the generic wait only)
- Package docs: `packages/tddy-code-restructuring/docs/readiness-and-gates.md`, `packages/tddy-code-restructuring/docs/test-binary-moves.md`
