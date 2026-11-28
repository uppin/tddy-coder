# 2026-11-28 — Throttle QueryBranch polling and de-duplicate unavailable warnings

**Type:** Bug Fix

`pr_status_unavailable` now warns through `WarnGate` (`work_queue/warn_gate.rs`): one
`log::warn!` line per branch and reason — the first appearance only, again when the reason
changes, and a lookup that succeeds clears the branch's key so its next failure warns again.
A branch is polled every few seconds per rendered branch, so the same line previously repeated
~2,200 times an hour (81 MB of daemon log overnight, investigation #1061). The gate is
process-wide (`WarnGate::shared()`), because a branch is polled by one `QueryBranch` route
whichever handler instance answers it. The RPC answer is unchanged — `unavailable` with its
reason still reaches the row every poll; only the log line is gated.
