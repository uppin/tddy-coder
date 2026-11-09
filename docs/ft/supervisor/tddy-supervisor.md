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

spawn_policy:                      # absent = no privilege granted at all (session users: users.yaml)
  exec_roots:                      # where a binary the daemon asks to run may live, every spawn kind
    - /usr/local/bin                 # a directory: any file beneath it
    - /usr/bin/git                   # one trusted file: exactly that file
    - { path: /nix/store, trusted_prefix: true }   # only when git resolves under /nix/store
  allowed_mount_roots: [/srv/tddy/repos]

cgroup:                            # absent = nothing is clamped
  memory_max_ceiling: 2147483648
  pids_max_ceiling: 512
```

Nothing is granted by default. An operator who writes no `spawn_policy` has a supervisor that supervises
the daemon and refuses every spawn request — which is the correct reading of an absent policy.

The OS users a session may run as are not in `supervisor.yaml` at all: they are the OS users
**`users.yaml`** maps (see [The user mapping](#the-user-mapping-usersyaml)). `root` is refused whatever
the mapping says.

The supervisor knows **where** a binary lives and **who** it runs as, and puts every child in a
cgroup scope. It does not know what the binary is, which environment variables it needs, or how git
works: no coding agent, harness or git detail is named in its code, its proto or its templates (a
guard test, `the_supervisor_names_no_coding_agent`, scans them for the coding-agent spec names).

### Exec roots

`spawn_policy.exec_roots` gates the binary of **every** spawn kind — session, sandbox, streamed and
relayed alike. Each entry is an absolute path naming either a **directory** (any file beneath it may
be run) or **one trusted file** (exactly that file, e.g. `/usr/bin/git`). A requested binary is run
only when:

- the path is plain and absolute, and **a symlink is judged by its target**: the file it resolves to
  must equal a file entry or lie under a directory entry — a link under a root to a file outside every
  root is refused, and a link outside every root to a file a root names runs that file;
- that file, every directory above it and the entry itself are owned by root (or the supervisor's
  euid) and are not group- or world-writable. **A world-writable directory is refused even when it is
  sticky** — `/tmp` is never trusted, unlike the policy file's own parents.

An entry written as `{ path: /nix/store, trusted_prefix: true }` is a **trusted prefix**: the operator
vouches for that directory itself, so its own owner and mode are not checked (`/nix/store` is
`drwxrwxr-t root:nixbld` by design), while every directory and file below it still is. Only the
operator opts in, per entry.

On Linux the descriptor that was checked is what is exec'd (`/proc/self/fd/<n>`, with `argv[0]` the
checked path), so a file swapped in after the check is not what runs. **macOS checks the path only**:
it has no exec through a descriptor, so a swap between check and exec is not closed there.

A refusal is the opaque denial to the caller; the reason is logged under the `tddy_supervisor::policy`
target. Granting a binary means installing it under a root, or adding its resolved file as an entry.

### One environment rule

There is no env key list. The environment the daemon sends passes **whole** to every spawn kind,
except:

- **loader variables are refused** — `LD_*`, `MALLOC_*`, `GCONV_PATH`, `NLSPATH`, `LOCPATH`,
  `HOSTALIASES`, `RESOLV_HOST_CONF`: `LD_PRELOAD` on a binary under a root is code execution as the
  session user, which would make `exec_roots` meaningless;
- `HOME`, `USER` and `LOGNAME` are **forced** from the target account;
- `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` are **stripped**.

Sessions, sandboxes and streamed spawns start from a minimal base (a fixed `PATH`, plus `LANG`/`TZ`
from the supervisor); a relayed spawn starts from exactly the env the daemon computed. A clone's SSH
settings and a session's commit identity therefore need no supervisor configuration.

### Relayed spawns

`SpawnRelayed` starts a binary as a mapped session user with its stdin/stdout relayed over a unix
socket the **daemon** listens on — the one request that gives a child stdio, because the ACP driver
talks to its agent over stdio and an unprivileged daemon cannot become another user itself. Relaying
is a stdio mode, not a trust class: it passes exactly the gates `SpawnSession` does (peer
authorization, the session user, `exec_roots`, the env rule). The supervisor connects to the daemon's
socket only if the path is absolute, is a socket (not a symlink to one) owned by the calling uid, and
the listener's `SO_PEERCRED` uid is that uid; any failure is the opaque denial and nothing is forked.
The child's exit status and a stop travel over `SessionStatus` / `StopSession`; a daemon that closes
the connection stops the child. The request may name a cgroup scope to join.

### Streamed spawns and clones

`SpawnStreamed` runs a binary as a session user and streams its stderr: `Started{pid}`, one
`Line{line}` per line (split on `\r` as well as `\n`), then `Exited{code, signal, stderr_tail}`. It
passes the same gates as `SpawnSession`, may name a cgroup scope, and is stopped by `StopSession(pid)`
or by the caller hanging up.

A supervised clone is **one** streamed spawn of the clone helper **`tddy-git-clone`**, which lives
in the bin dir beside the daemon, run as the session user:
`tddy-git-clone --git <git> -- <url> <destination>`. As that user it runs `git clone --progress`
into a `<destination>.partial-<random>` sibling, renames it into place on success, removes it on
failure, refuses a destination that already exists, and exits with git's code. The daemon passes the
absolute git found on the spawn-child `PATH` (none is a "failed precondition" refusal, never a bare
`git`), finds the helper beside itself (missing is a refusal), and parses git's progress from the
streamed lines. The supervisor never runs git and never touches the clone's files itself.

### The prerequisites that bite

- **Every binary the daemon runs must resolve under an `exec_roots` entry.** `./install` renders the
  bin dir (everything it installed, `tddy-git-clone` included) plus the one file git resolves to on
  the install host — never the whole directory git lives in — and `/nix/store` as a trusted prefix
  only when git resolves into it; no git on `PATH` fails the render. A coding-agent harness outside
  the bin dir (`opencode`, the `claude-agent-acp` binary `TDDY_CLAUDE_AGENT_ACP_CLI` names) is an
  operator entry: add its resolved file. Arguments are not checked, so naming `bunx` admits any
  package `bunx` can fetch, run as the session user. The daemon turns the supervisor's opaque denial
  of a relayed agent into an error naming `exec_roots`, the session user and the loader-variable rule.
- **The old keys fail loading.** `allowed_tool_paths` and `allowed_relay_tool_paths` name
  `exec_roots` as their replacement, and `allowed_env_keys` says it was removed. `./install` migrates
  an existing `supervisor.yaml` once: the listed tool and relay files become `exec_roots` file
  entries, and `allowed_env_keys` is dropped. A host upgraded without `./install` loses its
  supervisor until the file is migrated.
- **A daemon and a supervisor from different builds cannot clone**: the clone is a `SpawnStreamed`
  of `tddy-git-clone`, which an older supervisor does not serve.
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
| `SpawnSession` | a binary under `exec_roots` as an allowed OS user, optionally in a cgroup scope |
| `SpawnSandbox` | the same, jailed: user/mount/net namespaces, bind mounts, private root |
| `SpawnRelayed` | the same, with stdin/stdout relayed over the daemon's unix socket |
| `SpawnStreamed` | the same, its stderr **streamed**: `Started{pid}`, one `Line` per stderr line, `Exited{code, signal, stderr_tail}`; a refusal fails the call. `StopSession(pid)` stops it, as does the caller hanging up |
| `SessionStatus` / `StopSession` | the supervisor reaps its own children, so it is the only process that can answer |
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
- [x] A request naming an OS user or mount root outside the root-owned policy, a binary outside every `exec_roots` entry, or a loader variable is rejected.
- [x] Limits above a policy ceiling are clamped down, neither honoured nor rejected.
- [x] A session spawned by the supervisor is a child of the supervisor and leads its own process group.
- [x] The daemon's client socket is created by the supervisor as root and handed over, so the daemon never binds in `/run`.
- [x] `SpawnSandbox` builds a real jail — user/mount/net namespaces, private root, bind mounts — with privilege surrendered before any namespace exists.
- [x] The supervisor prepares its delegated cgroup subtree at startup and refuses to run if it cannot.
- [x] A session's cgroup scope is removed when the session ends.
- [x] A managed service is told the supervisor socket; a service's `env:` cannot override it; a session spawn is not told it.
- [x] A daemon started by the supervisor spawns and clones through it with no `supervisor:` block; one started otherwise forks its own worker; neither passes the variable on to children it spawns itself.
- [x] A supervised clone streams git's progress, then succeeds; a failed one's error carries git's own message.
- [x] A supervised clone asking for a loader variable is refused, as `SpawnSession` is.
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
- [x] A binary under an exec root directory, or named by an exec root file, root-owned and not group- or world-writable all the way up, is run. One outside every root, or with a writable file or parent (a sticky world-writable one included), is refused, and the log names the reason.
- [x] A symlink is judged by its target: a link under a root to a file outside every root is refused, and a link outside every root to a file a root names is run.
- [x] On Linux, swapping the file after the check does not change what is exec'd. On macOS the check is path-only, and the tests say so.
- [x] Session, sandbox, streamed and relayed spawns all pass the daemon's env whole, refuse a loader variable, force `HOME`/`USER`/`LOGNAME`, and strip `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET`.
- [x] An SSH clone needs no supervisor env configuration.
- [x] A clone through the supervisor is one streamed spawn of `tddy-git-clone` as the session user. It reports git's progress, parsed by the daemon. The clone ends up at its destination, renamed by the helper running as that user. A failed clone leaves no partial directory, and an existing destination is refused and left alone.
- [x] An exec root the operator marks `trusted_prefix` admits a binary below it although its own directory is group-writable and sticky; unmarked it does not, and a writable directory below the prefix is still refused.
- [x] `tddy-git-clone` is built by `./release` and shipped beside the daemon by `./install`, `./publish.sh` and the dev runtime list.
- [x] Relayed and streamed spawns can run in a cgroup scope: `SpawnRelayed` and `SpawnStreamed` take a `scope`, and a child that names one joins it. The daemon names none yet for either — see [docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md](../../dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md).
- [x] `allowed_tool_paths`, `allowed_relay_tool_paths` or `allowed_env_keys` in `supervisor.yaml` fails loading, naming the replacement. `./install` migrates them once.
- [x] No coding-agent name appears in `tddy-supervisor`'s non-test `src/`, `proto/`, or the supervisor templates (a guard scan).
- [x] A fresh install renders `exec_roots` naming the bin dir and the resolved git file.
- [x] The daemon's relay refusal names `exec_roots` and the env rule, not the removed keys.
- [ ] **Not yet proven on a real host:** a session for OS user `alice` running as `alice` while the daemon runs as `tddy`; enforced cgroup limits; `stop_session` against a live supervisor; a second account refused a `connect` to another user's daemon socket. These need root plus a second account, and are the subject of a planned VM-backed acceptance test — see [docs/dev/TODO.md](../../dev/TODO.md).

## Related

- Implementation: [packages/tddy-supervisor/docs/architecture.md](../../../packages/tddy-supervisor/docs/architecture.md)
- Install: [docs/ft/daemon/systemd-install.md](../daemon/systemd-install.md)
- [changelog/](changelog/)
