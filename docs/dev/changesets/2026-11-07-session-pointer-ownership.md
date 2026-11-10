# 2026-11-07 — The session pointer is owned, git-excluded and removed on every host

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)), branch
`feature/agent-tools/session-pointer`, based on `master`. Blocks
`feature/agent-tools/split-remote-env` (moves `TDDY_REMOTE_*` into this pointer shape),
`feature/agent-tools/acp-subagent-leftovers` and `feature/agent-tools/env-allow-list`.

Packages: `tddy-agent-backend`, `tddy-tools`, `tddy-agent-launch`, `tddy-session-lifecycle`,
`tddy-sandbox`, `tddy-coding-agent-specs`; `tddy-sandbox-app` and `tddy-session-split` only set the new
`cwd: None` at their MCP server literals.

## What changed

- **Owner.** `.tddy/session.json` carries `owner: SessionPointerOwner { pid, host }`
  (`tddy-agent-backend::session_pointer`; `host` is `gethostname`, which asserts on failure). A
  headless turn's, a CLI launch's and a headless conversation's pointer are owned by the process that
  wrote it. A pointer without `owner` is read as before; `deny_unknown_fields` stays, so only a newer
  `tddy-tools` against an older daemon is supported.
- **Refusal.** `tddy-tools`' `session_source` refuses a pointer whose owner runs on this host and has
  exited (`UnreadablePointerCause::OwnerGone`) or whose socket path is missing (`SocketMissing`); an
  owner on another host is not checked, and `--socket` never is. `UnreadablePointer.reason: String`
  is `cause: UnreadablePointerCause` (`Invalid`, `OwnerGone`, `SocketMissing`).
- **Exclusive hold.** `TurnFiles::write_session_pointer` refuses to overwrite a pointer for another
  socket whose owner is alive on this host (`SessionPointerHeld`, carried as
  `BackendError::InvocationFailed`).
- **SSH parity.** `tddy-agent-launch`'s `cli_session_pointer` places an ssh target's pointer with one
  bounded command (`run_over_ssh`: git `info/exclude` append unless present, `umask 077` write) and
  removes it with a bounded `rm -f` on agent exit and from `DeleteSession`. `run_over_ssh` is the
  slot-file write's ssh core, returning what the command printed.
- **Headless conversations.** An inline-MCP agent (Codex) gets
  `<conversation dir>/.tddy/session.json` (conversation id and minted token in `remote`, no socket)
  and a `cwd` override on its registration (`-c mcp_servers.tddy-tools.cwd=…`); the slot names no
  `env_vars` and the launcher's `forwards_server_env` is gone, so the command's env has no `TDDY_*`.
  `McpServerCommand`, `McpServerSpec` and `HeadlessMcpServer` carry `cwd: Option<PathBuf>`.
- **Jail socket.** `SandboxSpec::short_ipc_socket_path` names the socket
  `tddy-<fnv64(session id) hex>-<pid>.sock`, so sessions minted in the same millisecond differ.

## Deferred

The remote agent's own pid as an ssh pointer's owner: `record_remote_owner` and
`REMOTE_AGENT_PID_FILE` exist and are tested but have no caller, since no launch runs the agent on
the ssh target. Until one does, an ssh pointer is owned by the daemon, which a `tddy-tools` on the
target skips —
[2026-11-07-ssh-agent-launch-remote-pointer-owner](../todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md).

## Resolved backlog entries

- *The session pointer has one slot per worktree and no owner check* (`2026-11-05-session-pointer-lifecycle-hazards`)
- *A CLI session's pointer on an ssh target is not git-excluded or removed* (`2026-11-06-ssh-session-pointer-exclude-and-removal`)
- *A Codex conversation's tool-server variables are still `TDDY_*` in its environment* (`2026-11-05-headless-conversation-tool-server-env`)
- *Two jails started within ~256 ms of each other share a tool-IPC socket path* (`2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions`)

## Documentation

- [tddy-tools session resolution](../../../packages/tddy-tools/docs/session-resolution.md)
- [tddy-agent-backend architecture](../../../packages/tddy-agent-backend/docs/architecture.md)
- [tddy-agent-launch module layout](../../../packages/tddy-agent-launch/docs/module-layout.md)
- [tddy-session-lifecycle coding-agent launcher](../../../packages/tddy-session-lifecycle/docs/coding-agent-launcher.md)
- [tddy-coding-agent-specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md)
- [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md)
- [Claude CLI session § The generic CLI spec launch](../../ft/daemon/claude-cli-session.md), [Session agent roster](../../ft/daemon/session-agent-roster.md)

## Restructuring and code issues

Five files this change grew were already over the 500-production-line budget; each is split by the
restructure engine and is under it: `acp_driver.rs` 451, `spec_headless.rs` 451,
`cli_agent_steps.rs` 468, `opencode_cli_spawn.rs` 412, `coding_agent_launcher.rs` 424. Their
`oversized-file` records, and `complexity-coding-agent-launcher-headless-plan`, are closed; the
numbers are in the package entries.

