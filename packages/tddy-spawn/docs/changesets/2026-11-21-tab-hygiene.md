# 2026-11-21 — Tab hygiene

**Type:** Bug Fix · **Stack:** `#tab-hardening` 2/4 (PR #990)

`child_signal` (`ReapGate`, `wait_without_reaping`, `ToolSignal`, `Signalled`), `ChildEnd`, `SpawnedTool.signal`, and `WorkerRequest::Signal` / `WorkerResponse::Signalled`: a signal reaches a direct or worker child only while its pid is still its own.

Full record: [`docs/dev/changesets/2026-11-21-tab-hygiene.md`](../../../../docs/dev/changesets/2026-11-21-tab-hygiene.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#deleting-a-tmux-run-session).
