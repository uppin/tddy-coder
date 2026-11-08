# tddy-supervisor

`tddy-supervisor` is the single root-run process on a supervised host. systemd starts it instead of
`tddy-daemon`; it starts the daemon as an unprivileged child and brokers the operations that genuinely
need privilege.

## Why it exists

Before it, `./install --systemd` offered two deployments and both were wrong for a multi-user host.

**Root mode** (`INSTALL_DAEMON_USER=root`) was the only one where multi-user session isolation worked:
the daemon could `setgid`/`initgroups`/`setuid` into a per-GitHub-user OS account. The price was that
*everything else* in the daemon ran as uid 0 too — the axum HTTP server and Connect-RPC router, the
LiveKit participant, GitHub OAuth and its token store, the Telegram bot, the LSP executor, the BSP
catalog, the PTY runtime, the VM manager. Roughly 65 modules and a large third-party dependency tree,
all reachable from the network, all root.

**Unprivileged mode** (`User=tddy`) was safe but bought safety by giving up the feature: `same_user`
evaluated true, the privilege-drop was skipped entirely, and *every* session ran as the single `tddy`
account with no isolation between users. It also needed two host grants to sandbox at all — cgroup v2
delegation and an AppArmor grant for unprivileged user namespaces.

The supervisor keeps the isolation and removes the root surface: privilege lives in one small binary
with a root-owned declarative policy, and the daemon runs as `tddy`.

## What an operator sees

One systemd unit, `tddy-supervisor.service`, running as root with `Delegate=yes`. `tddy-daemon` appears
as its child, running as `tddy`. No `tddy-daemon.service` is installed, and an inherited one is stopped,
disabled and **masked** — socket first, because `systemctl disable` does not disarm an already-listening
socket and a stray `connect()` in that window would relaunch the old daemon.

`supervisor.yaml` is the entire policy surface, root-owned:

```yaml
socket:
  path: /run/tddy-supervisor.sock
  group: tddy-clients
  mode: "0660"

users:                             # the GitHub → OS-user mapping and its per-user sockets
  file: /etc/tddy/users.yaml
  sockets_dir: /run/tddy-users     # default; <sockets_dir>/<os user>/daemon.sock

services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    args: ["-c", "/etc/tddy/daemon.yaml"]
    user: tddy
    socket:                        # the supervisor binds this and hands it over as fd 3
      path: /run/tddy-daemon.sock
      group: tddy-clients
      mode: "0660"
    serves_users: true             # handed TDDY_USERS_FILE and one `user.<os user>` socket per mapped user
    restart: { max_retries: 5, initial_backoff_ms: 500, max_backoff_ms: 30000, stability_threshold_ms: 10000 }

spawn_policy:                      # absent = no privilege granted at all
  allowed_tool_paths: [/usr/local/bin/tddy-coder, /usr/bin/git]
  allowed_relay_tool_paths: []      # harness binaries a coding agent may run as, with relayed stdio; empty = off
  allowed_mount_roots: [/srv/tddy/repos]
  allowed_env_keys: [RUST_LOG]

cgroup:                            # absent = nothing is clamped
  memory_max_ceiling: 2147483648
  pids_max_ceiling: 512
```

Nothing is granted by default. An operator who writes no `spawn_policy` has a supervisor that supervises
the daemon and refuses every spawn request — which is the correct reading of an absent policy.

