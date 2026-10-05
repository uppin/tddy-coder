# 2026-10-05 — A `Snapshot` RPC, and the warm script warms

**Type:** Feature

Cross-package entry:
[2026-10-05-restructure-same-crate-moves.md](../../../../docs/dev/changesets/2026-10-05-restructure-same-crate-moves.md).
Product entry:
[2026-10-05-restructure-same-crate-moves.md](../../../../docs/ft/coder/changelog/2026-10-05-restructure-same-crate-moves.md).

## `Snapshot`

`code_index.CodeIndexService` has a unary `Snapshot(SnapshotRequest{workspace_root, plan})` returning
`SnapshotResponse{paths, rewritten, stale}`: how many files the plan's header names, whether the header
on disk differed from the tree and was replaced (stated, not inferred from `paths`), and the operations of
an item-anchored plan whose item changed or went, as `StaleOp`s in plan order. `queries.rs`
(`serve_snapshot`, `snapshotted`) takes the root's queue, acquires the root's warm client **only** for a
plan with item anchors, and runs `runner::snapshot_resolving` on a blocking task with a cancellation
token that fires when the request is dropped. `status.rs` maps `RestructureError::WarmNeedsIndexDaemon` to
the same class as `NeedsIndexDaemon`. The service has twenty RPCs.

Why: `tddy-tools restructure snapshot` of an item-anchored plan re-resolves its anchors through a language
server, and with no `Snapshot` RPC `answered_without_an_index` kept it in the CLI process, so a carve with
a warm daemon still started a cold rust-analyzer (six to ten minutes for one plan) and a machine with none
on `PATH` failed with `lsp server exited`. That was the "snapshot crash" seen while moving the lifecycle
crate: the cause is the missing RPC, not an empty `files` header.

## `./run-index-daemon` warms

After the daemon answers, whether it was started or reused, the script runs `tddy-tools restructure warm`
for the checkout root (`TDDY_INDEX_SOCKET` set to its own socket) and returns when the crate graph is
queryable; `--no-warm` (the first argument) skips it. The client is the `tddy-tools` beside the daemon
binary, built with the daemon unless `--no-warm`; with `TDDY_INDEX_DAEMON_BIN` it is whatever sits next to
that binary, and a path that is not executable is reported without warming. Its output goes to stderr so
stdout stays the one `export` line, and a warm that fails changes nothing else: the `export` line is still
printed and the exit code is the one the start earned. The `Warm` RPC itself is unchanged.

Tests: `code_index_service_acceptance` (a snapshot of an item-anchored plan reports the operation whose
item changed as stale; one with no item anchors is answered without waiting for a language server; a plan
that is not there is refused), and `detached_daemon_production`, two `#[ignore]`d tests that run the real
script (the checkout is warmed and the `export` line survives a failed warm; `--no-warm` leaves the graph
unloaded). Recorded while the change was built, scoped to `tddy-index-daemon` and `tddy-tools`: 551 passed
and 2 failed (exactly the two red tests) before the change, 558 passed and 0 failed after, with 11 ignored.

## Code issues

`complexity-warm-narrate-until-loaded` and `poisoned-warm-latch-on-interrupted-index` name `warm.rs`,
which this change did not touch; unchanged.
