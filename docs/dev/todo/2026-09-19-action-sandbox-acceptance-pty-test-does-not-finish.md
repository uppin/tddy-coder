# `sandboxed_bash_pty_action_streams_output` does not finish locally

**Category:** Flaky / hanging test
**Source:** `#keyring` 3/9 wave-2 baseline measurement (`docs/dev/1-WIP/2026-09-19-keyring-store.md`)
**Package:** `tddy-session-lifecycle`
**File:** `packages/tddy-session-lifecycle/tests/action_sandbox_acceptance.rs`

## What was measured

A scoped baseline run — `./test -p tddy-github -p tddy-daemon-auth -p tddy-session-lifecycle
--no-fail-fast`, which is `--test-threads=1` — reached
`action_sandbox_acceptance::sandboxed_bash_pty_action_streams_output` and stayed there for **more
than 14 minutes** with no further output, on macOS 24.5.0 (arm64). Its three siblings in the same
suite had each finished in under a second:

```
test sandboxed_action_denies_write_outside_egress ... ok
test sandboxed_action_without_output_dir_is_rejected ... ok
test sandboxed_bash_action_writes_to_output_dir ... ok
test sandboxed_bash_pty_action_streams_output ...          <- 14m, no result
```

The test binary and the `tddy-sandbox-runner` it had spawned were both alive and idle. Killing the
pair let the run continue through the rest of the package normally.

## Why this is worth an entry rather than a retry

Because `./test` runs single-threaded, this one test stops **every** suite behind it in the same
invocation. Anyone scoping a local gate to `tddy-session-lifecycle` — which this repo's verification
policy tells them to do — gets no result at all rather than a failure, and the natural reading is
that their own change hung the run.

It is not known whether the test waits forever or merely takes far longer than the others; the
observation is only that it did not finish in 14 minutes and that a kill was needed. Distinguishing
the two is the first thing to do here — a `#[test]` with no timeout cannot tell them apart, and
whichever it is, a pty stream that never closes is worth understanding rather than retrying.

## What was deferred, and why

`#keyring` 3/9 replaces the GitHub token store and touches
`connection_service/svc_pr_status_for_caller.rs` in this package. It does not touch the sandbox or
the action runner, so this is out of its path; absorbing an unrelated hang into a credential-store
PR would bury the reviewable diff in an investigation that has nothing to do with it. The node's
baseline records the hang and works around it.
