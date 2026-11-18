# 2026-11-18 — An embedded daemon's agent-tool socket is its own instance's
**Type:** Fix

**Packages:** `tddy-daemon-kernel`, `tddy-daemon`, `tddy-desktop` (tests: `tddy-agent-launch`,
`tddy-session-lifecycle`).

**Incident.** An agent session ran `tddy-desktop --help`. The binary ignored the argument and
started a second instance on the running one's `~/.tddy`. The embedded socket path was keyed on the
data dir alone, so the second instance unlinked the first's socket and bound its own. When it was
stopped, its aborted serve task left a dead file at the shared path. From then on every session of
the first instance was refused: `… is not being served: Connection refused (os error 61)`.

- `tddy-daemon-kernel`: `next_embedded_socket_path()` returns `$TMPDIR/tddy-agent-tools-<pid>-<n>.sock`,
  replacing `agent_tool_socket_path(data_dir)`. `AgentToolSockets` binds the `Embedded` socket on
  first use, owner-only. It checks the file's identity, binds again if the file is gone, clears the
  sockets of dead instances (`clear_dead_instances_sockets`), and stops serving when dropped.
- `tddy-daemon`: removed `src/agent_tool_socket.rs` and the `RuntimeTasks::agent_tool_socket` task.
  `runtime` re-exports `DaemonChildren`.
- `tddy-desktop`: `--help` / `--version` print and exit, and any other argument is refused
  (`invocation.rs`). Shutdown goes through `DaemonChildren::shut_down`, which also stops jails and
  the index daemon and removes the socket.

Kept as is by decision: two instances on one configuration share one LiveKit identity.
Not swept: the legacy `tddy-agent-tools-<16 hex>.sock` file, which names no pid.

Docs: [daemon kernel § agent_tool_sockets](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md),
[claude-cli session § `--daemon-socket`](../../ft/daemon/claude-cli-session.md),
[Tddy Desktop § Installing it](../../ft/desktop/tddy-desktop-tauri.md#installing-it).
