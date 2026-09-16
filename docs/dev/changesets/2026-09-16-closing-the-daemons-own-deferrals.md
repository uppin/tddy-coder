# 2026-09-16 — Closing four of the daemon's own deferrals

**Type:** Enhancement

Four gaps the warm code-intelligence daemon recorded rather than fixed, closed in the same pull
request, plus the module split its size was heading for.

**A coverage capture can be stopped.** `capture_coverage` and `analyze_duplicates` take a
`cancelled: &dyn Fn() -> bool` predicate, checked before the instrumented build, per harness and per
test in the capture, and per signature in both detection passes. A dependency-free predicate rather
than a `CancellationToken`, because `tddy-code-analysis` is synchronous and has no tokio; it fits the
closure sink the crate already took. A stopped run returns `AnalysisError::Cancelled { work, reached }`
rather than `Ok(())` — a partial capture reporting success would be worse than not being able to stop
it — and the daemon maps that to `DeadlineExceeded` and records the request as cancelled rather than
refused, since nothing was wrong with the request.

`Coverage` learns its caller is gone from a failed send into the response stream. `DuplicateTests` has
no such send, so it watches the channel closing instead.

**The complexity cache is bounded** at 8192 scored versions, LRU, renewed on a hit. The number is
reasoned rather than round: `generate_report` scores every `.rs` file in a tree — about 1,500 here —
so a bound near that would make two successive reports evict each other's entries and the cache would
cost a lookup per file while saving nothing. Eviction scans for the coldest entry rather than keeping a
second ordered index, which is tens of microseconds against the milliseconds of `syn` parsing a hit
saves.

**`tddy-index-daemon analyze`** covers the four analysis RPCs from the binary's own command line, so
single-shot mode no longer serves half the service it hosts. It uses `tddy-tools analyze`'s flag names,
so the same operation is asked for in the same words through both front ends. `^C` drops the response
stream, which is the signal the service turns into the cancellation predicate.

**`StatePaths`' fields are private.** `.restructure/`'s layout is no longer public API, so re-keying
the journal to a plan identity stays a non-breaking change. Nothing outside `runner.rs` needed a path:
the daemon uses `StatePaths::under` with `open_run` / `commit_operation`, and `runner::status` for a
status query.

**`index_daemon.rs` is split** — 479 lines into a 24-line facade over `error` (44), `spawn` (76) and
`registry` (355), all private so no public path moved. `SocketWasCleared` stays in the registry with
both halves of its guarantee: it is a `#[must_use]` witness the readiness wait takes as an argument,
so deleting the socket-clearing step is a compile error rather than a silently passing test.

Four `docs/dev/todo/` entries deleted. The six that remain are each blocked on something structural —
a non-destructive `drain_notifications`, a published renderer, a consumer's semantics, a
cancellation-aware `block_on`, a split that would bury this diff, or a trait change across many
implementors.
