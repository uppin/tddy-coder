# 2026-10-17 — apply refuses before it half-moves a file, checks the destination first, and never waits on a silent server

**Type:** Fix

`#reshape` 9/19 (PR [#607](https://github.com/uppin/tddy-coder/pull/607)), across
`tddy-code-restructuring`, `tddy-lsp` and `tddy-index-daemon`. Product summary:
[changelog entry](../../ft/coder/changelog/2026-10-17-reshape-apply-robust.md). State B is in
[Rust code restructuring](../../ft/coder/rust-code-restructuring.md) (§ Waiting, § Refusal classes,
§ Live plans, § What a run executes),
[readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)
(§ Silence is bounded, § The untracked-move preflight, § The compile gate),
[test-binary-moves.md](../../../packages/tddy-code-restructuring/docs/test-binary-moves.md) and
[code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md#refusals).

## What changed

- **Untracked-move preflight (M2).** `apply::untracked_move_sources`, `Overlay::introduced`,
  `runner::preflight_moves(index, id, resolved, root, overlay, spawns)` called right after
  resolution in both apply loops (`store_run.rs`, `tddy-index-daemon/src/apply.rs`), before
  `GroupRun::enter` and the `in_flight` record; `Rehearsal::resolve_and_preflight` for `check --deep`;
  `untracked_move_anchors` chained into static `check` through `static_move_findings`. New variant
  `RestructureError::UntrackedMoveSource { op, paths }`, `FailedPrecondition` over the wire.
- **Destination baseline (M3).** `compile_gate::baseline_packages` adds the `[package] name` of each
  cross-crate move's `to` (via `Destination::read`; a missing manifest adds nothing, an unreadable
  one is refused). Two pre-P4 tests had their expected command updated to `-p destination -p origin`
  (developer's decision), with the command string the only change.
- **Silence bounds (M4).** `SilenceBounds { ready, loading }` (`READY_HOVER_BOUND` 30 s,
  `LOADING_SILENCE_BOUND` 10 min), `RustBackend::with_silence_bounds`, the pure
  `silence_verdict` refused in one place (`refuse_silence`), `excused_silence`, and
  `keep_waiting_unless_stalled` judging the loading verdict in every 100 ms slice of both the wait for
  type inference and the crate-index warm-up. `wait_until_resolved_within_bound` removed;
  `wait_until_resolved` / `wait_until_answerable` keep their names and are bounded on silence. New
  variant `ServerStalled { stage, server, last, quiet_seconds, furthest }`, `DeadlineExceeded`. The
  heartbeat's third state, `is ready and gives no answer here`. `chatter.rs`'s `NOTHING_REPORTED`
  is shared by the heartbeat and `ServerStalled`. `fake_lsp` gained `--narrates-while-loading`, and
  `--goes-busy-after-hovers N`'s busy report supersedes a narrated crate-graph load under one lock
  without holding the triggering hover.
- **Operation context (M5).** `tddy_lsp::OperationContext { op, op_id, group }` and
  `ProcessStart::operation` (`None` from the language server's own start);
  `SpawnRecorder::for_operation` / `for_group`; `commit_operation` applies through
  `spawns.for_operation(index, id)` and `GroupRun::settle` gates through `spawns.for_group(name)`; the
  JSONL `start` line writes `op` / `op_id` / `group` only when present; the CLI's resolving line is
  `op {index} ({id}) of {total}: resolving …` (`resolving_line`).
- **Stale-plan context (M6).** `resume::written_by_run` and `WrittenByRun { last_applied,
  last_applied_id, journal, undone }` on `StaleOperation.written_by` (boxed). `undone` compares each
  touched file with the `pre` of the earliest completed record naming it, and is true only when at
  least one file was compared and all matched.
- **Daemon.** `status_of` maps the two new variants; the apply loop calls `preflight_moves`
  (consent extended 2026-10-17), with
  `code_index_service_acceptance::refuses_a_move_of_a_file_git_does_not_track_before_writing_anything`.

## M1 — the B2 reproduction (2026-10-17): not reproduced

Three attempts on a throwaway detached worktree at `4bf3d8853` against a warm `tddy-index-daemon`
(cold warm ~21 min). Each hand-edited 7 files of `tddy-code-restructuring` and applied a one-op
item-anchored `rename_symbol` (`PositionLedger::renames` → `path_renames`) whose v2 header listed only
`ledger.rs`, after `check --deep`. Attempt 1 (7 `.rs` comment lines) returned in 81 s; attempt 2
(6 `.rs` + a `Cargo.toml` comment) in 84 s; attempt 3 (a `Cargo.toml` edit between check and apply) in
4 min 23 s. The type-inference wait answered at once every time, so neither the ready-silent nor the
loading-silent shape occurred, and the new refusals rest on the `fake_lsp` tests. In attempts 2–3
rust-analyzer's build-script run (`cargo check --workspace --message-format=json`, a child of the
rust-analyzer pid) held `target/debug/.cargo-lock` while the engine's compile gate sat at 0.0% CPU:
~47 s and ~60 s of lock wait (gates of 50 s and 100 s against 11–13 s uncontended), not permanent.
That contention was deferred (F5) to `2026-10-17-restructure-rust-analyzer-shares-target-with-the-compile-gate`.

## Decisions (all the developer's)

- **F1** (2026-10-09) — a loading-silence bound: 10 min, injectable, refused as `ServerStalled`. It
  reverses `#sharpen` 4/8's "heartbeat, no deadline" for *silence while loading* only; a total deadline
  stays rejected (a large workspace legitimately indexes for 20+ minutes while narrating). Rejected: no
  bound (a daemon-wide outage stays possible), a caller-stated deadline (no caller states one).
- **F2** (2026-10-09) — an untracked source is refused; no automatic `git add` (staging the author's
  file without consent is a fallback), no `fs::rename`, no reordering inside an edit.
- **F3** (2026-10-09) — no live rust-analyzer test: `fake_lsp` reproduces every shape the bounds act
  on; the real-workspace reproduction is M1, recorded rather than automated.
- **F4** (2026-10-09) — operation context as a `ProcessStart` field (consented `tddy-lsp` edit) rather
  than a second observer event: two places construct it and the trait is unchanged.
- **F5** (2026-10-17) — a separate `cargo.targetDir` for rust-analyzer: deferred after M1 measured the
  contention.
- **F6** (2026-10-09) — stale context on `StaleOperation` only. `stale_findings` is shared with the
  daemon's `check`, outside the consented daemon edit, and a re-run meets `StaleOperation` first.
  Deferred to `2026-10-09-restructure-check-does-not-say-a-run-wrote-a-stale-plan`.
- **F7** (2026-10-17) — test 15 (`a_loading_server_that_keeps_narrating_is_never_refused_as_stalled`)
  reworked so it can fail: the fake passes the warm-up, then goes busy and narrates on its own 250 ms
  timer; a 3 s loading bound runs for 7 s and the cancelled run is still waiting in `type inference at
  src/lib.rs:1`. With the chatter's new-words reset disabled it goes red as `ServerStalled`.
- **F8** (2026-10-17) — test 13 waits on a condition (polling the heartbeat every 100 ms, up to 15 s)
  instead of a fixed 3 s window.
- **F9** (2026-10-17) — the loading-silence bound also covers the crate-index warm-up
  (`warming the crate index`), through the same `keep_waiting_unless_stalled` → `refuse_silence` →
  `silence_verdict` path; test 29 proves it. The no-deadline guard passes unchanged.

## Backlog

- **Deleted:** `2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check` — M1 did not
  reproduce the hang; the developer chose deletion on 2026-10-17 because its capability ask (fail with
  the stage rather than wait for ever) is delivered by the silence bounds.
  `2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade` — its only open item, the
  pre-apply gate omitting the destination, is M3.
  `2026-10-05-restructure-no-record-of-what-an-apply-executes` — item 3, its last open item, is M5.
- **Narrowed:** `2026-09-09-restructure-defects-from-the-first-cross-crate-move` (item 2, the
  uncommitted-file `git mv`, struck; items 1 and 3 stay open);
  `2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass` (§ 4, the stale-plan
  message, removed). Item R of `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures`
  needed no edit: `#reshape` 4/19 (#601) had already resolved it and deleted the entry.
- **Added and kept (deferrals):** `2026-10-09-restructure-baseline-omits-crates-a-move-re-points`,
  `2026-10-09-restructure-check-does-not-say-a-run-wrote-a-stale-plan`,
  `2026-10-09-restructure-keep-the-plan-as-it-was-before-a-run`,
  `2026-10-17-restructure-rust-analyzer-shares-target-with-the-compile-gate`.

## Code issues (final measurements)

- `oversized-file-backends-rust` — **3,025** production lines to the first inline `#[cfg(test)] mod`,
  against 3,009 on `master` at `e08b9e2f0` (+16, wiring only: the `SilenceBounds` re-export, the
  `silence_bounds` field and builder, one renamed call site, the heartbeat's `said_ready`). Row added;
  split deferred to #612 (`oversized-files`).
- **Opened** for the clean-code findings this PR deferred (only in-place helper, local and constant
  edits were allowed in files later `#reshape` branches carry):
  `complexity-runner-preflight-moves` (`preflight_moves` 6, `commit_operation` 8, `GroupRun::settle` 7
  parameters), `duplication-compile-gate-baseline-packages-moves-files-across-crates` (2 copies),
  `duplication-lib-written-by-run-op-id-suffix` (2 copies), `complexity-readiness-silence-verdict`
  (nesting 3, 21 lines), `signature-spawn-record-process-start` (`&Option<OperationContext>`).
- No other open record in the three packages covers code this PR touched; none was re-measured.
- Pre-existing over-length functions left the same or shorter: `apply_held_plan` 158 (CLI, master
  159) and 160 (daemon, master 159), `check_plan` 94 (master 110), `Rehearsal::rehearse` 45 (was 66).
  Files over 500 lines grew only by their inline `mod tests`; each production half is under 500
  (`readiness.rs` 477 → 631, `compile_gate.rs` 507 → 610, `apply.rs` 513 → 681, `store_run.rs`
  718 → 812 total lines).

## Verification (scoped)

`cargo test -p tddy-code-restructuring --lib` 1064 passed; `silent_server_acceptance` 6 (5/5 repeat
runs), `wait_heartbeat_acceptance` 7 (20/20 repeat runs), `apply_preflight_acceptance` 7,
`apply_compile_gate_acceptance` 9, `spawn_record_acceptance` 13, `check_precondition_parity` 23,
`test_binary_move` 18; `cargo test -p tddy-index-daemon --no-fail-fast` 160 passed, 12 ignored;
`cargo test -p tddy-lsp` 66 passed; clippy `-D warnings` and `cargo fmt --check` clean for the three
packages. The rest of the workspace is CI's.
