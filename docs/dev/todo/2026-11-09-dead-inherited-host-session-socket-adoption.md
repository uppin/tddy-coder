# 2026-11-09 — The inherited host-session socket adoption has no production caller

**Category:** Deferred from `user-mapping-in-supervisor` (#859, `#agent-tools` 31/32) at its wrap

**Source:** `docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md`

## What remains

Since #859, `tddy-supervisor` hands a `serves_users` service exactly one per-user descriptor name,
`user.<os user>`, and the daemon adopts those through `tddy-daemon-kernel`'s
`agent_tool_sockets::adopt_user_sockets_from_environment`. The host-session service is served on
those sockets (`HostSessionSockets::serve_on`). Nothing hands over a `host-session.<user>` descriptor
any more, so this code is dead in production:

- `packages/tddy-agent-launch/src/inherited_host_sockets.rs` — the `host-session.` prefix
  (`HOST_SESSION_FD_NAME_PREFIX`, private), `adopt_from_environment`, `adopt_descriptors` and
  `InheritedHostSocket`;
- `HostSessionSockets::adopt_inherited` (`packages/tddy-agent-launch/src/host_session_socket.rs`),
  which only those feed;
- the adoption tests in
  `packages/tddy-session-lifecycle/src/connection_service/inherited_host_sockets_tests.rs`
  (lines ~95–260: `adopts_a_listener_owned_by_its_user_and_closed_to_everybody_else` through
  `does_not_rebind_an_inherited_socket_whose_file_was_removed`), which keep it compiling and green.

`OwnerSocketKind::HOST_SESSION` (`packages/tddy-daemon-kernel/src/owner_socket.rs`) is still used by
the embedded host's own bind of `<data dir>/run/<user>/host.sock`, but its `setting: "host_sockets"`
names a `supervisor.yaml` key that is a load error since #859 — a refusal reaching an operator would
point them at a setting that no longer exists. It should either name nothing supervisor-side (an
embedded host is never supervised) or go with the rest once the embedded host serves host sessions on
its one agent-tool socket too.

## What would close it

Delete `inherited_host_sockets.rs`, `HostSessionSockets::adopt_inherited` and the adoption tests
above, keeping the merged-socket tests in the same file (`a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket`,
`a_supervised_host_session_refusal_names_users_yaml`,
`the_supervisor_and_the_daemon_share_one_user_socket_prefix`); then retire or rename
`OwnerSocketKind::HOST_SESSION`'s setting as described.

## Why it stays

Deleting a module file needs developer consent (CLAUDE.md: ask before deleting files), and #859's
wrap ran unattended. The code is unreachable rather than wrong, so leaving it costs only reading
time.
