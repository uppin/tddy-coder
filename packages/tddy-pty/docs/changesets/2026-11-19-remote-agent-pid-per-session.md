# 2026-11-19 — A PTY child's status can be read from what it printed last

**Type:** Fix

`PtySpawnSpec.exit_code_in_output: Option<ExitCodeInOutput>`. When the child exits
`client_code`, the runtime reads the decimal after the last `marker` in the final 4 KiB of its
output as the status; a clean exit, a signal, another code or no marker keeps the child's own. Full
record: [`docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md`](../../../../docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md).
