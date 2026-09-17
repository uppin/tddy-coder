# 2026-09-16 — Cancellable captures, and a bounded complexity cache

**Type:** Enhancement

`capture_coverage` and `analyze_duplicates` take a `cancelled: &dyn Fn() -> bool` predicate.
The capture checks it before the instrumented build, per harness and per test; detection checks it per
signature in both the inverted-index and subset passes; `analyze_coverage_dir` checks it per artefact
loaded. A predicate rather than a cancellation token because this crate is synchronous and has no
tokio — and it matches the closure progress sink the capture already accepted. `never_cancelled()`
is the command line's, which is its own caller.

A stopped run returns `AnalysisError::Cancelled { work, reached }`, naming what was cancelled and how
far it got. Not `Ok(())`: a partial capture that reported success would be worse than one that could
not be stopped, because `analyze report` would then join against a denominator that is quietly short.

`InMemoryComplexityCache` is bounded at `SCORED_VERSIONS_KEPT` = 8192, least-recently-used, renewed on
a hit. It counts scored *versions* rather than files, and the floor is set by `generate_report`, which
scores every `.rs` file in a tree — around 1,500 in this workspace. A bound near that would make two
successive reports over one workspace evict each other's entries, so the cache would cost a lookup per
file and save nothing. 8192 holds roughly five such passes: the last report's scores alongside a
working set of files under active edit. Entries are a name, a line and a count per function, so the
ceiling is single-digit megabytes.

Eviction scans the map for the coldest entry rather than maintaining a second stamp-ordered index — a
walk of the bound against the milliseconds of `syn` parsing that a hit avoids, and one fewer structure
to keep in step on every renewal.
