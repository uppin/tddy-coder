# 2026-10-04 — Confirm the corrected compile-share summary on CI, and keep the report's shape pinned to cargo

**Category:** Future enhancement (CI observability)
**Source:** the `#e2e-leg` stack, node 3 — `scripts/ci-e2e-timing.ts` and the `Rust compile timings` job.

## What remains

The first manual dispatch of `Rust compile timings` (run 37220592932) wrote a **wrong** summary to the job
page — `0%`, verdict `stop` — because the parser it ran read a report shape cargo does not write: it
looked for `mode: "test"`, while a real `cargo-timing.html` has `mode: "todo"` for every compile and marks
a test unit in its target string (` test "name" (test)`, ` lib (test)`). The parser and its fixtures were
corrected and the real artifact re-read locally (53.3%, proceed — see
[split the compile by test target](./2026-10-04-split-e2e-leg-compile-by-test-target.md)), but **the
corrected script has not yet run in the job itself**, so the page for that run still says `stop`.

- Dispatch `Rust compile timings` once more from a branch that carries the correction, and check the
  summary says what the local read says. It is a cold ~28-minute build and `workflow_dispatch` also
  starts the rest of `ci.yml`, so batch it with the first change that needs a fresh number — the first
  split leg in the linked entry is the natural moment.
- The fixtures in `scripts/ci-e2e-timing.test.ts` now follow the real shape, but nothing ties them to
  cargo's output: a cargo release that renames a field fails here only when someone dispatches the job.
  Consider committing a small trimmed real report as a fixture, or having the script fail loudly when a
  report holds units but none is a test target (it currently reports `0 s` and `stop`, which is exactly
  how this went unnoticed).

## Why it was left

Verifying it needs a cold CI build that costs a full pipeline, and the number it would show is already
known from the local read. The second point is a hardening idea, not a defect with a user.

## What would close it

A dispatched run whose step summary matches a local read of the same artifact, and a report with units
but no test targets failing the step instead of printing a verdict.
