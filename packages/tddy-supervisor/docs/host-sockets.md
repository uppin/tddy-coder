# Host-session sockets (`host_sockets:`)

A tool session reaches the daemon over **one unix socket per OS user** (`github_token`,
`spawn_conversation`; see
[session identity](../../tddy-session-lifecycle/docs/session-identity.md)). A daemon that can `chown`
binds that socket itself. Under `tddy-supervisor` the daemon is an unprivileged child and cannot give a
socket to a session user, so the supervisor creates each socket as root and hands the listener to the
daemon.

> **Status: built and unit-proven; a real run under a root supervisor with two accounts is pending.**
> Every test runs as the current user. See [What is not proven](#what-is-not-proven).

## Configuration

A managed service lists `host_sockets: [{user, path}]` in `supervisor.yaml`
(`supervisor.yaml.production` documents it). Validation at load (`host_socket_config.rs`):

- the service needs its own `socket:` — descriptor 3 stays that;
- each `user` is in `spawn_policy.allowed_session_users` and is not `root`;
- one entry per user;
- one directory per socket, which nothing else shares and no other declared socket lives inside — the
  directory is handed to the user wholesale;
- at most 15 entries (`MAX_HANDED_OVER_LISTENERS`); paths must fit a unix socket (about 100 bytes).

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
4… are the host sockets in declaration order, announced as `LISTEN_FDS=<n>` and
`LISTEN_FDNAMES=connection:host-session.<user>:…`. Each listener is copied above the slots before any
is placed, so one listener's descriptor can never be another's slot, and the supervisor keeps
descriptors 3–18 occupied so nothing it opens lands on one.

## Adoption in the daemon

The daemon reads the names **before** adopting its own listener clears `LISTEN_*` (`runtime.rs`), and
adopts a descriptor only if it is an open, listening (Linux) unix socket bound to a path whose file is
a socket **owned by the user it is named for with no group or other bits**. It sets close-on-exec and
serves the socket where it is. It never binds, replaces or removes an inherited socket; if the file
vanishes it says the supervisor must be restarted. Code:
`tddy-session-lifecycle/src/connection_service/inherited_host_sockets.rs`.

An OS user that is not declared keeps the explicit refusal, which names `host_sockets:` in
`supervisor.yaml`.

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

`tests/supervisor_socket_handoff.rs` holds three Linux-only integration tests (socket owner and mode,
descriptor 4 and `LISTEN_FDNAMES` seen by a real child, no host socket means no descriptor 4). They
compile but have not been run: spawning a managed service needs Linux (`PR_SET_PDEATHSIG`). This path
is called done only after one run of a root supervisor with two accounts and a session for each,
asserting that the other account's socket refuses a connection.
