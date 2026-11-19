# 2026-11-19 — A driven session's hooks are followed as a daemon target start's are

**Type:** Feature

For an agent whose spec declares `hooks`, a start wires them on every target as a daemon target
start does — token, `remote_log` hook slots, `create_remote_hook_log`, `RemoteHookFollower`,
`ActivityHookSink` — and records the reports in place of serving them; `read_hooks` and
`wait_for_hook` answer them. Fixtures moved to `tests/support/`. See
[architecture.md](../architecture.md) § Hooks. Full record: [`docs/dev/changesets/2026-11-19-session-driver-hooks.md`](../../../../docs/dev/changesets/2026-11-19-session-driver-hooks.md).
