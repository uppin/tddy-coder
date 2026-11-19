# 2026-11-19 — An interactive command says how its remote status is read

**Type:** Fix

`InteractiveCommand.exit: InteractiveExit` — `ClientStatus` for OpenSSH and the in-memory double,
`ReportedLast { client_code, marker }` for Coder, whose CLI exits 1 for any failed command and names
its status last (`CoderShell::FAILED_COMMAND_EXIT_CODE`, `FAILED_COMMAND_STATUS`). See
[architecture.md](../architecture.md). Full record: [`docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md`](../../../../docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md).
