# tddy-supervisor architecture

`tddy-supervisor` is the only process on a supervised host that runs as uid 0. Its whole job is to be
a small, auditable privilege boundary in front of `tddy-daemon`: it starts the daemon as an
unprivileged child and performs, on request, the handful of operations that genuinely need root.

```
systemd ── tddy-supervisor.service (User=root, Delegate=yes)
             │
             ├─ tddy-supervisor                       [root]
             │    ├─ mini-init: start / reap / restart-with-backoff / shutdown
             │    ├─ owns the delegated cgroup v2 subtree
             │    └─ serves SupervisorService on /run/tddy-supervisor.sock (root:tddy-clients 0660)
             │         authorized by SO_PEERCRED against declared-service uids
             │
             ├─ tddy-daemon                           [tddy]   declared managed service
             │    └─ SupervisorClient ──────► the socket above
             │
             ├─ session process                       [alice]  SpawnSession
             └─ jailed sandbox                        [bob]    SpawnSandbox
```

Everything network-facing — the axum server, LiveKit, GitHub OAuth, Telegram, LSP, BSP, the VM
service — stays in the daemon, as `tddy`.

## Why the crate owns its own proto

`proto/supervisor.proto` lives here with a crate-local `build.rs`, not in `tddy-service`. Routing it
through the shared service crate linked **327** crates into the uid-0 binary — a SQL engine, a TUI
with syntax highlighting, an HTTP server *and* client, a TLS stack — none of it reachable from this
code. It links **48** now. The wire path is unchanged: `tddy-rpc`'s frame codec over AF_UNIX via
`tddy-stdio`'s duplex endpoint, never gRPC.

The same reasoning is why `nix` is not a dependency: every syscall goes through raw `libc`, which
also keeps the post-fork paths visibly allocation- and lock-free.

## The two gates

Every request passes both, in this order, and neither is skippable on any path:

1. **`authz::Authorizer`** — is this peer one of my services? Decided from `SO_PEERCRED`, before a
   single field of the request is interpreted. The allowed set is exactly the uids owning a declared
   service, so a supervisor managing nothing serves nobody. Root is **not** special-cased in.
2. **`policy`** — may that peer have *this*? Decided from the root-owned config, never from anything
   the request asserts about itself.

They answer different questions and are separate so a regression in one cannot hide behind the other.

**Denials are opaque.** Every authorization or policy refusal is `SupervisorError::Denied`, which
carries no fields and renders exactly `"request denied"`, in both directions across the wire. A caller
refused a spawn as `alice` must not be able to learn whether `alice` exists — so an allowlisted
account that is missing from the host is refused *identically* to one the policy never listed.

`SupervisorError::Invalid` is the deliberate exception: it carries a message, and is used only for
requests malformed on their face (a `cpu.max` that is not two integers), where precision leaks
nothing and saves an operator an hour.

## Ordering is the security contract, and it is data

`spawn_broker::pre_exec_plan` returns an ordered `Vec<PreExecStep>`; `PreExecSteps::prepare` compiles
that plan and `run()` walks it. **Ordering exists in exactly one place.** Expressed as statements
inside `pre_exec` it could not be asserted at all; expressed as a value, every edge is pinned by unit
tests that run on any host.

```
SetParentDeathSignal
LeadOwnProcessGroup
[JoinCgroupScope]                 ← needs root: the scope is root-owned
DropPrivilege                     ← setgroups → setgid → setuid; root surrendered here
SetParentDeathSignal              ← re-armed: see below
ChangeDirectory                   ← after the drop, so a caller's cwd is traversed as the target user
[EnterUserNamespace]
SetParentDeathSignal              ← re-armed again
[EnterMountNamespace]
[EnterNetworkNamespace]           ← only when isolate_network
[MakeRootMountPrivate]
[BindMount …]                     ← in declaration order
[BringLoopbackUp]                 ← only when isolate_network
```

Four things about this are easy to get wrong and were:

