# 2026-07-25 — enqueued-input-overlay: `AppliedOffset` (monotonic `AtomicU64` accumulator: `record` returns whether the max advanced, `get` reads it) and `TaskChannel` gains the shared input-offset ACK state

**Type:** Feature

`acknowledge_input(offset)` records + publishes on a `watch<u64>`, `subscribe_acked_offset()` for output-stream subscribers. Shared home so a daemon `PtyHandle` rebuilt per RPC still shares ack state across the input and output paths. Unit tests in `task.rs`. Cross-package [changeset](../../../../docs/dev/changesets/). (tddy-task)
