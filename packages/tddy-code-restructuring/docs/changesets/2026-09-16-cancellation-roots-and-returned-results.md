# 2026-09-16 — Cancellation, workspace roots, and results as values

**Type:** Architecture

Three changes that let something other than a command line drive this engine.

**Waits end on readiness or cancellation.** `--indexing-budget` is withdrawn, along with
`WARMUP_BUDGET`, `SETTLE_BUDGET`, `settle_budget_for` and `resolution_budget`. The flag set a warm-up
bound and derived the per-operation bound as a twentieth of itself, so `--indexing-budget 900`
produced a 45-second ceiling and refused plans that had already indexed for twenty minutes. A server
should not invent a deadline its caller never stated. A `CancellationToken` is checked *inside* the
poll loops rather than awaited, because the engine is synchronous and runs under `spawn_blocking`
where dropping the calling future stops nothing. A server that stays unable to answer one method is
`ServerNotSettled`, kept distinct from a malformed plan.

**Every entry point takes the workspace root it acts on.** No `std::env::current_dir()` remains
outside the command line, where a process directory legitimately becomes a root. That is what lets one
process serve several worktrees. `StatePaths`, `open_run`, `restore_ledger` and `commit_operation` are
public, so a host can drive the apply loop without re-deriving `.restructure/` or re-implementing the
write-ahead commit sequence.

**Nothing here writes to stdout.** `apply`, `status`, `check`, `anchors` and `verify` return
`RunSummary`, `PlanProgress`, `Vec<Finding>`, `Range` and `Comparison`; `dispatch` returns an
`Outcome` the front end renders. Progress and the per-operation account go to caller-owned sinks on
`Options`. A test reads this crate's own sources and asserts only `restructure_cli.rs` prints — a
server serving `check` over its own stdin and stdout would otherwise have every RPC frame after the
first finding corrupted. A check with findings is now an `Ok` carrying them; the command line prints
them and exits non-zero.