**The uid map comes from `plan.target.uid`, never `geteuid()`.** Pre-fork, `geteuid()` is the
supervisor's 0 — a user namespace created with that map gives the child *real* root against the host.
Post-`unshare`, it is the overflow uid 65534, which the kernel refuses outright. The target uid is
correct because the drop has already happened (or was skipped precisely because euid already equals
it).

**`PR_SET_PDEATHSIG` is re-armed after every credential change.** `commit_creds()` zeroes
`pdeath_signal` on any change of effective ids, so arming it once meant a killed supervisor left its
daemon and every session running. Re-arming after each such step, rather than reasoning about which
is last, means a future step cannot silently invalidate it.

**`setuid` leaves the process non-dumpable**, so it gets `EACCES` opening its own
`/proc/self/uid_map`. A `PR_SET_DUMPABLE` re-arm is mandatory — the unprivileged reference
implementation in `tddy-sandbox-cgroups` never needed it because it was never privileged.

**The supplementary group list is truncated to `getgrouplist`'s `ngroups`, never to its return
value.** glibc returns the number of groups it found; Darwin's libc returns `0` for "they fit". Both
write the number into `ngroups`. Reading the return value therefore hands the child an *empty* group
list on one of the two platforms — the same silent privilege downgrade the `bail` beside it exists to
refuse, arrived at by another route. (Found by making the crate compile off Linux, not by a test.)

**A bind mount's source is resolved in the child, not pre-fork.** A descriptor opened before
`unshare(CLONE_NEWNS)` belongs to the old mount namespace and `mount(2)` rejects it. The
authoritative `openat2(RESOLVE_NO_SYMLINKS)` therefore happens immediately before the bind, so check
and use are the same object with no TOCTOU window. `compile` keeps a pre-fork resolution purely to
carry a readable message, because only an errno escapes `pre_exec`.

### Every child leads its own process group

`setsid` in `pre_exec`. The daemon signals sessions with `kill(-pid, …)`; without this they would
inherit the supervisor's group, so a group signal aimed at a session would either fail with `ESRCH`
or reach the supervisor and everything under it.

### Forking from a multi-threaded process

`fork` copies only the calling thread, so a lock another thread held at fork time stays locked forever
in the child. Two things keep that safe: **every** fork happens on `ForkBroker`'s one dedicated
thread, never a runtime worker; and the `pre_exec` closure allocates nothing and takes no lock —
every string is a `CString` built pre-fork, and pids are formatted into stack buffers.

`ForkBroker` is a thread rather than `spawn_blocking` for a second reason: `PR_SET_PDEATHSIG` is keyed
to the *forking thread*, and tokio retires idle blocking threads after seconds, which would tell every
child its parent had died.

## The child's environment is an allowlist

`ChildEnvironment` builds the whole `envp` pre-fork. `spawn_now` calls **no** `Command::env*` method,
which matters: std runs `pre_exec` closures *before* it installs `Command`'s own `envp`, so a `putenv`
from inside the closure would be discarded — and would take glibc's `__environ_lock` and may
`realloc`, neither safe after a fork. Instead a pre-sized `LISTEN_PID=` buffer has its digits written
in place, which is how the child learns its own pid.

`SpawnPolicy::allowed_env_keys` is a **deny, not a filter**: a request naming an unlisted key is
refused outright rather than having the key dropped, because silently discarding a variable a caller
believed it set is worse than saying no. Loader keys (`LD_*`, `MALLOC_*`, `GCONV_PATH`, `NLSPATH`,
`LOCPATH`, `HOSTALIASES`, `RESOLV_HOST_CONF`) can never be listed and are rejected at config load:
`LD_PRELOAD` on an allowlisted tool is arbitrary code execution as the session user, which would make
`allowed_tool_paths` meaningless.

Sessions get a minimal base (fixed `PATH`, forwarded `LANG`/`TZ`); declared services keep an inherited
one, because root authored that declaration. `HOME`/`USER`/`LOGNAME` are always derived from the
resolved account, after the request's own vars, so a caller cannot override them.

