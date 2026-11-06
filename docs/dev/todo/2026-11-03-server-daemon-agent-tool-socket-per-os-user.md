# 2026-11-03 — A server daemon's host Cursor hooks need a per-OS-user agent-tool socket

**Category:** Blocked developer decision from `#agent-tools` 7/12 (#796)
**Source:** `/validate-changes` on `feature/agent-tools/cursor-cli`. The held implementation is on
the local ref `wip/agent-tools-cursor-cli-server-socket` (`07128a63e`).

**Starting point:** the local ref `wip/agent-tools-cursor-cli-server-socket` (`07128a63e`, on top of #796's `33c53ed34`): the server binding in `build_context.rs`, `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` (3 tests, red → green) and the doc edits across 6 crates. #796 shipped without it (developer decision, 2026-11-03).

## What is there

#796 renders a host Cursor session's `.cursor/hooks.json` with
`--daemon-socket <agent_tool_socket_path(data_dir)>` and no HTTP fallback. Only an **embedded**
daemon (Tddy Desktop) binds that socket (`packages/tddy-daemon/src/runtime/build_context.rs`). So on
a **server** daemon, a host Cursor session's hook reports go to a socket nothing serves and are
dropped fail-quiet. Before #796 they went over HTTP (`install_cursor_hooks_in_worktree`) and worked.

The developer decided (2026-11-03) to have server daemons bind the socket too. That change is
written and tested: `server_agent_tool_socket_acceptance.rs` holds 3 tests, red then green, and
Claude's suites passed with it. It is **held**, because binding one socket for the daemon's own user
is unsafe on a multi-user host:

- `agent_tool_socket_path` is under the daemon user's `$TMPDIR`, bound with the default mode.
- Under `tddy-supervisor`, sessions run as other OS users (`spawn_policy.allowed_session_users`).
  Those users get EACCES on the socket.
- `embedded_host_agent_tool_socket` is a connect probe *from the daemon*, so it answers `Some`.
  Claude's hook, notify and tool transport (`claude_cli_spawn_steps.rs`, `notify_env.rs`, `chat.rs`,
  `project_home_launch.rs`, `tddy-session-split` `coding_agent_wiring.rs` and
  `svc_resolve_tddy_tools_path.rs`) would then switch from HTTP to a socket those sessions cannot
  reach, and lose every report silently.

The developer's fix is **one agent-tool socket per OS user**, following
`packages/tddy-agent-launch/src/host_session_socket.rs`: a `0700` directory and a `0600` socket,
owned by that user, with supervisor-inherited listeners where the daemon cannot `chown`.

## Why it is blocked: the supervisor hands over one socket per user

`tddy-supervisor` creates exactly one socket per `host_sockets:` entry. It names that socket
`host-session.<user>` in `LISTEN_FDNAMES` (`packages/tddy-supervisor/src/socket.rs`). The validation
in `host_socket_config.rs` requires that socket's directory to be "used by this socket alone". An
unprivileged daemon under the supervisor therefore cannot bind a second socket for that user, and the
supervisor has no way to hand it one. That is a supervisor protocol change, which this unattended
stage was told not to make.

## Design A — the developer's direction (needs supervisor changes)

1. **Supervisor**
   - Add `agent_tool_sockets: [{user, path}]` per service, or a `kind:` on `HostSocket`.
   - Announce the descriptor as `agent-tools.<user>`.
   - Validate it like `host_sockets`: the user is allowed, the directory is distinct, and both
     socket kinds together fit `MAX_HANDED_OVER_LISTENERS`. Two sockets per user halve how many
     users fit.
   - Pin the new FD-name prefix in both crates, as the host-session prefix test does.
2. **Daemon**
   - Add an `AgentToolSockets` registry, a twin of `HostSessionSockets`.
   - Path: `<data dir>/run-agent-tools/<user>/agent.sock`. It is a separate directory because the
     supervisor hands `run/<user>/` over to the host socket alone.
   - Bind lazily with `bind_owner_only`, widened to `pub(crate)` and shared, not copied.
   - Adopt inherited `agent-tools.<user>` listeners the way `inherited_host_sockets` adopts the
     host-session ones.
   - Serve the roster `build_context.rs` already builds.
3. **Consumers**
   - Replace the daemon-side probe `embedded_host_agent_tool_socket(data_dir)` with
     `AgentToolSockets::ensure_bound(session_os_user, data_dir)`. It is async and refuses an unlisted
     user under the supervisor with the setting to add, instead of handing out a path that user
     cannot reach.
   - Thread the registry through `LaunchSessions` / `DaemonSessionHost` to every caller listed
     above, plus Cursor's `spawn_cursor_from_spec` and `colocated_jail_tool_env`.
4. **Tests, test first**
   - A session running as user X connects to X's socket and is served.
   - An unlisted user under the supervisor is refused at start, not dropped silently.
   - The single-user paths (`./web-dev`, `--user`, desktop) still report.
   - Claude's hook and tool suites pass on a server daemon.

## Design B — no supervisor change (an alternative to put to the developer)

Serve the daemon's tool roster on the per-user **host-session** socket that already exists.
`HostSessionSockets::serve` would answer a `MultiRpcService` of `HostSessionService` plus the
roster. Consumers would take that path from `HostSessionSockets::ensure_bound(os_user)`, which the
Claude start already calls (`svc_start_claude_cli_session.rs:210`).

- **For:** the same principal can reach it as under A (only that user, by file mode), and it reuses
  the supervisor hand-off as it is.
- **Against:** it widens `host.sock` from its two RPCs to the whole roster, and couples two
  channels the code documents separately. The consumer plumbing (point 3 of A) is the same.

## What would close it

The developer picks A or B. Then apply the held ref `wip/agent-tools-cursor-cli-server-socket`
(server binding plus its 3 tests), build the chosen per-user design on top, test first, and re-run
Claude's and Cursor's host suites on a server daemon. Natural owner: a follow-up node of
`#agent-tools` before the sweep, since #797–#799 do not depend on it.
