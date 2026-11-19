# 2026-11-18 — `StartSessionEvent` reports a start's remote calls and its cut worktree

**Type:** Feature

`session.proto`: `StartSessionEvent` gains `remote_call` (`StartRemoteCall { host, command,
duration_ms, outcome, exit_code, stderr_tail }`, field 7) and `worktree_ready`
(`StartWorktreeReady { branch, worktree }`, field 8). The committed TS is regenerated.
See [start-session-phases.md](../start-session-phases.md). Full record: [`docs/dev/changesets/2026-11-18-start-visibility.md`](../../../../docs/dev/changesets/2026-11-18-start-visibility.md).
