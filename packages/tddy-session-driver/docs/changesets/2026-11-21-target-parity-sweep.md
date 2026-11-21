# 2026-11-21 — Session hooks pass their answer wait to the sink

**Type:** Bug Fix · **Stack:** `#target-parity` sweep

`SessionHooks` keeps its spec's `answer_wait_secs` and passes it to
`ActivityHookSink::answering_through`, which now needs it to remove a late answer from the target.
Full record: [`docs/dev/changesets/2026-11-21-target-parity-sweep.md`](../../../../docs/dev/changesets/2026-11-21-target-parity-sweep.md).
