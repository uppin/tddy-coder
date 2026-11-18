# 2026-11-18 — `GetSessionStart` lists what a start ran on its target, and keeps what it cut

**Type:** Feature

- The start ledger folds `remote_call` events into the record's `remote_calls` (oldest first, at most
  `MAX_REMOTE_CALLS` = 50, newest kept; credentials scrubbed again; each call advances
  `updated_at_unix_ms`). The log types (`RemoteCallRecord`, `RemoteCallOutcome`) live in
  `start_ledger/remote_calls.rs`, re-exported from `start_ledger`.
- `worktree_ready` sets `branch`/`worktree`; they stay on a failed or cancelled record, and on a
  succeeded one whose session listing has none.
- `DescribeSessionStartOptions` describes `usable` as *listed as startable*, never a probe.
- `start_ledger.rs` split by the restructure engine: 531 → 485 production lines + `remote_calls.rs` (54).

See [project-concept.md § Project-home tools](../../../../docs/ft/daemon/project-concept.md). Full record: [`docs/dev/changesets/2026-11-18-start-visibility.md`](../../../../docs/dev/changesets/2026-11-18-start-visibility.md).
