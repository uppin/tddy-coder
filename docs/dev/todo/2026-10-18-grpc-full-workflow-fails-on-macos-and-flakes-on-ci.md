# 2026-10-18 — `grpc_full_workflow::full_workflow_asserts_each_state_transition` fails on macOS and flakes on CI

**Category:** Known failing test (macOS, two of two runs), flaky (CI)
**Source:** scoped verification of [#645](https://github.com/uppin/tddy-coder/pull/645) and the CI attempts of
[#651](https://github.com/uppin/tddy-coder/pull/651)

`packages/tddy-e2e/tests/grpc_full_workflow.rs` drives the whole workflow (interview, plan, acceptance tests, red,
green, demo, evaluate, validate, refactor) through a gRPC stream and fails at line 408:

```text
Expected WorkflowComplete event (got 185 state transitions)
```

- **macOS: failed both times it was run.** Once on `master` at `b73c97e0` (in a clean worktree, so not caused by any
  branch) and once on #645's branch, each taking about 34 s with the same message. Every other test in the file
  passed. Two runs do not establish that it is deterministic.
- **Linux CI: intermittent.** `FLAKY 2/3` in one of four attempts on #651 (passed on retry in 0.3 s); passing in the
  others. It was not red in any CI run examined.

The workflow does run (185 transitions were seen) but `WorkflowComplete` does not arrive within the test's budget: a
timed drain loop of 300 × 100 ms after the main loop. The assertion therefore measures how quickly a presenter thread
and a gRPC stream deliver the last event, not whether the workflow completes.

## What would close it

A run with `RUST_LOG` raised and `--nocapture`, comparing the order of the last events on macOS against a Linux run,
to learn whether the completion event is slow, lost, or sent before the stream is drained. If the event is merely
late, replace the fixed 30 s of polling with a wait on the event itself; if it is lost, that is a product defect in
the stream.

**Why deferred:** nobody has looked at it with logging, and it does not gate any PR (macOS-only, and retried to green
on CI).
