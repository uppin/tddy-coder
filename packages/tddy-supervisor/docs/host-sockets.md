# Per-OS-user sockets (`host_sockets:`, `agent_tool_sockets:`)

The supervisor creates two kinds of owner-only socket per session OS user and hands them to the daemon:

| Kind | `supervisor.yaml` key | Descriptor name | What a session of that user reaches over it |
|---|---|---|---|
| Host-session | `host_sockets:` | `host-session.<user>` | `github_token`, `spawn_conversation` (see [session identity](../../tddy-session-lifecycle/docs/session-identity.md)) |
| Agent-tool | `agent_tool_sockets:` | `agent-tools.<user>` | the daemon's whole RPC roster: hook reports (`tddy-tools session-hook`), `tddy-tools notify` and the `tddy-tools --mcp` tool server |

A daemon that can `chown` binds both itself. Under `tddy-supervisor` the daemon is an unprivileged child
and cannot give a socket to a session user, so the supervisor creates each socket as root and hands the
listener to the daemon. The two kinds share one code path in both processes: `host_socket.rs` creates
them here, and the daemon binds and adopts them through
`tddy-daemon-kernel`'s `owner_socket.rs` (see
[daemon kernel § Per-OS-user sockets](../../tddy-daemon-kernel/docs/daemon-kernel.md#per-os-user-sockets-owner_socket-agent_tool_sockets)).

**What a missing entry does differs.** A user with no `host_sockets:` entry starts sessions without a
host-session socket (the two RPCs refuse, naming `host_sockets:`). A user with no
`agent_tool_sockets:` entry **cannot start a session at all**: the start fails `FAILED_PRECONDITION`,
naming `agent_tool_sockets` in `supervisor.yaml`, rather than pointing the agent's hooks at a path it
cannot reach and losing every report to their fail-quiet contract. `./install` writes the
`agent_tool_sockets:` entries from `spawn_policy.allowed_session_users` (see
[systemd install](../../../docs/ft/daemon/systemd-install.md#behavior-notes)).

> **Status: built and unit-proven; a real run under a root supervisor with two accounts is pending.**
> Every test runs as the current user. See [What is not proven](#what-is-not-proven).

## Configuration

A managed service lists `host_sockets: [{user, path}]` and `agent_tool_sockets: [{user, path}]` in
`supervisor.yaml` (`supervisor.yaml.production` documents both; both default to empty). Both kinds are
validated at load by the same rules (`host_socket_config.rs`, `validate_kind` per kind):

- the service needs its own `socket:` — descriptor 3 stays that;
- each `user` is in `spawn_policy.allowed_session_users`, is not `root`, and is a plain account name
  (ASCII letters, digits, `_`, `-`, `.`; not starting with `.`);
- one entry per user per kind;
- one directory per socket, which nothing else shares and no other declared socket lives inside — the
  directory is handed to the user wholesale. A user's host socket and agent-tool socket therefore live
  in **two** directories (for example `/run/tddy-host-sessions/<user>/host.sock` and
  `/run/tddy-agent-tools/<user>/agent.sock`);
- host and agent-tool sockets **together** at most 15 (`MAX_HANDED_OVER_LISTENERS` is 16, descriptor 3
  included), so a host serving both to every user fits seven users; paths must fit a unix socket
  (about 100 bytes).

`spawn_policy.allowed_env_keys` is a deny, not a filter: a request naming a key it does not list is
refused. A project that resolves to an account sends the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*` keys,
so `supervisor.yaml.production`, `dev.supervisor.yaml` and `daemon.yaml.production` list them, and a
supervised host must list them or every tool-session spawn for such a project is refused. That fails
loudly rather than starting the session under the machine's identity.

## Creation (`host_socket.rs`)

At startup the root supervisor makes each socket:

1. every directory above the user's must be writable by root alone (checked);
2. the user's directory is opened `O_NOFOLLOW | O_DIRECTORY` (by descriptor, so a link or file standing
   in for it is refused) and taken back to root `0700`;
3. a stale socket is removed (only a socket ever is), the socket is bound and set `0600`;
4. the socket is `lchown`ed to the user;
5. **only then** the directory is `fchown`ed to the user.

So nothing the user controls is in the path while root works.

## Hand-over (`handover.rs`)

The supervisor passes the listeners to the daemon after its own: descriptor 3 is the service socket,
then the host sockets in declaration order, then the agent-tool sockets in declaration order
(`SocketHandover::with_agent_tool_socket`), announced as `LISTEN_FDS=<n>` and
`LISTEN_FDNAMES=connection:host-session.<user>:…:agent-tools.<user>:…`. The two prefixes
(`HOST_SESSION_FD_NAME_PREFIX`, `AGENT_TOOLS_FD_NAME_PREFIX`) are spelled again on the daemon side, which
does not link the supervisor for them; `inherited_host_sockets_tests.rs` in `tddy-session-lifecycle`
pins both readers against the supervisor's writers. Each listener is copied above the slots before any
is placed, so one listener's descriptor can never be another's slot, and the supervisor keeps
descriptors 3–18 occupied so nothing it opens lands on one.

## Adoption in the daemon

The daemon reads the names **before** adopting its own listener clears `LISTEN_*`
(`runtime/mount_host.rs`), and adopts a descriptor only if it is an open, listening (Linux) unix socket
bound to a path whose file is a socket **owned by the user it is named for with no group or other
bits**. It sets close-on-exec and serves the socket where it is. It never binds or replaces an inherited
socket, and never removes one — a daemon that stops closes its copy of the listener and leaves the file
to the supervisor, which hands it to the next daemon it starts. If the file vanishes the daemon says the
supervisor must be restarted. Code: `tddy-daemon-kernel`'s `owner_socket.rs` (`descriptors_named`,
`adopt_descriptor`, `vet`, `require_listening`), used by `tddy-agent-launch`'s
`inherited_host_sockets.rs` for the host-session kind and by `agent_tool_sockets.rs` for the agent-tool
kind.

An OS user that is not declared keeps the explicit refusal, which names the kind's setting
(`host_sockets:` or `agent_tool_sockets:`) in `supervisor.yaml`.

## Who can connect

The declared OS user, and root: the socket and its directory are owned by that user, `0600` and `0700`;
connecting needs search on the directory and write on the socket. The daemon is not among them and does
not need to be — a file's mode is checked at `connect`, never at `accept`. Another session user, the
daemon's own account and other groups are refused by the kernel; within one user's socket,
`HostSessionService` still refuses a session registered for a different OS user.

The socket path comes from the supervisor, not the daemon's data dir, and must sit under directories
only root can write (for example `/run/tddy-host-sessions/<user>/host.sock`). A tool session running
in a sandbox that does not bind-mount that path cannot reach it (unverified).

## What is not proven

Unprovable on a host without root and a second account:

- the `chown` / `fchown` to a *different* uid;
- the unprivileged daemon accepting on a socket it does not own;
- the real fork/exec descriptor hand-over through a root supervisor;
- a sandboxed tool session reaching the socket;
- the final `fchown` of the directory to the user survives a mutation (removing it fails no test).

For the agent-tool kind the same three facts are proven only by unit tests (`supervisor.rs`:
`creates_every_declared_agent_tool_socket_when_it_is_built`,
`agent_tool_listeners_are_announced_as_agent_tools_user`; daemon side
`server_agent_tool_socket_acceptance.rs` with a socket for the account running the test). A second
real account being refused a `connect` to another user's agent-tool socket needs Linux, root and two
accounts.

`tests/supervisor_socket_handoff.rs` holds three Linux-only integration tests (socket owner and mode,
descriptor 4 and `LISTEN_FDNAMES` seen by a real child, no host socket means no descriptor 4). They
compile but have not been run: spawning a managed service needs Linux (`PR_SET_PDEATHSIG`). This path
is called done only after one run of a root supervisor with two accounts and a session for each,
asserting that the other account's socket refuses a connection.
