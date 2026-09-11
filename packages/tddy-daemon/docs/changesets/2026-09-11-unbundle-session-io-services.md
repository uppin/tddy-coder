# 2026-09-11 — The terminal and session-file subsystems become their own services

Thirteen modules and roughly 3,500 production lines leave `tddy-daemon`, and with them 22 RPC
coordinates: the terminal family to `terminal_session.TerminalSessionService` in
[`tddy-terminal-rpc`](../../../tddy-terminal-rpc/docs/terminal-session-service.md), and the
session-file families to `session_files.SessionFilesService` in
[`tddy-session-files`](../../../tddy-session-files/docs/session-files-service.md).
`connection.ConnectionService` serves 50.

What stays here is the answers only a daemon has, and it stays as **ports** rather than as a branch
per handler:

- `connection_service/svc_terminal_ports.rs` — which terminal a `(session_id, terminal_id)` names,
  who holds the control lease, how a login shell becomes a task, which identity a token belongs to.
- `connection_service/svc_session_files_ports.rs` — the OS-user resolver, the data and staging
  directories, the attachment cap, the instance id, which checkout a session's guidance is read from,
  and the read deadline. Plus `PeerRoutedSessionFiles`, which wraps the crate's implementation and
  routes eight of the thirteen methods by `daemon_instance_id`.
- `terminal_session_adapter.rs` — binds both terminal owners to the bridge's trait, and
  `DaemonTerminalSessionStore` is a composite that resolves a **sandboxed** session first. A jailed
  terminal is an ordinary terminal to the bridge, so the four sandbox branches in the terminal
  handlers are gone — one of which carried its own transcription of the bridge's replay and offset
  loop.
- `cli_session_manager.rs` — PTY session *lifecycle* and the origin of the `TaskRegistry` several
  services share. The terminal surface reaches it through `TerminalSessionStore`.
- `connection_service/svc_split_context_from_codebase_host.rs` — the split-context directory
  builder, which reads through the served surface rather than a read of its own.

`local_socket_server.rs` mounts a fourth tonic service, `terminal_session.TerminalSessionService`,
because `tddy-sandbox-app` inside every jail dials the bidirectional terminal stream there. Its
adapter is generated, not hand-written. `host_tonic_adapter.rs` and `worktree_tonic_adapter.rs` are
still hand-written — see
[`docs/dev/todo/2026-09-11-node-ones-two-tonic-adapters-are-still-hand-written.md`](../../../../docs/dev/todo/2026-09-11-node-ones-two-tonic-adapters-are-still-hand-written.md).

`to_tonic_status` moved to `tddy-service`, because generated adapters land in `OUT_DIR`s of crates
that do not depend on `tddy-daemon`. The three adapters here import it from there.

Docs: [connection-service.md](../connection-service.md).
Full record, including the four behaviours the move silently lost and how each surfaced:
[../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