The OS users a session may run as are not in `supervisor.yaml` at all: they are the OS users
**`users.yaml`** maps (see [The user mapping](#the-user-mapping-usersyaml)). `root` is refused whatever
the mapping says.

### Relayed spawns (coding agents)

`SpawnRelayed` starts an allowlisted harness binary (`opencode`, `codex-acp`, `claude-agent-acp`) as a mapped session
user with its stdin/stdout relayed over a unix socket the **daemon** listens on — the one request that gives
a child stdio, because the ACP driver talks to its agent over stdio and an unprivileged daemon cannot become
another user itself. It passes the gates `SpawnSession` does (peer authorization, an OS user `users.yaml` maps; its
environment is the daemon's, passed whole but for loader variables) **and** its own: `allowed_relay_tool_paths`, **empty by default**, so enabling session
tools never enables it. The supervisor connects to the daemon's socket only if the path is absolute, is a
socket (not a symlink to one) owned by the calling uid, and the listener's `SO_PEERCRED` uid is that uid; any
failure is the opaque denial and nothing is forked. The child's exit status and a stop travel over
`SessionStatus` / `StopSession`; a daemon that closes the connection stops the child.

### The prerequisites that bite

- **`allowed_tool_paths` must include git's absolute path** or repo cloning is denied, because cloning
  goes through the supervisor too. The supervisor finds git as the first executable `git` on **its own**
  `PATH` (the unit's environment, never the caller's), so the value is `which git` in the supervisor's
  environment; matching is verbatim, with no symlink resolution.
- **A coding agent runs as the session user only if its harness is in `allowed_relay_tool_paths`.** The
  list is empty in the shipped template, so a supervised install starts no coding-agent subagent as
  another user until the operator lists each harness binary by the absolute path the daemon resolves
  (`codex-acp` in the install's bin directory, `opencode`, and for `claude-acp` the `claude-agent-acp`
  binary the daemon's `TDDY_CLAUDE_AGENT_ACP_CLI` names). Without that variable `claude-acp` runs as
  `bunx claude-agent-acp`; arguments are not allowlisted, so listing `bunx` admits any package `bunx`
  can fetch, run as the session user. A relayed agent's environment is the one the daemon computed,
  passed whole rather than checked against `allowed_env_keys`; a loader variable is still refused. The
  daemon turns the supervisor's opaque denial into an error naming the lists that still gate it.
- **`allowed_env_keys` is a deny, not a filter.** A request naming an unlisted key is refused outright.
  An ordinary session spawn sends no environment, so the shipped empty policy works — but a user whose
  `~/.tddy/config.yaml` sets `spawn_path_extra` makes the daemon send `PATH`, which must then be listed.
- **Loader keys can never be listed.** `LD_*`, `MALLOC_*`, `GCONV_PATH`, `NLSPATH`, `LOCPATH`,
  `HOSTALIASES` and `RESOLV_HOST_CONF` are rejected at config load: `LD_PRELOAD` on an allowlisted tool
  is code execution as the session user, which would make the tool allowlist meaningless.
- **The config must be root-owned.** The supervisor stats what it parsed, including ancestors, and
  refuses a file or directory that is not owned by uid 0 or is group/world-writable without being
  sticky. `mkdir -p /etc/tddy` under a permissive umask would otherwise let any local user rewrite the
  policy the root broker enforces.

## What it does

**Mini-init.** Starts declared services in declaration order, drops to their declared user, reaps them,
restarts with exponential backoff, gives up at a retry ceiling, and takes everything down on `SIGTERM`.
A service that stays up past its stability threshold gets its full retry budget back, so a daemon
healthy for a week is not abandoned on its first crash.

**Privileged broker**, on a root-owned unix socket, authorized by peer credentials against the uids
owning declared services:

| Operation | Notes |
|---|---|
| `ListServices` / `StartService` / `StopService` | by name from the root-owned config; a caller can never name a binary |
| `SpawnSession` | an allowlisted tool as an allowlisted OS user |
| `SpawnSandbox` | the same, jailed: user/mount/net namespaces, bind mounts, private root |
| `SessionStatus` / `StopSession` | the supervisor reaps its own children, so it is the only process that can answer |
| `CloneRepository` | `git clone --progress` as an allowlisted OS user, **streamed**: `Started{pid}`, git's progress lines, `Exited{code, stderr_tail}`. Same user, tool (git on the supervisor's `PATH`, allowlisted) and environment checks as `SpawnSession`; a refusal fails the call. Clones into a random `.partial-*` sibling renamed into place on success, removed otherwise. `StopSession(pid)` stops it, as does the caller hanging up |
| `CreateScope` / `AttachPid` / `DestroyScope` | cgroup v2 scopes inside the subtree it owns |

Requested resource limits are **clamped down to policy ceilings, not rejected** — a session that asks
for too much runs smaller rather than failing to start. An omitted limit still receives the ceiling.

**Denials are opaque.** Every refusal renders exactly `"request denied"`, so a caller cannot learn from
an error whether a user, path or scope exists. An allowlisted account missing from the host is refused
identically to one the policy never listed.

**Tells the daemon it is supervised.** Every managed service the supervisor starts is given the path of
its privileged socket in **`TDDY_SUPERVISOR_SOCKET`**, set after the service's `env:` overlay so
`supervisor.yaml` cannot set or override it; sessions it spawns never receive it. A daemon started this
way spawns and clones through the supervisor; one started any other way (Tddy Desktop, `--user`, by hand)
forks its own spawn worker. `daemon.yaml` has no `supervisor:` key — see
[systemd install](../daemon/systemd-install.md).

### The user mapping (`users.yaml`)

`users.yaml` is the one place a GitHub login is mapped to the OS user its sessions run as:

```yaml
- github_user: "octocat"
  os_user: "alice"
- github_user: "hubot"
  os_user: "bob"
```

On a supervised host the root supervisor reads it, from the path in `supervisor.yaml`'s top-level
`users: {file, sockets_dir}` block (`./install` writes `file: /etc/tddy/users.yaml`; `sockets_dir`
defaults to `/run/tddy-users`). It is identity and authorization, not anything specific to a coding
agent, so it is the supervisor's to own. The supervisor uses only the `os_user` column; the daemon reads
the GitHub logins.

- **Trust.** The file is checked exactly like `supervisor.yaml`: owned by root, writable by nobody
  else, in directories nobody else can write. A file another user could write is refused — by the
  supervisor, and by a supervised daemon reading it.
- **Shape.** A login mapped to two OS users, a mapping to `root`, and an OS user that is not one plain
  account name (it must ride in a path segment and a `LISTEN_FDNAMES` entry) are refused.
- **Cap.** At most **15** distinct OS users: each costs one handed-over descriptor, and descriptor 3 is
  the daemon's own listener out of 16. A sixteenth fails loading, naming `users.yaml` and the limit.
- **Required.** A missing `users.yaml` is an error naming it, never an empty mapping. `./install`
  renders an empty one (`[]`, `0644`).

**Session users come from the mapping.** `SpawnSession`, `SpawnRelayed`, `SpawnSandbox` and
`CloneRepository` run only as an OS user `users.yaml` names; any other, or `root`, is the opaque denial.

### One socket per user, handed to the daemon

A session running as `alice` must reach the daemon over a socket only `alice` can open, and the
unprivileged daemon cannot make one for her. So for every OS user `users.yaml` maps, the supervisor
creates **one** daemon socket as root at `<sockets_dir>/<os user>/daemon.sock` — directory `0700` and
socket `0600`, both owned by that user, every directory above it writable by root alone — and hands the
listeners to the service that declares **`serves_users: true`**, after its own `socket:` (fd 3). They are
announced in `LISTEN_FDNAMES` as `user.<os user>` (`connection:user.alice:user.bob`), and the service is
told the mapping's path in **`TDDY_USERS_FILE`**, set after its `env:` overlay so `supervisor.yaml`
cannot forge it. No path is handed separately: the daemon reads each socket's path from its listener.
`serves_users` needs the service's own `socket:` and the top-level `users:` block.

The daemon serves everything a session of that user needs on that one socket: its agent's hook reports
(activity status, permission answers), `tddy-tools notify`, the `tddy-tools --mcp` tool server (the
daemon's whole RPC roster), and the host-session services (`github_token`, `spawn_conversation`). A
supervised daemon derives and binds no socket path of its own; a session for an OS user it was handed
no socket for is **refused at start**, naming `users.yaml` — never started with hooks whose reports
would be lost silently.

A daemon that is not supervised (`./web-dev`, `--user`, Tddy Desktop) reads `users.yaml` beside its own
config and binds each user's socket itself, at `<data dir>/run/<os user>/daemon.sock`.

### A saved edit restarts the daemon

The supervisor watches `users.yaml` (on its directory, so an editor's write-then-rename is seen). Each
change is re-read with the same trust and shape checks as at start-up:

- **Valid:** the per-user listeners are rebuilt — a user both mappings name keeps the very descriptor
  they had, a user no longer mapped loses theirs, a new user gets one — and then every `serves_users`
  service is restarted with the rebuilt handover. A client blocked in `connect` on an unchanged user's
  socket queues in the kernel across the restart and is served afterwards.
- **Invalid:** the edit is refused and logged, and the running daemon keeps the mapping in force. A
  typo never takes every user's socket away.

Changes are applied one at a time, in the order they were saved. A restart stops the service with
`SIGTERM`, then `SIGKILL`, waiting at most 10 s after each; a service that survives both is reported as
not restarted rather than left silently on the old mapping. **A restart drops every live relayed agent
and the daemon's in-memory state** (open vaults, room connections, inference subscriptions); sessions
survive on disk and resume. The watch refuses to start if the path of `users.yaml` cannot be resolved.

### Old keys are load errors

`spawn_policy.allowed_session_users`, `services[].host_sockets` and `services[].agent_tool_sockets` are
refused when `supervisor.yaml` loads, with an error naming `users.yaml` and the key to delete — as is a
`users:` key in `daemon.yaml`. `./install` migrates an existing install once (see
[systemd install](../daemon/systemd-install.md#behavior-notes)).

## `--user` installs no supervisor

`./install --systemd --user` installs a per-user `tddy-daemon` unit and **no supervisor**. Rootless, a
supervisor could neither `setuid` to another account, nor own a delegated cgroup subtree, nor bind a
root-owned socket — it would broker nothing. A single-user install also has no privilege separation to
provide, because the daemon already runs as the only user involved. That deployment is documented by
[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example).

## What still runs as the daemon user

On a supervised host these session types have **not** moved behind the supervisor and therefore run as
`tddy` rather than as the requesting user:

- **Sandbox sessions.** `tddy-sandbox-cgroups` still builds those jails in-process from the daemon.
  `SpawnSandbox` exists and works, but the daemon's sandbox path does not call it yet.
- **claude-cli, cursor-cli and PTY sessions.** `pty_runtime.rs` drops privilege by shelling out to
  `setpriv --reuid`, which an unprivileged daemon cannot do.

Both are tracked in [docs/dev/TODO.md](../../dev/TODO.md). Session and repo-clone spawning *do* route
through the supervisor, and fail closed when it is unreachable — there is deliberately no fallback,
because quietly spawning as the daemon user instead is the regression this feature removes.

**Every call the daemon makes is bounded.** A unary call that has not answered within 60 s fails
`Unavailable`, naming the method and the deadline (`the supervisor sent nothing for SpawnRelayed within
60s`). A supervised clone waits at most 300 s for its first event, and then 300 s between events; a
stream that goes quiet that long ends with the same `Unavailable` as its last item. git reports
progress continuously while it transfers, so silence that long is a stopped supervisor or git, not a
slow clone. Neither deadline is configurable.

## Acceptance criteria

- [x] A host installed with `./install --systemd` runs one root tddy unit with `tddy-daemon` as its unprivileged child.
- [x] Killing the daemon has the supervisor restart it; killing it repeatedly stops at the retry ceiling rather than spinning.
- [x] A request from a process owning no declared service is rejected, and the rejection reveals nothing about what exists.
- [x] A request naming an OS user, tool path, mount root or env key outside the root-owned allowlists is rejected.
- [x] Limits above a policy ceiling are clamped down, neither honoured nor rejected.
- [x] A session spawned by the supervisor is a child of the supervisor and leads its own process group.
- [x] The daemon's client socket is created by the supervisor as root and handed over, so the daemon never binds in `/run`.
- [x] `SpawnSandbox` builds a real jail — user/mount/net namespaces, private root, bind mounts — with privilege surrendered before any namespace exists.
- [x] The supervisor prepares its delegated cgroup subtree at startup and refuses to run if it cannot.
- [x] A session's cgroup scope is removed when the session ends.
- [x] A managed service is told the supervisor socket; a service's `env:` cannot override it; a session spawn is not told it.
- [x] A daemon started by the supervisor spawns and clones through it with no `supervisor:` block; one started otherwise forks its own worker; neither passes the variable on to children it spawns itself.
- [x] A supervised clone streams git's progress, then succeeds; a failed one's error carries git's own message.
- [x] A supervised clone asking for an env key outside `allowed_env_keys` is refused, as `SpawnSession` is.
- [x] Stopping a supervised clone (`StopSession(pid)`) ends git and leaves no destination directory.
- [x] A supervisor that accepts a call and never answers fails it `Unavailable` naming the method within the call deadline — every unary call, not only `SpawnRelayed`.
- [x] A supervised clone with no first event, or one that goes quiet after starting, fails `Unavailable` within its progress deadline.
- [x] A `serves_users` service is handed one socket per OS user `users.yaml` maps, at `<sockets_dir>/<user>/daemon.sock`, named `user.<user>`, plus the mapping's path in `TDDY_USERS_FILE`; one socket serves both the host-session services and the agent-tool roster.
- [x] The daemon learns the `users.yaml` path and every socket from the supervisor (each socket's path from its listener) and derives none itself.
- [x] A session can run as a mapped OS user; an unmapped one, or `root`, is refused.
- [x] Fifteen mapped OS users fit; a sixteenth fails loading, naming `users.yaml` and the limit.
- [x] A saved edit to `users.yaml` rebuilds the sockets — an unchanged user keeps their descriptor — and restarts the daemon; an invalid edit is refused and the running mapping kept.
- [x] A `users.yaml` that a non-root user could write is refused, by the supervisor and by a supervised daemon.
- [x] `allowed_session_users`, `host_sockets` and `agent_tool_sockets` in `supervisor.yaml` fail loading with an error naming `users.yaml`.
- [x] Under the supervisor, a session for a user it handed the daemon no socket for is refused at start, naming `users.yaml`.
- [ ] **Not yet proven on a real host:** a session for OS user `alice` running as `alice` while the daemon runs as `tddy`; enforced cgroup limits; `stop_session` against a live supervisor; a second account refused a `connect` to another user's daemon socket. These need root plus a second account, and are the subject of a planned VM-backed acceptance test — see [docs/dev/TODO.md](../../dev/TODO.md).

## Related

- Implementation: [packages/tddy-supervisor/docs/architecture.md](../../../packages/tddy-supervisor/docs/architecture.md)
- Install: [docs/ft/daemon/systemd-install.md](../daemon/systemd-install.md)
- [changelog/](changelog/)
