# 2026-11-07 — Server daemons serve an agent-tool socket per OS user

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)), branch
`feature/agent-tools/agent-tool-socket-per-user`, based on `feature/agent-tools/split-remote-env`
(and beside `feature/agent-tools/host-registry`'s non-optional `coding_agents`).

Packages: `tddy-daemon-kernel`, `tddy-supervisor`, `tddy-daemon`, `tddy-agent-launch`,
`tddy-session-split`, `tddy-session-lifecycle`; `./install`, `supervisor.yaml.production`,
`dev.supervisor.yaml`, `tddy-e2e`.

## What changed

- **`tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets`.** Placements `Embedded` (Tddy
  Desktop: the runtime's one socket, refused if not served) and `PerOsUser` (the binary:
  `<data dir>/run-agent-tools/<user>/agent.sock`, dir `0700` / socket `0600`, bound lazily owner-only or
  adopted from the supervisor's `agent-tools.<user>` descriptors; path limit 103 bytes).
  `install_roster`, `adopt_inherited`, `ensure_bound` / `ensure_bound_blocking`, `stop_serving`.
- **`tddy_daemon_kernel::owner_socket`.** `bind_owner_only` and its helpers (`ensure_dir`,
  `clear_stale_socket`, `chown`, `undo`) and the descriptor vetting (`descriptors_named`,
  `adopt_descriptor`, `vet`, `require_listening`) moved here from `tddy-agent-launch` as `pub`, the
  refusal parameterised by `OwnerSocketKind` (`HOST_SESSION` / `AGENT_TOOL`). `HostSessionSockets`
  calls it.
- **Removed:** `tddy_daemon_kernel::embedded_host_agent_tool_socket` and its 3 unit tests; the
  agent-launch `socket_for_hooks` / `SessionWiring.hook_socket`; split's
  `agent_tool_socket_for_embedded_host`.
- **Supervisor.** `ManagedService.agent_tool_sockets: Vec<HostSocket>`, validated with `host_sockets`
  by `host_socket_config::validate_kind`; at most 15 of both kinds; handed over after the host
  sockets (`SocketHandover::with_agent_tool_socket`), prefix `AGENT_TOOLS_FD_NAME_PREFIX`.
- **Daemon runtime.** `BuildContext` builds the `AgentToolSockets` from `RuntimeHost`, adopts the
  inherited descriptors before the local socket server clears `LISTEN_*` (`mount_host.rs`), installs
  the finished roster (`serve_common_room`), and gives it to `DaemonSessionHost`
  (`with_agent_tool_sockets`) and `DaemonLauncherDeps`.
- **Consumers.** `LaunchSessions::agent_tool_socket_of` and `SplitSessions::agent_tool_socket_of`
  resolve the session user's socket before a start cuts anything (`FAILED_PRECONDITION` on refusal);
  hooks, notify pairs, the project-home main, split / jailed-codebase agents and
  `CodingAgentToolHost::tool_server` all take that path. Hooks therefore carry `--daemon-socket` on
  every host (golden `host_start_settings_local.json` updated).
- **Shutdown.** `DaemonSessionHost::shut_down_children` calls `stop_serving` last; it removes only
  sockets the daemon bound itself (a bug unlinking served supervisor sockets was fixed in this PR).
- **Install.** `declare_agent_tool_sockets` / `survey_agent_tool_sockets` write a live
  `agent_tool_sockets:` block for every `spawn_policy.allowed_session_users` user while the
  `tddy-daemon` service declares none.

## Decisions

- **Design A** (developer, 2026-11-06) over B (serving the roster on `host.sock`): keeps the host-session
  socket at its two RPCs; costs a second handed-over listener per user, halving per-host user capacity.
- **Placement** (2026-11-07): `AgentToolSockets` in `tddy-daemon-kernel`, with `tddy-stdio` as a normal
  dependency — `tddy-daemon` was impossible because the consumers sit below it, and a `pub(crate)`
  `bind_owner_only` in `tddy-agent-launch` could not reach `tddy-session-split`.
- **Behaviour** (2026-11-07): under the supervisor an unlisted user's start is **refused**, naming
  `agent_tool_sockets` (unlike the host-session socket, whose absence only refuses its two RPCs); a
  `DaemonSessionHost` with no runtime roster binds and accepts with an empty roster.
- **Install writes live entries** (2026-11-07): rather than only documenting the setting, because an
  upgrade that kept an operator's `supervisor.yaml` would otherwise stop every session of its policy
  users. An existing block is the operator's (warn only); refusals leave the file unchanged.
- **Shape:** kept one node (complexity 5; pass 2 assessed 6), with the supervisor half as a split
  contingency that was not needed.
- The held server binding (`wip/agent-tools-cursor-cli-server-socket`, `07128a63e`) was **not**
  re-applied — one socket for the daemon's user is the EACCES defect design A removes; its 3 tests were
  re-applied per user.

## Validation

- Acceptance: `tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` (server binds and serves per
  user; a session told its own user's socket), `tddy-session-lifecycle/tests/agent_tool_socket_per_user_acceptance.rs`
  (host Claude and Cursor hooks report over the user's served socket), `tddy-e2e/tests/install_supervisor.rs`
  (`an_upgrade_declares_a_live_agent_tool_socket_for_every_user_the_policy_allows_sessions_as`,
  `a_fresh_install_declares_no_agent_tool_socket_while_the_policy_allows_no_session_user`).
- Unit: `tddy-daemon-kernel/src/agent_tool_sockets_tests.rs` (incl.
  `an_unlisted_user_under_the_supervisor_is_refused_naming_the_setting`,
  `stopping_leaves_a_socket_the_supervisor_made_where_it_is`); `tddy-supervisor` `host_socket_config.rs`
  (`supervisor_config_rejects_a_shared_dir_and_too_many_listeners` and parity), `supervisor.rs`
  (`agent_tool_listeners_are_announced_as_agent_tools_user`); the FD-name prefix pinned across crates by
  `reads_the_same_agent_tool_descriptor_names_the_supervisor_writes`.
- **Not provable on macOS unprivileged** (Linux/VM only): a second real account refused a `connect`
  to another user's socket; a root supervisor handing a socket to another account; an unlisted user
  refused under a real supervisor. Pinned here by mode/owner checks.
- Code issues: the complexity rows this PR moved are recorded in the touched records under
  `packages/tddy-agent-launch/docs/code-issues/` and `packages/tddy-session-split/docs/code-issues/`.

## Final validation (`/validate-changes`, `/pr-wrap`, 2026-11-07)

- **Fixed — a served supervisor socket was unlinked at shutdown.** `stop_serving` told the two kinds
  apart by the inherited map, whose entry is taken once served, so the next daemon was handed a
  listener at a path nobody could reach. A server now records whether it bound the file itself.
- **Fixed — a re-attached agent lost its socket across a daemon restart.** A fresh daemon serves a
  user's socket only when something asks for it, and the tmux re-attach of an agent that outlived
  its daemon never asked; before this PR those hooks went over HTTP, which comes back with the
  daemon. `TmuxMainReattach` now serves the session user's socket before re-attaching (a user it can
  no longer serve is logged and still gets the terminal). Test:
  `a_fresh_daemon_serves_the_reattached_agents_tool_socket_again`.
- **Checked, no change:** `ensure_bound_blocking` on a runtime worker (std locks held only for
  bounded syscalls, never across an `.await`; documented); `MAX_SOCKET_PATH_BYTES` 103 fits macOS's
  104-byte `sun_path` with its NUL (Linux: 108).
- **Kept, with reason:** `DaemonSessionHost::new` still defaults a `PerOsUser` registry with no
  roster rather than taking one — the parent's contract keeps `new`'s signature, and the one
  production builder (`mount_host.rs`) always replaces it. Inherited agent-tool descriptors are
  adopted (made close-on-exec) only when a session host is built, as the host-session ones are.
- The accept-failure log no longer promises a rebind for a supervisor-made socket.
- `resumed_session_wiring` gives back the six lines a wrapped call cost (79 → 80).

## Resolved backlog

- `docs/dev/todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md` — "A server daemon's host
  Cursor hooks need a per-OS-user agent-tool socket" (design A built here).

Product docs: [tddy-supervisor.md](../../ft/supervisor/tddy-supervisor.md),
[systemd-install.md](../../ft/daemon/systemd-install.md),
[claude-cli-session.md](../../ft/daemon/claude-cli-session.md),
[cursor-cli-session.md](../../ft/daemon/cursor-cli-session.md).
