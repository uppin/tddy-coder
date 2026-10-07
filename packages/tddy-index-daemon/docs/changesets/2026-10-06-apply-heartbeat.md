# 2026-10-06 — A queued run beats, and a hung-up caller releases the root

**Type:** Feature

`CodeIndexPorts` gains `wait_heartbeat`; `operations.rs`'s `serve_apply` and `serve_check` wait for the
root gate with `select!` against a heartbeat interval, sending `still waiting (Ns) — queued behind another
operation on <root>`. A failed send drops the request out of the queue, so a caller that has hung up is
noticed within one cadence and the root it held is released — the most plausible reason a *retry* of the
incident hung. `apply.rs` and `queries.rs` call `registry_for_waiting`; the warm and cold paths share the
same heartbeat. `WorkspaceIndex::with_wait_heartbeat` and `wait_heartbeat` are the test seam.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-apply-heartbeat.md).
