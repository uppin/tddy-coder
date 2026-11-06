# 2026-11-06 — Two jails started within ~256 ms of each other share a tool-IPC socket path

**Category:** Defect (pre-existing; found by `/validate-changes` on #823)
**Source:** `packages/tddy-sandbox/src/spec.rs` (`SandboxSpec::short_ipc_socket_path`)

`short_ipc_socket_path(session_id)` names a jail's tool-IPC socket
`$TMPDIR/tddy-<first 10 alphanumerics of the session id>-<daemon pid>.sock`. Session ids are UUIDv7:
their first 10 hex digits are the top 40 bits of the 48-bit millisecond timestamp, so they change
only every 256 ms. Two sessions one daemon starts within that window get the same path, and the second runner fails at start with
`tool ipc bind failed: Address already in use` or `File exists`, which the daemon reports as
`DeadlineExceeded: wait for sandbox ready: sandbox child died before ready marker`.

In production this needs two jailed starts inside one 256 ms window on one daemon (a stack fan-out
could do it). In the test suites it happens routinely. On #823's local run, ten jailed tests failed
this way across `claude_cli_jail_from_spec_acceptance`, `sandboxed_claude_cli_acceptance`,
`sandboxed_claude_launch_baseline`, `sandboxed_cursor_cli_acceptance` and
`harness_clis_from_spec_acceptance`, a different test each run. Every one passes with
`--test-threads=1`.

What would close it: take the socket name from the session id's random tail, or a short hash of
the whole id, rather than its timestamp prefix. Keep it inside `SUN_LEN`. Add a unit test that two
ids from the same 256 ms window give different paths.
