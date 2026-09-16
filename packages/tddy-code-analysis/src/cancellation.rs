//! How a caller stops work that takes tens of minutes.
//!
//! [`crate::coverage::capture_coverage`] is 55 minutes on a package this size and
//! [`crate::duplicate_tests::analyze_coverage_dir`] ~22, so both take a predicate — `&dyn Fn() ->
//! bool` — and check it between units of work: per test in the capture loop, per signature in the
//! detection's two passes. A caller that has gone away therefore costs the remaining minutes of
//! one unit rather than of the whole run.
//!
//! **A predicate rather than a token.** A `CancellationToken` would read the same to the caller,
//! but it lives in `tokio-util` and this crate is synchronous, has no tokio at all, and takes its
//! progress sink as a closure already — so a token would buy nothing and cost a dependency, which
//! `CLAUDE.md` requires the developer's consent for. A predicate is what a caller already holding
//! a token passes *from* it, in one line.
//!
//! What a stop *means* is [`crate::AnalysisError::Cancelled`] and never `Ok(())`: the per-test
//! artefacts a stopped capture wrote are real, but the denominator every later report reads is
//! written last, so a partial capture reported as a success would send a report over a tree it
//! never finished measuring.

/// Nothing will ask this work to stop.
///
/// What a caller with nobody to hang up on passes — a command line that owns the whole process and
/// exits with the run. Pass it as `&never_cancelled`.
#[must_use]
pub fn never_cancelled() -> bool {
    false
}
