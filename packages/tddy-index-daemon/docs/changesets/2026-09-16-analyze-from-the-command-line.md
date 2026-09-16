# 2026-09-16 — `analyze` from the command line, and cancellation reaching the capture

**Type:** Enhancement

`tddy-index-daemon analyze` covers `coverage`, `report`, `duplicate-tests` and `complexity`, so
single-shot mode no longer serves half the service this binary hosts. The flags are
`tddy-tools analyze`'s, so the same operation is asked for in the same words whichever front end a
reader is holding. The subcommands live in `cli/analyze.rs` rather than `cli.rs`, which would
otherwise have gone over its line budget.

`^C` on a streaming analysis drops the response stream, and a dropped stream is what the service turns
into a cancellation: `serve_coverage` cancels its token when a send fails, and `serve_duplicate_tests`
— which has no send to learn a disconnect from — watches its event channel closing instead. The
predicate reaches `tddy-code-analysis`, so a capture stops between tests rather than running to
completion for a caller that has gone.

A cancelled request is recorded as cancelled rather than refused. Nothing was wrong with it, and the
activity log already distinguished the two. `AnalysisError::Cancelled` maps to `DeadlineExceeded` in
`status.rs`'s exhaustive match — which is how the mapping site was found at all: the match has no
catch-all arm, so the crate would not compile until the new variant was placed.