## Handing a listening socket to a managed service

`ManagedService::socket` declares a listener the supervisor creates **as root, before the service
starts**, and hands over as fd 3 with `LISTEN_FDS=1` and a `LISTEN_PID` naming the child. This is
exactly what `tddy-daemon.socket` used to do, which is why the daemon needed no changes — its
`resolve_socket_source` already implements the receiving half.

The listener is created once and **held across restarts**. Rebinding would unlink and recreate the
socket node, and a client connecting in that window gets `ECONNREFUSED` on a path that is about to
work; holding it keeps the kernel's accept queue, so connections arriving while the service is down
simply wait.

A service that declared no socket gets no fd and no `LISTEN_*` at all — the activation variables are
stripped *after* the request's own env, so neither inheritance nor a request can forge one.

`ForkBroker` reserves fd 3 at startup, because `Command::spawn` reports exec failures over a
`SOCK_SEQPACKET` pair opened just before the fork: had that landed on fd 3, a *failed* exec would have
looked successful. (In both real deployments fd 3 is already occupied — by tokio's epoll fd when
self-binding, by systemd's listener under activation — so the reservation is currently inert but the
invariant is stated rather than assumed.)

## The cgroup subtree

`prepare_delegated_subtree` runs once in `run()`, before the listener and before any service: it
creates `<base>/<supervisor_leaf>`, moves the supervisor's own TGID into it, then delegates
`+memory +cpu +pids` into `<base>/cgroup.subtree_control`. That order is forced by cgroup v2's
no-internal-processes rule — controllers cannot be delegated out of a cgroup that still holds
processes.

**Both failures are fatal at startup**, diverging deliberately from `tddy-sandbox-cgroups`, which only
warns. Without `+memory` in `subtree_control` a scope's `memory.max` write has no controller behind it,
yet `CreateScope` still reports `AppliedLimits { memory_max: Some(..) }` — a ceiling claimed as applied
that the kernel does not enforce, invisible until an OOM that never comes.

After preparation the supervisor sits in `<base>/<leaf>`, so **every managed service and session is
born in the leaf**, not the base. That is what keeps the base process-free for the run.

`base_override`, when set, is used **verbatim** — no derivation from `/proc/self/cgroup`. It is a
documented production option (pinning an already-prepared subtree) and it is also what lets the
acceptance tests point the broker at a temp directory.

Requested limits are **clamped to policy ceilings, never rejected**: a session that asks for too much
runs smaller rather than failing to start. An *omitted* limit still receives the ceiling — leaving a
field out is not a way to escape it. A `cpu.max` whose period differs from the ceiling's is `Invalid`
rather than guessed at, because comparing quotas across periods would silently multiply the grant.

Scope reclamation is keyed to a **session generation**, never a bare pid. A pid is reissued while the
previous session's retained status is still readable, so a pid-keyed reclaim would delete the *new*
session's live scope. The association is derived from the spawn plan itself, so the scope a session
joins and the scope recorded as its own cannot disagree — and a bare `CreateScope` with no session
attached is consequently never swept; it stays the caller's to destroy.

`EBUSY` (descendants outliving the session leader) gets a bounded retry off the reaper, because every
other exit on the host queues behind `reap`, and those descendants are the supervisor's
*grand*children so their exits raise no `SIGCHLD` here at all.

## Session lifecycle

The supervisor reaps its own children, so it is the only process that can answer what became of a
session — a caller cannot `waitpid` a process it did not fork. `SessionStatus` therefore has to
**survive the reap**: a caller's poll always arrives after the child was reaped, so a status dropped
at reap time is one nobody could ever observe. Retention is bounded at 256 entries, evicting oldest,
because a map of dead pids that only grows is a slow leak in a process that must not need restarting.

A `session_status` query for a pid the supervisor never spawned is **denied**, not answered —
otherwise the privileged surface becomes a liveness oracle for arbitrary host processes. `attach_pid`
is gated the same way, and more strictly: a *retained-exited* pid is refused too, because the kernel is
free to have reissued it.

