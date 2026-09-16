# 2026-09-16 — A coverage capture cannot be cancelled, so a disconnected client leaves it running

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M7

`code_index.CodeIndexService`'s `Coverage` and `DuplicateTests` are server-streaming precisely
because they are long — 55.8 minutes for a `tddy-daemon` capture, ~22 for duplicate-tests. But
`capture_coverage` (`packages/tddy-code-analysis/src/coverage.rs`) takes no cancellation surface, so
a client that hangs up one minute in leaves the other 54 running. The progress sink notices the
dropped receiver, logs once and stops sending — it has no way to *cancel*.

`DuplicateTests` has the same gap plus no progress at all, because `analyze_coverage_dir` takes no
sink either.

## Why it was not fixed with the rest

The restructure half does not share the problem: that changeset threaded a `CancellationToken`
through its three wait loops. The asymmetry is the point — analysis was left alone deliberately,
because closing it is an API change to the capture pipeline, and `tddy-code-analysis` has no
`tokio-util`, so a token cannot cross the boundary without a new dependency. `CLAUDE.md` requires the
developer's consent for that, and the changeset's `#### tddy-code-analysis` delta explicitly scoped
the capture pipeline out ("no change to the capture pipeline").

## What closing it would take

A cancellation parameter on `capture_coverage` and `analyze_coverage_dir`. Either `tokio-util` in
`tddy-code-analysis` (it is already a per-crate dependency of eight crates at `0.7`, so nothing new
enters `Cargo.lock`), or a dependency-free `&dyn Fn() -> bool` the caller supplies — the crate is
synchronous and has no tokio at all today, which is an argument for the second.

Worth doing before anyone runs a capture through the daemon in earnest: a 55-minute job nobody can
stop is the kind of thing that gets the daemon killed instead.
