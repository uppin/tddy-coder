# Per-OS-user daemon sockets (`users.yaml`)

The supervisor creates **one** owner-only socket per OS user `users.yaml` maps and hands it to the
daemon. A session of that user reaches everything it needs from the daemon over it:

| Served on `user.<user>` | Used by |
|---|---|
| the daemon's whole RPC roster | hook reports (`tddy-tools session-hook`), `tddy-tools notify`, the `tddy-tools --mcp` tool server |
| the host-session service (`github_token`, `spawn_conversation`) | `tddy-coder` tool sessions (see [session identity](../../tddy-session-lifecycle/docs/session-identity.md)) |

A daemon that can `chown` binds the socket itself. Under `tddy-supervisor` the daemon is an
unprivileged child and cannot give a socket to a session user, so the supervisor creates each one as
root and hands the listener to the daemon. A supervised daemon derives and binds no socket path of its
own (`AgentToolSocketPlacement::Supervised`, see
[daemon kernel § Per-OS-user sockets](../../tddy-daemon-kernel/docs/daemon-kernel.md#per-os-user-sockets-owner_socket-agent_tool_sockets)):
a session for an OS user it was handed no socket for **cannot start**, and the start fails
`FAILED_PRECONDITION` naming `users.yaml`, rather than pointing the agent's hooks at a path it cannot
reach and losing every report to their fail-quiet contract.

> **Status: built and unit-proven; a real run under a root supervisor with two accounts is pending.**
> Every test runs as the current user. See [What is not proven](#what-is-not-proven).

## The mapping (`users_file.rs`)

`UsersFile::load(path)` reads `users.yaml`, a YAML list of `MappedUser { github_user, os_user }` rows —
the shape `daemon.yaml` `users:` had. The supervisor uses only the `os_user` column
(`UsersFile::os_users()`, distinct and sorted); the GitHub logins are the daemon's.

- **Trust** is the check `SupervisorConfig::load` makes (`require_trustworthy_config_file`,
  `require_trustworthy_config_directory`): owned by root (or the supervisor's own uid, which is root on
  a real host), writable by nobody else, in directories nobody else can write.
- **Shape**: no `root`; a GitHub login mapped to two OS users is refused; every OS user is one plain
  path and descriptor-name segment (ASCII letters, digits, `_`, `-`, `.`; not starting with `.`).
- **Budget**: at most `MAX_MAPPED_OS_USERS` = 15 distinct OS users — `MAX_HANDED_OVER_LISTENERS` (16)
  less descriptor 3. A sixteenth fails loading, naming `users.yaml` and the limit.
- **Missing** is an error, never an empty mapping.

## Configuration

`supervisor.yaml` names the file and the directory its sockets live under in a top-level block, and the
service that is handed them declares `serves_users: true`:

```yaml
users:
  file: /etc/tddy/users.yaml       # absolute; required once a service serves users
  sockets_dir: /run/tddy-users     # absolute; the default
services:
  - name: tddy-daemon
    socket: {path: /run/tddy-daemon.sock}
    serves_users: true
```

Validated at load (`host_socket_config.rs`, `validate`): a service that serves users needs the top-level
`users:` block and its own `socket:` — descriptor 3 stays that. Both paths must be absolute.
`spawn_policy.allowed_session_users`, `services[].host_sockets` and `services[].agent_tool_sockets` are
refused before typed parsing (`refuse_old_user_list_keys` in `config.rs`), with an error naming
`users.yaml` and the key to delete.

`spawn_policy.allowed_env_keys` is a deny, not a filter: a request naming a key it does not list is
refused. A project that resolves to an account sends the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*` keys,
so `supervisor.yaml.production`, `dev.supervisor.yaml` and `daemon.yaml.production` list them, and a
supervised host must list them or every tool-session spawn for such a project is refused. That fails
loudly rather than starting the session under the machine's identity.

## Creation (`user_sockets.rs`, `host_socket.rs`)

`UserListeners::bind(service, users, sockets_dir)` makes one socket per mapped OS user at
`user_socket_path(sockets_dir, user)` = `<sockets_dir>/<user>/daemon.sock`, through `host_socket.rs`:

1. every directory above the user's must be writable by root alone (checked);
2. the user's directory is opened `O_NOFOLLOW | O_DIRECTORY` (by descriptor, so a link or file standing
   in for it is refused) and taken back to root `0700`;
3. a stale socket is removed (only a socket ever is), the socket is bound and set `0600`;
4. the socket is `lchown`ed to the user;
5. **only then** the directory is `fchown`ed to the user.

So nothing the user controls is in the path while root works. A mapped user whose socket cannot be
created stops the supervisor from being built.

## Hand-over (`handover.rs`)

The supervisor passes the listeners to the `serves_users` service after its own: descriptor 3 is the
service socket, then one listener per mapped OS user in OS-user order
(`SocketHandover::with_user_socket`), announced as `LISTEN_FDS=<n>` and
`LISTEN_FDNAMES=connection:user.<user>:…`. The service is told the mapping's path in
**`TDDY_USERS_FILE`** (`spawn_broker::USERS_FILE_ENV`), set after its `env:` overlay so
`supervisor.yaml` can neither set nor forge it; no socket path is handed separately — the daemon reads
each one from its listener. The prefix (`USER_SOCKET_FD_NAME_PREFIX` = `user.`) and the variable name
are spelled again on the daemon side, which does not link the supervisor for them;
`inherited_host_sockets_tests.rs` in `tddy-session-lifecycle` pins both
(`the_supervisor_and_the_daemon_share_one_user_socket_prefix`). Each listener is copied above the slots
before any is placed, so one listener's descriptor can never be another's slot, and the supervisor
keeps descriptors 3–18 occupied so nothing it opens lands on one.

The listeners are held for as long as the supervisor runs, so a restarted service is handed the same
ones.

## A changed `users.yaml` (`users_watch.rs`, `supervisor/users_change.rs`)

`UsersWatch::spawn` watches the file's **directory** with the `notify` crate, so an editor's
write-to-temporary-then-rename is seen. It refuses to start if the path cannot be canonicalized. Each
event for the file re-reads it through `UsersFile::load`:

- **valid** — it becomes the current mapping (`UsersWatch::current`) and is reported to
  `UsersWatchSink::changed`;
- **invalid** — it is reported to `UsersWatchSink::refused` and logged; the mapping in force is kept.

The sink in `lib.rs` applies changes **one at a time**, in the order they were saved (a
`tokio::sync::Mutex` around each apply), because two overlapping applies would each stop and start the
same service. `Supervisor::apply_users_change(next)`:

1. rebuilds the listeners of every `serves_users` service with `UserListeners::rebuild` — a user both
   mappings name keeps the very listener they had (the same `Arc`), a user only `next` names is bound
   afresh, a user only the old set has is dropped. The new listeners exist before the old daemon is
   stopped, so an unchanged user's clients queue in the kernel across the restart;
2. stops each such service: `SIGTERM`, then `SIGKILL`, waiting at most `RESTART_STOP_TIMEOUT` (10 s)
   after each. A service that survives both is reported as an error and not restarted — its stop was
   requested, so a later exit would otherwise read as "stopped on request" and never be restarted;
3. starts it again with the rebuilt handover.

Session spawns read the same current mapping: `policy::resolve_mapped_session_user` accepts only an OS
user it names, and refuses `root` whatever it says.

## Adoption in the daemon

The daemon reads the names **before** adopting its own listener clears `LISTEN_*`
(`runtime/mount_host.rs`, `adopt_user_sockets_from_environment`), and adopts a descriptor only if it is
an open, listening (Linux) unix socket bound to a path whose file is a socket **owned by the user it is
named for with no group or other bits**. It sets close-on-exec and serves the socket where it is. It
never binds or replaces an inherited socket, and never removes one — a daemon that stops closes its
copy of the listener and leaves the file to the supervisor, which hands it to the next daemon it starts.
If the file vanishes the daemon says to map the user in `users.yaml` and restart the supervisor. Code:
`tddy-daemon-kernel`'s `owner_socket.rs` (`descriptors_named`, `adopt_descriptor`, `vet`,
`require_listening`), used by `agent_tool_sockets.rs` (`user_socket_fds`, `adopt_user_descriptors`).

## Who can connect

The mapped OS user, and root: the socket and its directory are owned by that user, `0600` and `0700`;
connecting needs search on the directory and write on the socket. The daemon is not among them and does
not need to be — a file's mode is checked at `connect`, never at `accept`. Another session user, the
daemon's own account and other groups are refused by the kernel; within one user's socket,
`HostSessionService` still refuses a session registered for a different OS user.

The socket path comes from the supervisor, not the daemon's data dir, and sits under directories only
root can write (`/run/tddy-users/<user>/daemon.sock`). A tool session running in a sandbox that does not
bind-mount that path cannot reach it (unverified).

## What is not proven

Unprovable on a host without root and a second account:

- the `chown` / `fchown` to a *different* uid;
- the unprivileged daemon accepting on a socket it does not own;
- the real fork/exec descriptor hand-over through a root supervisor;
- a sandboxed tool session reaching the socket;
- the final `fchown` of the directory to the user survives a mutation (removing it fails no test).

The handover and the rebuild are proven by unit tests run as the current user (`user_sockets.rs`,
`handover.rs`, `supervisor.rs`: `creates_every_mapped_users_daemon_socket_when_it_is_built`,
`hands_the_service_its_own_socket_first_and_every_user_socket_after_it`; daemon side
`server_agent_tool_socket_acceptance.rs` and
`a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket`). A second real
account being refused a `connect` to another user's socket needs Linux, root and two accounts.

`tests/supervisor_user_sockets.rs` holds two Linux-only integration tests against a real supervisor
(`LISTEN_FDNAMES`, `TDDY_USERS_FILE` and the socket at its derived path at exec; a saved `users.yaml`
restarting the daemon with descriptor 4's inode unchanged), and `tests/supervisor_socket_handoff.rs`
the service-socket ones. They compile out on macOS and run on CI: spawning a managed service needs
Linux (`PR_SET_PDEATHSIG`). This path is called done only after one run of a root supervisor with two
accounts and a session for each, asserting that the other account's socket refuses a connection.