## Restart policy

`restart.rs` is pure, so the timing policy is testable without waiting for it. `record_exit` resets the
retry budget when uptime passed the stability threshold, *then* checks it — a service healthy for a
week must not be abandoned on its first crash. Give-up does not increment, so a service configured for
two retries reports two restarts when it gives up, not three.

`ServiceState::Starting` becomes `Running` after a 500ms grace, mirroring the daemon's own spawner,
which catches a child that dies instantly (a missing shared library, a bad config) rather than
reporting it healthy.

## Config is the whole policy surface

`SupervisorConfig` uses `deny_unknown_fields`: a typo'd security setting is a startup failure, not a
silently ignored line. Validation is part of parsing, because a config that would let the daemon
escalate is a startup failure and not a runtime surprise.

`load` also **stats what it parsed** — the same handle, plus canonicalized ancestors — and refuses a
config file or directory that is not root-owned, or that is group/world-writable without being sticky
(the kernel's own rule: `/tmp` at `1777` is safe, `/etc/tddy` at `0777` is not). Every doc comment in
the crate rests on "root-owned policy file"; this is what makes that true rather than assumed.

`root` is refused as a service or session user by name at load, and the *resolved* uid/gid is checked
too — a passwd entry aliased to uid 0 under another name would otherwise pass validation and then skip
the privilege drop entirely.

## Testing seams

Policy, authorization, backoff, path resolution, limit clamping and the `pre_exec` ordering are pure
functions over plain data, unit-tested exhaustively. Syscall execution sits behind narrow seams — an
injected cgroup base, an injected passwd resolver — following the pattern `tddy-sandbox-cgroups`
established.

Acceptance tests run the **real binary** as the invoking user, with a config declaring that same user.
The privilege drop is then a no-op at the syscall level while fork, exec, reap, backoff, socket bind,
`SO_PEERCRED` and the RPC round trip all execute for real. No test-only branch exists in production
code; only the injected base and target user differ.

**Off Linux the crate compiles and refuses.** The supervisor is a Linux program — systemd starts it,
it joins cgroup scopes, it builds a jail from namespaces and bind mounts — and none of that exists on
Darwin. What both platforms share is everything decided *before* the fork: step ordering, the
credentials refused, the environment built. So each `pre_exec` step that needs a Linux facility
returns `ErrorKind::Unsupported` ("the supervisor will not spawn a session it cannot confine") rather
than being skipped, and the six `policy` tests that assert what `openat2(2)` answers are
`#[cfg(target_os = "linux")]` — off Linux a source would be denied for want of a kernel rather than on
its merits, so an accepting test would fail and a denying one would pass without testing anything.
A macOS developer runs the pre-fork decisions; nobody gets a reduced jail.

Test fixtures wait on `await_ready`, which polls `SupervisorClient::connect` **and** `try_wait` for
the whole window. Waiting for the socket *inode* was the earlier mistake: dropping a `UnixListener`
does not unlink it, so a supervisor that died any time after binding left the file behind and every
later connect reported `ECONNREFUSED` with no mention of the exit that caused it.

**What that cannot prove**, and is therefore operator smoke rather than CI: a session actually running
as a *different* user, the `PR_SET_PDEATHSIG` re-arm surviving a real drop (both need root and a second
account), and real cgroupfs behaviour — `rmdir` of an emptied scope succeeding and a populated one
returning `EBUSY`, which a plain directory never does.

## Related

- Product docs: [docs/ft/supervisor/tddy-supervisor.md](../../../docs/ft/supervisor/tddy-supervisor.md)
- Install: [docs/ft/daemon/systemd-install.md](../../../docs/ft/daemon/systemd-install.md)
- The no-supervisor deployment: [docs/dev/tddy-daemon.service.example](../../../docs/dev/tddy-daemon.service.example)
- Known limitations and follow-ups: [docs/dev/TODO.md](../../../docs/dev/TODO.md)
