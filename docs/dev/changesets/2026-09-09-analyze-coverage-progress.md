# 2026-09-09 — `analyze coverage` reports progress

**Type:** Feature

`capture_coverage` printed nothing between invocation and exit. On `tddy-daemon` that is a silence spanning an instrumented build of the whole dependency graph followed by ~2,000 per-test captures, with no way to tell a working run from a hung one — diagnosing it meant counting files in `coverage/per-test`, deriving a rate from their mtimes, and reading `ps` for child processes. The silence covered exactly the two slowest phases: the build emits nothing either, since its stdout is captured for the artifact JSON and its stderr surfaced only on failure.

`capture_coverage` now takes a progress sink (`&mut dyn FnMut(CaptureProgress)`) and reports `BuildStarted`, `BuildFinished`, `HarnessStarted`, `TestCaptured` and `Finished`. **The library still never prints** — rendering belongs to `tddy-tools`, so a capture stays usable from a TUI where stray stdout would corrupt the display, and callers that want silence pass `&mut |_| {}`. Formatting is a pure function, so the wording is unit-tested rather than eyeballed.

The CLI adapts to its output: a terminal gets one line rewritten in place per test, a pipe gets a line every 25 tests, so a logged run shows movement without 2,000 lines. Failed tests are labelled — a capture deliberately continues past them, and previously they were invisible until the artifacts were read.

**Also per-package build directories.** The instrumented build directory was shared across packages, but the rustc wrapper names the crate it instruments and cargo fingerprints `RUSTC_WRAPPER` — so alternating packages rebuilt the whole dependency graph each time, and two concurrent captures contended on one cargo lock. Each package now builds under its own directory.

**Measured on `tddy-daemon`:** 1.78 s per test against 21.2 s before this branch, over 515 captured tests — a full run projects to ~60 min where it was ~14 h. (tddy-code-analysis, tddy-tools)
