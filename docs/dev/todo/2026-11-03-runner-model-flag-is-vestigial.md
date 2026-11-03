# 2026-11-03 — `tddy-sandbox-runner --model` is read by no mode

**Category:** Cleanup
**Source:** `#agent-tools` 10/12 (#799), `TODO(jail-agnostic)` at `packages/tddy-sandbox-runner/src/runner.rs`
(`SandboxRunnerArgs::model`)

## What is there

#799 deleted the runner's agent flags and agent modes. `--model` was not on that list — it names no
agent — but its only readers were the Claude, Cursor, OpenCode and Muse modes, so no mode reads it now.
The action runner (`tddy-daemon-sandbox`'s `sandbox_plan_builder`) still passes `--model ""`, and
`tddy-sandbox-app` passes the model it chose, and several runner and daemon-sandbox tests pass it.

## What to decide

Drop the flag and the argv entries that pass it, in one change with those tests. Left in #799 because
removing a flag beyond the changeset's list touches every launcher and test that spells it.
