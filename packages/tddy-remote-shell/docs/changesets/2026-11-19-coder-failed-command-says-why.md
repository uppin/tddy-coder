# 2026-11-19 — A command that failed on a Coder workspace says why

**Type:** Fix

`CoderShell::output_of` reports a command that ran (marker on stdout) and exited non-zero as
`ExecOutput::exited(status, merged words)`: the pty-merged output is its stderr and the `coder` CLI's
generic wrapper is dropped. See [architecture.md § Coder](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-19-target-start-host-paths.md`](../../../../docs/dev/changesets/2026-11-19-target-start-host-paths.md).
