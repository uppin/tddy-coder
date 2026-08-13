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

services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    args: ["-c", "/etc/tddy/daemon.yaml"]
    user: tddy
    socket:                        # the supervisor binds this and hands it over as fd 3
      path: /run/tddy-daemon.sock
      group: tddy-clients
      mode: "0660"
    restart: { max_retries: 5, initial_backoff_ms: 500, max_backoff_ms: 30000, stability_threshold_ms: 10000 }

spawn_policy:                      # absent = no privilege granted at all
  allowed_session_users: [alice, bob]
  allowed_tool_paths: [/usr/local/bin/tddy-coder, /usr/bin/git]
  allowed_mount_roots: [/srv/tddy/repos]
  allowed_env_keys: [RUST_LOG]

cgroup:                            # absent = nothing is clamped
  memory_max_ceiling: 2147483648
  pids_max_ceiling: 512
```

Nothing is granted by default. An operator who writes no `spawn_policy` has a supervisor that supervises
the daemon and refuses every spawn request — which is the correct reading of an absent policy.

### The prerequisites that bite

- **`allowed_tool_paths` must include git's absolute path** or repo cloning is denied, because cloning
  now goes through the supervisor too. The value is `which git` on the daemon's `PATH`; matching is
  verbatim, with no symlink resolution.
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
| `CreateScope` / `AttachPid` / `DestroyScope` | cgroup v2 scopes inside the subtree it owns |

Requested resource limits are **clamped down to policy ceilings, not rejected** — a session that asks
for too much runs smaller rather than failing to start. An omitted limit still receives the ceiling.

**Denials are opaque.** Every refusal renders exactly `"request denied"`, so a caller cannot learn from
an error whether a user, path or scope exists. An allowlisted account missing from the host is refused
identically to one the policy never listed.

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
- [ ] **Not yet proven on a real host:** a session for OS user `alice` running as `alice` while the daemon runs as `tddy`; enforced cgroup limits; `stop_session` against a live supervisor. These need root plus a second account, and are the subject of a planned VM-backed acceptance test — see [docs/dev/TODO.md](../../dev/TODO.md).

## Related

- Implementation: [packages/tddy-supervisor/docs/architecture.md](../../../packages/tddy-supervisor/docs/architecture.md)
- Install: [docs/ft/daemon/systemd-install.md](../daemon/systemd-install.md)
- [changelog.md](changelog.md)
