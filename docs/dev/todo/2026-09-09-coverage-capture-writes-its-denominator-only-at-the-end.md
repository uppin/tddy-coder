# 2026-09-09 — A coverage capture is all-or-nothing

**Category:** Future enhancement
**Source:** `analyze-coverage-export-and-harness-selection` (#466)

`capture_coverage` accumulates the denominator in memory across every harness and calls
`write_denominator` once, after the last test. A failure at test 2,100 of 2,159 therefore loses
`rust-coverage-final.json` entirely, and `analyze report` has nothing to join — the per-test
artifacts survive on disk, but the run has to be repeated from the start.

That mattered little when the capture could not complete at all. Now that it does — 55.8 min for
`tddy-daemon` — the exposure is a real 55 minutes of work behind a single unhandled error, and the
per-test loop calls out to `cargo`, a test binary, `llvm-profdata` and `llvm-cov`, any of which can
fail on one test for reasons unrelated to the other 2,158. Note the loop already propagates such an
error with `?`, so one bad test aborts everything.

Two independent improvements: persist the denominator incrementally (or per harness) so a partial
run is still reportable, and decide deliberately whether one test's export failure should abort the
capture or be recorded and skipped. The second is a behaviour question, not a refactor — a silently
skipped test would understate coverage, so it needs the developer's call rather than a default.
