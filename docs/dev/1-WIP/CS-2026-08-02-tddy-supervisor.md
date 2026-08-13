# Changeset: tddy-supervisor

**Created:** 2026-08-02
**Status:** In Progress — Milestones 1-4, 6 and 7 complete; Milestone 5's jail execution outstanding
(`SpawnSandbox` is gated and fails closed), and the daemon's own call sites are not yet rewired.
`tddy-supervisor` 174/174 green; `tddy-daemon` 424/424 lib; `tddy-e2e` 77/77. Four validation passes
found and closed three escalation paths and one silent-wedge bug — see Validation results below.
**PRD:** docs/ft/supervisor/1-WIP/PRD-2026-08-02-tddy-supervisor.md

## Affected Packages

- [x] `tddy-supervisor` — **new**: root-run mini-init + policy-gated privileged broker (lib + bin), owning `proto/supervisor.proto` and its tddy-rpc-flavored codegen
- [x] `tddy-daemon` — supervisor client; session and clone spawning route through it when configured; config gains a `supervisor` block. **Sandbox, claude-cli, cursor-cli and PTY sessions do not** — see Known incomplete.
- [x] ~~`tddy-sandbox` — `SandboxPlan` gains the shape the supervisor needs~~ — **not needed.** The
  supervisor carries its own `SandboxJail`/`JailMount` in `spawn_broker.rs`, deliberately: `SandboxPlan`
  and its sub-specs derive neither `Serialize` nor `Deserialize`, so they cannot cross the RPC boundary,
  and half of what they carry (macOS `PolicySpec`, secrets materialization, copies/symlinks) is not the
  supervisor's business. `tddy-sandbox` is untouched.
- [x] ~~`tddy-sandbox-cgroups` — jail/scope mechanics become callable for a target uid~~ — **not needed,
  and reusing it would have been worse.** Its `enter_rootless_jail` builds the uid map from `geteuid()`,
  which is correct only for a process that was never privileged; from the supervisor that would map the
  child to real host root. The supervisor implements the same sequence from `plan.target.uid` instead.
  The duplication is deliberate and documented; `tddy-sandbox-cgroups` is untouched, so the
  no-supervisor deployment keeps working exactly as before.
- [x] `tddy-e2e` — install contract tests for the supervisor unit/socket/config
- [x] repo root — `Cargo.toml` members, `BUILD.yaml`, `install`, `release`, `supervisor.yaml.production`, `dev.supervisor.yaml`

## State A (Current)

**One unit, two bad modes.** `./install --systemd` writes `tddy-daemon.service` +
`tddy-daemon.socket`. `INSTALL_DAEMON_USER` selects between root (multi-user isolation works,
but the whole network-facing daemon is uid 0) and `tddy` (safe, but `same_user` in
`packages/tddy-daemon/src/spawner.rs:535,637,757` is true so the `pre_exec` privilege-drop is
skipped and every session runs as the one service account).

**Privilege lives inside the daemon.**
- `spawner.rs` — `clone_as_user` / `run_capture_as_user` / `spawn_as_user`, each doing
  `getpwnam_r` → `setgid` → `initgroups` → `setuid` in `pre_exec`.
- `spawn_worker.rs` — `fork_spawn_worker()` raw-forks *before tokio starts* (fork from a
  multi-threaded process can deadlock), then talks newline-delimited JSON over anonymous pipes
  (`WorkerRequest::{Spawn,Clone}` / `WorkerResponse::{SpawnOk,CloneOk,Error}`) to a child that
  holds no more privilege than its parent.
- `pty_runtime.rs` — a *second*, unrelated privilege-drop mechanism: `wrap_argv_for_privilege_drop`
  shells out to `setpriv --reuid --regid --init-groups`.
- `sandbox_session.rs:673,2779,3223,3667` — `build_sandbox_plan` → `spawn_sandbox_runner` →
  `tddy_sandbox_cgroups::spawn_plan`, with `cgroup: self.config.sandbox_cgroup_config()`.

**Sandbox privilege is bought with two host grants, not root.**
`tddy-sandbox-cgroups::detect_and_prepare_base` (OnceLock, once per process) derives a cgroup v2
base from `/proc/self/cgroup` + `/proc/self/mountinfo`, then `relocate_self_into_leaf` moves
**the calling process's own TGID** into a `supervisor` leaf to satisfy the no-internal-processes
rule. `enter_rootless_jail` runs in `pre_exec`: `unshare(CLONE_NEWUSER)`, `uid_map = "0 <euid> 1"`,
`setgroups=deny`, `gid_map`, `unshare(CLONE_NEWNS|CLONE_NEWNET)`, `mount(/, MS_REC|MS_PRIVATE)`,
`bring_loopback_up()`. Requires systemd `Delegate=yes` and — on hosts with
`apparmor_restrict_unprivileged_userns=1` — the `packages/tddy-daemon/apparmor/tddy-daemon`
profile whose only grant is `userns,`.

**Known dead end.** `packages/tddy-sandbox/docs/architecture.md` records that an unprivileged
`tddy-sandbox-app` cannot place its own child into a limited scope (cgroup v2 delegation
containment: the common ancestor of its shell scope and any delegated subtree is the root
cgroup), which is why it routes through the daemon instead.

**No supervisor process exists.** The word appears only as `CgroupConfig::supervisor_leaf` (a
directory name), `run_oauth_tunnel_supervisor` (an async task), and doc prose.

## State B (Target)

```
systemd ── tddy-supervisor.service (User=root, Delegate=yes)
             │
             ├─ tddy-supervisor                       [root]
             │    ├─ mini-init: start / reap / restart-with-backoff / signal-forward
             │    ├─ owns the delegated cgroup v2 subtree
             │    └─ serves SupervisorService on /run/tddy-supervisor.sock (root:tddy 0660)
             │         authorized by SO_PEERCRED against declared-service uids
             │
             ├─ tddy-daemon                           [tddy]  ← declared managed service
             │    └─ SupervisorClient ──────► the socket above
             │
             ├─ session process                       [alice] ← SpawnSession, setuid by supervisor
             └─ sandbox runner                        [bob]   ← SpawnSandbox, jailed by supervisor
```

Root privilege is confined to one small binary with a root-owned declarative policy. The daemon
keeps its axum server, LiveKit, GitHub, Telegram, LSP, BSP and VM code — all as `tddy`.

**The privilege-drop ordering is the load-bearing detail.** For `SpawnSandbox`, the supervisor's
`pre_exec` runs `setgid` → `initgroups` → `setuid(target)` **before** `unshare(CLONE_NEWUSER)`,
so the namespace is owned by the unprivileged target uid and the uid map is byte-identical to
today's rootless map. Root is used only to *choose* the uid and to write the cgroup scope, and
is surrendered before any namespace exists.

## Delta

### New

**`packages/tddy-supervisor/`** (lib `tddy_supervisor` + bin `tddy-supervisor`)

| Module | Contents |
|---|---|
| `config.rs` | `SupervisorConfig` (`deny_unknown_fields`), `ManagedService` (incl. `ServiceSocket`), `RestartPolicy`, `SpawnPolicy` (`allowed_session_users`, `allowed_tool_paths`, `allowed_mount_roots`, `allowed_env_keys`), `CgroupPolicy` (ceilings + delegation fields), `SocketConfig`. `load` refuses `root` by name, refuses a config file or ancestor that is not root-owned or is non-sticky group/other-writable, and rejects loader env keys. |
| `restart.rs` | Pure: `BackoffState`, `next_delay`, `record_exit`, `record_stability`, `RestartDecision::{Restart(Duration), GiveUp}` |
| `authz.rs` | Pure: `PeerIdentity { uid, gid, pid }`, `Authorizer::authorize(&PeerIdentity, &SupervisorRequest) -> Result<(), Denied>`; `Denied` carries no existence information |
| `policy.rs` | Pure: `resolve_session_user`, `resolve_tool_path` / `resolve_mount_source` (absolute, no `..`, allowlisted, no canonicalization), `resolve_env` (denies an unlisted key rather than dropping it), `clamp_limits`, `scope_dir`, `CpuMax` |
| `services.rs` | `ServiceSupervisor`, `ServiceState::{Starting,Running,Backoff,GaveUp,Stopped}`, `ServiceStatus { name, pid, state, restarts }` |
| `reaper.rs` | `SIGCHLD` handling, `waitpid` loop, exit→service attribution |
| `signals.rs` | `forward_shutdown(grace) -> ShutdownOutcome`, SIGTERM→grace→SIGKILL |
| `cgroup_broker.rs` | `CgroupBroker::new(base)`, `create_scope` (which is also what applies clamped limits — there is deliberately no `apply_limits`), `attach_pid`, `destroy_scope`, `scope_procs_path` — every write relative to an injected base, so tests point it at a tempdir |
| `spawn_broker.rs` | `SpawnPlan`/`SandboxJail`/`PreExecStep` + the pure `pre_exec_plan` that decides the ordering; `ForkBroker` (the one thread allowed to fork); `PreExecSteps`/`CompiledStep`, which walk that plan and nothing else; `ChildEnvironment`; `TargetUser` with supplementary groups resolved pre-fork |
| `socket.rs` | UDS listener; reuses the `resolve_socket_source(my_pid, LISTEN_PID, LISTEN_FDS, fallback)` pattern from `tddy-daemon/src/local_socket_server.rs` |
| `server.rs` | `SupervisorServiceImpl` — wires authz → policy → broker |
| `main.rs` | thin: parse `-c`, load config, start services, serve |

**Other new files**
- `packages/tddy-supervisor/proto/supervisor.proto` — `ListServices`, `StartService`, `StopService`, `SpawnSession`, `SpawnSandbox`, `SessionStatus`, `StopSession`, `CreateScope`, `AttachPid`, `DestroyScope`. No `ApplyLimits`: `CreateScope` already writes the clamped limits, and a second writer for the same three files is a second place to forget the clamp
- `packages/tddy-supervisor/build.rs` — tddy-rpc-flavored codegen for the proto above, so the uid-0 binary does not link `tddy-service`'s dependency tree
- `packages/tddy-daemon/src/supervisor_client.rs` — `SpawnBackendChoice`, the pure `spawn_backend_choice`, and `connect_supervisor` (hard-errors, naming the socket). **No callers yet** — see Known incomplete 0b
- `supervisor.yaml.production`, `dev.supervisor.yaml`
- `docs/dev/tddy-supervisor.service.example`
- `packages/tddy-supervisor/BUILD.yaml`

### Modified

- Root `Cargo.toml` — add `packages/tddy-supervisor` to `members`
- Root `BUILD.yaml` — add `tddy-supervisor:bin` to the `all:build` group
- `release` — build `tddy-supervisor`
- `install` — install the supervisor binary; write `tddy-supervisor.service` (root, `Delegate=yes`) and `tddy-supervisor.socket`; install `supervisor.yaml` from `supervisor.yaml.production`; stop writing `tddy-daemon.service` and disable an existing one; new `INSTALL_SUPERVISOR_*` overrides
- `packages/tddy-daemon/src/config.rs` — `supervisor: Option<SupervisorClientConfig>`
- `packages/tddy-daemon/src/spawn_worker.rs` — when a supervisor is configured, skip
  `fork_spawn_worker()` entirely and route `Spawn`/`Clone` to the supervisor
- `packages/tddy-daemon/src/sandbox_session.rs` — `spawn_sandbox_runner` routes to the supervisor when configured
- `packages/tddy-daemon/src/pty_runtime.rs` — the `setpriv` wrapper becomes the no-supervisor path only
- `packages/tddy-sandbox-cgroups/src/lib.rs` — `enter_rootless_jail` and scope mechanics accept an explicit target uid/gid and base instead of implying "self"
- `packages/tddy-e2e/src/install_contract.rs` + `tests/install_script.rs` — supervisor unit/socket/config contracts.
  ⚠️ **Two existing tests contradict the target state and must be retargeted in the green phase**:
  `install_writes_systemd_unit_with_expected_execstart` (`tests/install_script.rs:314`) and
  `install_preserves_systemd_unit_unless_overwrite_env` (`:353`) both read
  `${SYSTEMD_DIR}/tddy-daemon.service` and will fail once install writes
  `tddy-supervisor.service` instead. They should assert the same properties (ExecStart shape,
  preserve-unless-`INSTALL_OVERWRITE_SYSTEMD_UNIT`) against the supervisor unit. The install
  script's `UNIT_PATH` (`install:45`) and the `tddy-daemon.socket` wiring (`install:58,249-250,
  320-324`) change with them — the socket unit becomes the supervisor's.
- `docs/ft/daemon/systemd-install.md` — points at the supervisor unit
- `CLAUDE.md` / `AGENTS.md` — scripts and package tables

### Removed

- `tddy-daemon.service` as an independently installed unit (the supervisor owns the daemon's lifecycle). `docs/dev/tddy-daemon.service.example` stays, relabelled as the no-supervisor fallback.
- The daemon's need for `Delegate=yes` and `AppArmorProfile=tddy-daemon`. The AppArmor profile stays in the tree for the no-supervisor deployment.

## Milestones

### Milestone 0: Planning
- [x] Create/update PRD documentation
- [x] Create changeset

### Milestone 1: Config and policy (pure, no syscalls)
- [x] `SupervisorConfig` + `deny_unknown_fields` + load-time validation
- [x] `policy.rs` — user/tool-path/mount-root resolution, limit clamping
- [x] `authz.rs` — peer-credential authorization
- [x] `restart.rs` — backoff state machine

### Milestone 2: Mini-init
- [x] `ServiceSupervisor` start in declaration order with privilege drop
- [x] `reaper.rs` — SIGCHLD reap and exit attribution
- [x] Restart with backoff; give up at the ceiling; reset after the stability threshold
- [x] `signals.rs` — SIGTERM → grace → SIGKILL → exit

### Milestone 3: Privileged RPC surface
- [x] `supervisor.proto` and codegen through `tddy-supervisor/build.rs`
- [x] `socket.rs` — UDS bind/activation, ownership and mode
- [x] `server.rs` — authz → policy → broker wiring for every method

### Milestone 4: Cgroup broker
- [x] Scope create/limits/attach/destroy against an injected base
- [ ] Supervisor owns the delegated subtree; scope cleanup on session end

### Milestone 5: Session and sandbox spawn brokers
- [x] `spawn_session` — setuid/setgid/initgroups against the allowlist
- [x] `spawn_sandbox` — privilege drop **then** namespace/mount setup, plus scope placement

### Milestone 6: Daemon integration
- [x] `SupervisorClient` and `supervisor` config block
- [x] Route spawn/clone through the supervisor; fail closed with no fallback (sandbox/PTY paths excepted — see Known incomplete)
- [x] No-supervisor path unchanged

### Milestone 7: Install and docs
- [x] `install` writes the supervisor unit/socket/config and disables the old daemon unit
- [x] `release` builds the supervisor
- [ ] Docs: `packages/tddy-supervisor/docs/architecture.md`, `systemd-install.md`, service example

## Testing Strategy

### The core constraint

None of the privileged behavior can execute as root in CI. The strategy is the one
`tddy-sandbox-cgroups` already established: **split the decision from the syscall.** Every
policy, authorization, backoff, path-resolution and limit-clamping decision is a pure function
over plain data and is unit-tested exhaustively. Syscall execution sits behind a narrow seam —
an injected cgroup base (a tempdir in tests), an injected `getpwnam` resolver, an injected
spawn function — so the orchestration around it is tested without privilege.

Acceptance tests run the **real `tddy-supervisor` binary** as the invoking unprivileged user,
with a config that declares that same user. The privilege drop is then a no-op at the syscall
level while every other real path — fork, exec, reap, backoff, socket bind, peer-credential
auth, RPC round trip, shutdown — executes for real. No test-only branch exists in production
code; only the injected base and target user differ.

### Contracts the acceptance tests pin

- `CgroupPolicy::base_override`, when set, is used **verbatim** — no derivation from
  `/proc/self/cgroup`, no `/proc/self/mountinfo` probe. That is what makes the scope tests run on
  any host, and it is a documented production option, not a test hook.
- A denial is opaque. `SupervisorError::Denied` carries no fields and always renders as
  `"request denied"`, whether the caller was an unauthorized peer, named a disallowed user, or
  named a disallowed path — otherwise the error becomes an existence oracle.
- Over-ceiling limits are **clamped, not rejected**. A session that asks for too much gets less.
- Sessions are children of the supervisor, not of the daemon. Pinned by having the spawned tool
  record its own `$PPID`.
- Declaration *order* is asserted through `ListServices`, not through observing which child wrote
  to a file first — the latter is a scheduler race, not a supervisor contract.

### Acceptance Tests

All 20 are written and failing. Harness: `packages/tddy-supervisor/tests/support/mod.rs`.

**Mini-init** — `packages/tddy-supervisor/tests/supervisor_lifecycle.rs`
- [x] `starts_every_declared_service_and_reports_it_running`
- [x] `reports_every_declared_service_in_declaration_order`
- [x] `restarts_a_managed_service_that_exits_and_reports_a_new_pid`
- [x] `stops_restarting_a_service_once_the_retry_ceiling_is_reached`
- [x] `terminates_every_managed_service_when_the_supervisor_is_asked_to_shut_down`

**Privileged surface** — `packages/tddy-supervisor/tests/supervisor_authorization.rs`
- [x] `rejects_a_session_spawn_from_a_peer_that_owns_no_declared_service`
- [x] `rejects_a_session_spawn_for_an_os_user_outside_the_allowlist`
- [x] `rejects_a_session_spawn_for_a_tool_path_outside_the_allowlist`
- [x] `spawns_an_allowlisted_tool_for_an_allowlisted_user_as_a_child_of_the_supervisor`

**Cgroup broker** — `packages/tddy-supervisor/tests/supervisor_cgroups.rs`
- [x] `creates_a_scope_with_the_requested_limits_when_they_are_under_the_ceiling`
- [x] `clamps_requested_limits_down_to_the_policy_ceiling`
- [x] `places_a_spawned_session_into_the_scope_it_asked_for`
- [x] `removes_the_scope_directory_when_the_scope_is_destroyed`

**Daemon integration** — `packages/tddy-daemon/tests/supervisor_routing.rs`
- [x] `delegates_spawning_to_the_supervisor_when_the_config_declares_a_socket`
- [x] `keeps_the_forked_spawn_worker_when_no_supervisor_is_declared`
- [x] `fails_to_reach_a_declared_supervisor_whose_socket_is_absent`

**Install contract** — `packages/tddy-e2e/tests/install_supervisor.rs`
- [x] `installs_a_root_supervisor_unit_that_delegates_a_cgroup_subtree`
- [x] `installs_a_supervisor_config_declaring_the_daemon_as_an_unprivileged_service`
- [x] `no_longer_installs_a_standalone_daemon_unit`
- [x] `installs_the_supervisor_binary_alongside_the_daemon`

### Unit Tests (77, inline `#[cfg(test)] mod tests`)

Builders live in `packages/tddy-supervisor/src/test_util.rs` (`a_restart_policy`,
`a_managed_service`, `a_spawn_policy`, `a_cgroup_policy`, `requesting_*`, `unlimited`).

| Module | Count | Covers |
|---|---|---|
| `src/config.rs` | 17 | minimal parse; defaults (mode `0660`, 20s grace, `memory cpu pids`, leaf `supervisor`, deny-all spawn policy); octal mode parsing; service list order + restart policy; `deny_unknown_fields` at top level *and* inside a service; rejects `user: root`, `root` in `allowed_session_users`, duplicate service names, and relative socket / `exec_start` / tool / mount-root paths |
| `src/policy.rs` | 30 | session-user resolution incl. `root` refused even if listed; tool paths (absolute, no `..`, listed verbatim, no filesystem canonicalization); mount roots incl. prefix-not-component and traversal escape; limit clamping incl. *omitted request still gets the ceiling* and per-limit independence; `cpu.max` parse/render, single-field and zero-period rejection, period-mismatch rejection; scope dir naming and separator / `..` / empty-name rejection |
| `src/restart.rs` | 9 | initial delay, doubling, cap, give-up at the budget, zero-retry budget, restarts-performed vs exits-seen, delay reset and budget restore after a stable run, threshold boundary |
| `src/services.rs` | 9 | `Starting` → `Running` across the startup grace period, pid dropped on exit, `Backoff`, `GaveUp` with the performed count, stop-on-request neither restarting nor spending budget, restart-after-give-up restoring the budget, suppression not persisting |
| `src/authz.rs` | 6 | authorizes declared-service uids; denies unknown uids, denies all when nothing is declared, denies root; denial reveals nothing |
| `src/socket.rs` | 6 | adopts fd 3 only when `LISTEN_PID` names us; self-binds on foreign `LISTEN_PID`, absent vars, zero count, unparseable count, unparseable pid |

Two of the 77 pass already (`renders_a_cpu_max_back_in_the_kernel_format`,
`reports_a_declared_but_unstarted_service_as_starting_without_a_pid`) — their production code is
the `Display` impl and the `status()` snapshot, both complete by construction. The other 75 fail on
`todo!()`.

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| Config parsing, validation, `deny_unknown_fields` | Unit | Pure serde over strings; exhaustive cheaply |
| Restart backoff sequence, ceiling, stability reset | Unit | Pure state machine; timing must not be observed through sleeps |
| Peer-credential authorization | Unit | Pure decision over `(PeerIdentity, Request)`; every deny path needs a case |
| Tool path / user / mount-root resolution, limit clamping | Unit | Pure; this is the security boundary and deserves the densest coverage |
| Socket source resolution (`LISTEN_FDS`) | Unit | Pure env→enum mapping, already precedented in `local_socket_server.rs` |
| Start / reap / restart / shutdown orchestration | Acceptance (real binary) | Fork, exec and `waitpid` cannot be meaningfully faked; the bugs live in the orchestration |
| RPC round trip + authz over a real UDS | Acceptance (real binary) | `SO_PEERCRED` only exists on a real socket |
| Cgroup scope writes | Acceptance (injected tempdir base) | Deterministic on every host; identical code path to `/sys/fs/cgroup` |
| Actual `setuid` to a *different* user | Manual / operator smoke | Requires root and a second account; documented in the architecture doc |
| Actual namespace + mount jail under a root parent | Manual / operator smoke | Requires root; the ordering contract is pinned by a unit test on the `pre_exec` plan |
| Install script | Acceptance (`INSTALL_NO_SYSTEMCTL=1` temp tree) | Existing precedent in `packages/tddy-e2e/tests/install_script.rs` |

## Validation results (2026-08-02)

Four analyses ran against the finished implementation: change-risk, test-quality, production-readiness
and clean-code. They found **three escalation paths and one silent-wedge bug that the 126 passing tests
did not catch**. All are fixed; the suite went 126 → **174** (141 unit + 33 acceptance).

### Escalation paths found and closed

- **`LD_PRELOAD` defeated the tool-path allowlist.** A caller's `env` was passed through verbatim
  (only the three `LISTEN_*` names were stripped). The child execs *after* `setuid` and the target is
  not setuid, so `AT_SECURE` is unset and the loader honours `LD_PRELOAD`/`LD_LIBRARY_PATH`/`LD_AUDIT`
  — meaning the daemon could pass both gates with an allowlisted user and an allowlisted tool and
  still run its own code as every session user, without touching `allowed_tool_paths` at all.
  Fixed with an allowlist (`SpawnPolicy::allowed_env_keys`, empty by default, loader keys refused at
  load *and* per request), plus a minimal base environment for sessions and `HOME`/`USER`/`LOGNAME`
  derived from the resolved account after the request's own vars.
- **`AttachPid` had no second gate.** `CreateScope("x", memory_max: 1)` — clamping only lowers, so a
  tiny limit is honoured — then `AttachPid("x", 1)` moved **PID 1** into it and systemd was
  OOM-killed. The same primitive worked against `sshd` or the supervisor itself. It was the only
  handler missing gate 2, and it contradicted the proto's own stated invariant. Now gated on the
  session table, and more strictly than `stop_session`: a *retained-exited* pid is refused too,
  because the kernel is free to reissue it.
- **The privileged listener leaked into every child across `exec`.** Measured, not inferred: before
  the fix a managed service that declared no socket held `fd 3 -> socket:[…]` with the **same inode**
  as the supervisor's privileged listener, so a session running as another user could `accept()` the
  daemon's privileged connections. systemd passes activation fds with `FD_CLOEXEC` *clear* —
  `sd_listen_fds(1)` sets it, and this hand-rolled adoption did not. Socket-declaring services were
  accidentally masked by their own `dup2`, and the guard test could never fail because the harness
  always self-binds.
- **The "root-owned policy file" premise was unenforced.** `load` never stat'd anything. Now the same
  handle that is parsed is `fstat`ed, and canonicalized ancestors are walked; group/other-writable is
  refused unless the directory is sticky (the kernel's own rule — `/tmp` at `1777` is safe, `/etc/tddy`
  at `0777` is not).

### Silent-wedge bug

**`initgroups` in `pre_exec` allocated and entered NSS**, violating the module's own documented
invariant. This repo had already been bitten by it — `packages/tddy-daemon/src/spawner.rs:909` carries
a comment about spawns hanging "often stuck in initgroups". It was worse here: `ForkBroker` has one
thread, so a single wedged child would block every later session spawn *and* every managed-service
restart permanently, while the supervisor stayed up and kept answering `ListServices`. The
supplementary group list is now resolved **before** the fork (`getgrouplist`, ERANGE retry) and the
child calls bare `setgroups`.

Also fixed: a stale deferred `SIGKILL` in `stop_session` could kill a *different* session that reused
the pid (generation counter added); mutex poisoning silently disabled the supervisor forever (now
degrades with a logged recovery, rather than a root process killing itself over bookkeeping and taking
the daemon with it via `PR_SET_PDEATHSIG`); the accept loop died permanently on any error and had no
connection cap (transient errors now backoff, concurrency bounded at 64).

### ⚠️ Prerequisite for Milestone 6

`allowed_env_keys` is a **deny, not a filter**: a request naming an unlisted key is refused outright
rather than having the key dropped, because silently discarding an environment variable a caller
believed it set is worse than saying no. With the default empty, whoever wires the daemon's spawn path
must send **no** `env`, or list every key it sends. Documented at the field and in both templates.

### Measured, and worth knowing

`ForkBroker::reserve_handover_slot` is **inert in both deployments** — fd 3 is always already
occupied, by tokio's epoll fd when self-binding and by systemd's listener under activation. Its
invariant (nothing std opens lands on fd 3, so a failed `exec` cannot be reported over the handover
slot) therefore holds by accident rather than by the reservation. It is not currently a bug and the
`dup2`-onto-3 happens only post-fork in the child, so it cannot displace the parent's listener.

### Findings deliberately not acted on

- **Sessions still receive a displaced exit status when a pid is reissued** (`supervisor.rs:114`,
  `TODO`). A caller can only name a session by pid, so this needs a session handle on the wire —
  a change across `proto/`, `protocol.rs` and `client.rs`, and a wire change is not worth making
  before Milestone 6 needs it. A `WARN` fires when it happens.
- **`INSTALL_NO_SYSTEMCTL=1` still does not gate the file writes.** Gating them would have hollowed
  out two existing failure tests that only assert `!success` — they would have kept passing for a
  different reason. The header's promise was narrowed to state exactly what is skipped instead.
- **The socket-path drift check warns rather than fails.** Install cannot repair a preserved operator
  config without overwriting it, and the mismatch is often benign since the daemon adopts the passed
  fd regardless of `local.socket_path`.
- **Managed services keep an inherited environment** (sessions get a minimal one). Root authored that
  declaration, and an operator's `Environment=` reaching the daemon is existing behavior.

## Red phase 2 — failing tests for the three gaps (2026-08-02)

**21 new failing tests.** `cargo test -p tddy-supervisor` → 94 passed, 21 failed; clippy and fmt clean.

### `pre_exec` ordering is now observable (11 tests, `src/spawn_broker.rs`)

The privilege-drop ordering the PRD calls load-bearing ran as straight-line statements inside a
private `run()`, so no test could assert it. It is now data: `PreExecStep`, `SandboxJail`,
`JailMount`, and a pure `pre_exec_plan(&SpawnPlan) -> Vec<PreExecStep>`. Tests pin
`DropPrivilege` before every namespace step, `JoinCgroupScope` before `DropPrivilege`,
`MakeRootMountPrivate` before any `BindMount`, `BringLoopbackUp` after the netns unshare,
`ChangeDirectory` after the drop, bind-mount declaration order, no namespace steps for an unjailed
spawn, and no drop when the supervisor already runs as the target.

### A privileged listening socket for a managed service (5 + 3 tests)

`ManagedService::socket: Option<ServiceSocket>`. `tests/supervisor_socket_handoff.rs` asserts the
socket is bound before the service starts, has the configured mode, arrives as fd 3 with
`LISTEN_FDS=1`, carries a `LISTEN_PID` naming the *service*, is rebound on restart, and is not
handed to a service that declared none (that last one passes today and is a guard against
regression). Config tests cover relative paths, non-octal modes, and two services claiming one path.

**Implementation note the tests force:** `LISTEN_PID` must be the child's pid, which is only known
after `fork`, so it cannot be set via `Command::env`. It needs a `pre_exec`-side `putenv` over a
buffer allocated *before* the fork — the `pre_exec` closure must stay allocation-free.

### Sessions are not their own process group (2 tests, `tests/supervisor_session_lifetime.rs`)

⚠️ **A live hazard, not future work.** `spawn_broker::spawn_now` never calls `setsid`/`setpgid`, so a
supervisor-spawned session inherits the supervisor's process group — confirmed by a failing test, not
inferred. Meanwhile `terminate_sandbox_process` (`sandbox_session.rs`) signals sessions with
`kill(-pid, SIGTERM)` then `kill(-pid, SIGKILL)`, and `CliSessionManager::kill_all` reaches for pids
the same way. A group signal aimed at a session therefore either fails with `ESRCH` or, if a pid ever
collides with the supervisor's group id, takes down the supervisor and every service under it.

### Milestone 6 contract, now decided and red-phased (11 more tests)

**Decision: an AF_UNIX path, not fd-passing.** The session's control channel is a `--grpc-uds` path
the daemon passes in `args` and dials once the child is up — the same reason
`SpawnRequest::host_session_socket` is a path: "fds can't cross the fork boundary". No `SCM_RIGHTS`
is introduced, so the supervisor gains no new primitive.

**Exit status is a poll RPC**, mapping 1:1 onto the daemon's existing 50ms `try_exit_diagnostic()`
loop rather than adding a stream. `SessionStatus { pid, state, exit_code }` plus `stop_session`.
The supervisor must *retain* an exited session's status until asked: a caller's poll always arrives
after the reap, so a status discarded at reap time is one no caller could ever observe.

New API: `SpawnSandboxRequest`, `SandboxMount`, `SessionState`, `SessionStatus`;
`SupervisorClient::{spawn_sandbox, session_status, stop_session}`.

`tests/supervisor_sandbox.rs` (6) asserts only the policy gate — mount source outside every allowed
root, traversal escape, prefix-not-component, no-roots-granted, plus user and tool denials.
**Deliberately no "successfully jails" acceptance test**: that needs unprivileged user namespaces,
which a host with `apparmor_restrict_unprivileged_userns=1` denies to any unprofiled binary, so the
test would pass or fail depending on the machine. The jail's *ordering* is pinned by the
`pre_exec_plan` unit tests, which run anywhere; jailing end-to-end stays operator smoke.

`tests/supervisor_session_lifetime.rs` (5 more) covers running/exited reporting, the exit code
surviving the reap, `stop_session`, and — importantly — that a status query for a pid the supervisor
never spawned is **denied**, so the privileged surface cannot be used to probe arbitrary host
processes for liveness.

## Superseded: the design fork that blocked Milestone 6

Wiring the daemon to the supervisor for *sandbox* sessions is blocked on one choice that is the
developer's, not something to guess:

**The daemon needs things only a parent has.** `SandboxHandle` owns a `std::process::Child`, and the
daemon uses it for three things `waitpid` only permits on your own children:
`wait_for_sandbox_ready` polls `try_exit_diagnostic()` every 50ms so a child that dies before its
ready-marker fails fast; `SandboxSessionState::stop` does `child_mut().kill()` + `.wait()`;
`sandbox_action.rs` reads exit codes via `child_mut().wait()`. With the supervisor as parent, all
three need a supervisor-mediated replacement (an exit-status RPC, or an event stream) — the shape of
which is a protocol decision.

**And the piped stdio cannot cross a process boundary.** `bridge_sandbox_stdio` requires the child's
piped stdin+stdout (`take_stdio()`) to build its `StdioEndpoint`; `dial_and_bridge` is the only
session transport actually in use. There is **no `SCM_RIGHTS` anywhere in this repo**, so the two
options are: (a) add fd-passing over the supervisor socket, or (b) follow the existing precedent —
`SpawnRequest::host_session_socket` and `--grpc-uds` both hand the child an AF_UNIX *path* rather
than fds, explicitly because "fds can't cross the fork boundary".

(b) matches the grain of the codebase and needs no new primitive; (a) is fewer moving parts at
runtime but new machinery. Until one is chosen, a `SpawnSandbox` reply cannot be specified, so no
honest test can be written for it.

## ⚠️ Known incomplete — needs a decision before this ships

### ✅ 0. RESOLVED — `SpawnSandbox` builds a real jail

The five namespace/mount steps now execute: `unshare(CLONE_NEWUSER)` with a `0 <target> 1` uid/gid map
and `setgroups=deny`, `unshare(CLONE_NEWNS)`, `mount(/, MS_REC|MS_PRIVATE)`, the bind mounts, and — only
when `isolate_network` is set — `unshare(CLONE_NEWNET)` plus loopback bring-up.

**Verified on a real kernel**, contrary to an earlier note in this changeset claiming it could not be:
this host has `apparmor_restrict_unprivileged_userns=1` *but* `apparmor_restrict_unprivileged_unconfined=0`,
and an unconfined process is exempt. Driving the production code through a jailed spawn produced
`uid=0(root)` inside the namespace, a working bind mount, a read-only mount refusing writes, a netns
containing only `lo` (UP), and no mount propagation to the host. No test depends on that, since a host
running under a confining profile would fail it — the plan's *ordering* is what the unit tests pin.

Three things the implementation had to get right that the unprivileged reference version never faced:

- **The uid map is built from `plan.target.uid`, never from `geteuid()`.** Pre-fork `geteuid()` is the
  supervisor's 0, which would map the jail to *real host root*; post-`unshare` it is the overflow uid
  65534, which the kernel refuses outright.
- **`setuid` leaves the process non-dumpable, so it gets `EACCES` opening its own `/proc/self/uid_map`.**
  Every jailed spawn would have failed without a `PR_SET_DUMPABLE` re-arm.
- **A pre-fork descriptor cannot be used for the bind.** An fd opened before `unshare(CLONE_NEWNS)`
  belongs to the old mount namespace and `mount(2)` rejects it (`EINVAL`, measured). The authoritative
  `openat2(RESOLVE_NO_SYMLINKS)` therefore happens in the child immediately before the bind, so check
  and use are the same object with no window; `compile` keeps a pre-fork resolution purely to carry a
  readable message, because only an errno escapes `pre_exec`.

### ✅ `resolve_mount_source` symlink hole — RESOLVED

Closed with `openat2(RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH)` against the matching allowed root. A
kernel without `openat2` **refuses** the mount rather than falling back to a path check — the fallback
would be the exact TOCTOU the change removes. `ENOENT` is deliberately not a policy denial (the bind
refuses a missing source anyway, and a denial reading "no such directory" sends operators hunting a
policy bug); verified that `openat2` returns `ELOOP` even for a *dangling* symlink, so nothing is
smuggled through that branch.

### ✅ Milestone 6 — session and clone spawning now route through the supervisor

`spawner::plan_session_child` was extracted as the single place that decides *what* to run — passwd
resolution, session id, LiveKit room and identity, the child config, the grpc port, and the whole argv.
`spawn_as_user` is now only the fork; the supervised path sends the same plan's program/args to
`SpawnSession` and assembles `SpawnResult` from the plan plus the returned pid. **The argv exists in one
place.** Both backends watch startup on the same schedule — the supervised one by polling
`session_status`, since `waitpid` is not available to a non-parent.

On a supervised host `main.rs` does **not** fork a spawn worker at all. Every `ForkedWorker` arm is the
pre-existing code unchanged, and no `spawn_as_user`/`clone_as_user` call survives outside one.

**Operator prerequisites this creates**, both because an unlisted value is *refused* rather than
adjusted: `allowed_tool_paths` must contain git's absolute path (`which git` on the daemon's PATH) or
cloning is denied; and `allowed_env_keys` must contain `PATH` if any session user's
`~/.tddy/config.yaml` sets `spawn_path_extra`. An ordinary spawn sends no env, so the shipped empty
policy works.

**Design choice worth review:** the client is connected per operation from `config.supervisor.socket_path`
rather than held on `ConnectionServiceImpl`. That avoided changing a constructor signature used by ~40
test files, and it survives a supervisor restart under a long-lived daemon with no reconnect logic. Cost
is one AF_UNIX connect per spawn or clone.

### ⚠️ Still spawned by the daemon on a supervised host — two gaps, neither previously recorded

- **`SignalSession` will fail with `EPERM`.** `connection_service.rs` calls `libc::kill(pid, sig)`
  directly, which an unprivileged daemon cannot do to a session running as another user. `SIGTERM` and
  `SIGKILL` map onto the supervisor's `stop_session`; **`SIGINT` has no equivalent**, so closing this
  needs a protocol decision (add a `SignalSession` rpc, or accept TERM/KILL only) and its own red phase.
  Deliberately not guessed at.
- **claude-cli, cursor-cli and PTY sessions still spawn from the daemon.** `pty_runtime.rs` drops
  privilege by shelling out to `setpriv --reuid`, which an unprivileged daemon cannot do — so on a
  supervised host those session types run as the daemon user. Same class as the sandbox path, but it
  was not called out anywhere until now. The fix is routing them through `SpawnSession`, which needs the
  pty master fd to cross the socket via `SCM_RIGHTS` — the one place the "paths, not fds" decision does
  not stretch.

### ⚠️ Two defects the jail work exposed, both fixed

- **`PR_SET_PDEATHSIG` was silently cleared by the privilege drop.** `commit_creds()` zeroes
  `pdeath_signal` on any change of effective ids, so step 1 of the ordering contract was undone by
  step 3 for **every** child that drops privilege — and again by `unshare(CLONE_NEWUSER)`. A killed
  supervisor *did* leave its daemon and sessions running. This was invisible because the acceptance
  suite declares the invoking user as the service user, so no drop is planned: the property held in
  tests and failed in production. Now re-armed after *each* credential-changing step rather than after
  whichever is currently last, so a future step cannot silently invalidate it.
- **`isolate_network: false` produced an empty netns with loopback down.** `pre_exec_plan` emitted a
  combined `CLONE_NEWNS|CLONE_NEWNET` unconditionally and gated only the loopback. Split into
  `EnterMountNamespace` (always, for a jailed plan) and `EnterNetworkNamespace` (only when isolating).

### ~~0. `SpawnSandbox` — the surface exists and is gated; the jail itself does not execute~~ (superseded)

**Updated.** The proto rpc, the client method, the policy gate and 6 acceptance tests all now exist.
What remains missing is only the *execution* of the jail's five namespace/mount steps.

`pre_exec_plan` plans them — that is what the ordering unit tests pin — but
`spawn_broker::CompiledStep::compile` **refuses** them before the fork with `ErrorKind::Unsupported`,
so a policy-passing `SpawnSandbox` fails with:

```
privileged operation failed: spawn sandbox: sandbox namespace and mount setup is not implemented;
the supervisor will not spawn a session it cannot isolate
```

No process is created and no pid is recorded. This is deliberate and fail-closed on two counts: the
refusal happens pre-fork so an operator gets a sentence rather than an errno, and it surfaces as
`OperationFailed` rather than `Denied` — dressing an unimplemented jail as a policy denial would hide
it. **Consequence unchanged:** `tddy-sandbox-cgroups::spawn_plan` is still what jails sandbox
sessions, from the unprivileged daemon, exactly as before. The sandbox path has not moved behind the
supervisor. Implementing the five steps needs its own red phase, and cannot be verified on a host
with `apparmor_restrict_unprivileged_userns=1`.

### ✅ `ApplyLimits` — resolved as "do not add"

The changeset previously listed this as an absent method. It should not exist. `CgroupBroker` has no
`apply_limits` at all: `create_scope` calls `clamp_limits` and writes `memory.max` / `cpu.max` /
`pids.max` itself, so `ApplyLimits` would not expose existing machinery — it would add a **second
writer for the same three files**, and a missing clamp in only one of them is a silent ceiling bypass
on the security-critical path. A scope is created per session immediately before spawning into it, and
nothing in the tree re-limits a live scope. Adding the rpc later is purely additive to the proto;
removing it once shipped would be a wire break. `CreateScope` is the whole feature.

### ~~0. `SpawnSandbox` was scoped but is NOT implemented~~ (superseded above)

The changeset scoped four privileged operations. Three are done (declared-service control, cgroup
scope lifecycle, `SpawnSession` as another user). **The fourth is not**: there is no
`SpawnSandbox` proto method, no client method, and no test. `spawn_broker.rs:299` carries a `TODO`
marking exactly where the namespace/mount setup must go — *after* the privilege drop, per the PRD's
binding ordering contract. `ApplyLimits` is likewise absent from the proto (limits are currently
only settable at `CreateScope`).

This was left unbuilt rather than guessed at: the red phase wrote no acceptance test for it, and
implementing the jail — the security-critical half of the feature — from an unpinned contract would
be worse than not having it. It needs its own red phase.

Consequence: `tddy-sandbox-cgroups::spawn_plan` is still what jails sandbox sessions, from the
unprivileged daemon, exactly as before. The sandbox path has not moved behind the supervisor.

### 0b. Milestone 6 integration is not wired

`spawn_backend_choice` and `connect_supervisor` exist and are tested, but have **no callers**.
`connection_service.rs`, `sandbox_session.rs` and `spawn_worker.rs` still use the forked spawn
worker. So on a host installed today the supervisor starts and supervises `tddy-daemon`, and its
privileged surface is live and reachable, but the daemon does not yet *use* it for session spawning.
Wiring it needs `SpawnedProcess` to carry the stdio/socket paths the daemon's session bridge
expects (it currently carries only a pid, and sessions get `Stdio::null()`).

### ✅ 1. RESOLVED — the daemon's local ConnectionService socket

Fixed as designed, via option (a): `ManagedService::socket` declares a listener, the supervisor binds
it as root before the service starts, and hands it to the child as fd 3 with `LISTEN_FDS=1` and a
`LISTEN_PID` naming the child. The daemon needed **no changes** — `resolve_socket_source` already
implements the receiving half. One `INSTALL_DAEMON_SOCKET_PATH` value is substituted into both
`supervisor.yaml`'s `socket:` block and `daemon.yaml`'s `local.socket_path`, so the two cannot drift;
pinned by `declares_the_daemons_local_socket_for_the_supervisor_to_create`.

The listener is created once and **held across restarts** rather than rebound: rebinding would unlink
and recreate the socket node, and a client connecting in that window gets `ECONNREFUSED` on a path
that is about to work. Holding it keeps the kernel's accept queue, so connections arriving while the
daemon is down simply wait — systemd's socket-activation bargain.

**Implementation note that contradicted the original plan.** The plan called for a `pre_exec`-side
`putenv` to set `LISTEN_PID`. That cannot work: std runs `pre_exec` closures *before* it installs
`Command`'s own `envp` (`library/std/src/sys/process/unix/unix.rs`), so any `putenv` there is
discarded whenever `Command::env` was called — and `putenv` would take glibc's `__environ_lock` and
may `realloc`, neither of which is safe after a fork. Instead `spawn_now` calls no `Command::env*`
method at all, and a `ChildEnvironment` built pre-fork owns the whole `envp` plus a pre-sized
`LISTEN_PID=` buffer whose digits are written in place. Allocation-free, lock-free, and the same
mechanism std uses — without the ordering problem.

Also hardened along the way: `ForkBroker` now reserves fd 3 at startup. `dup2` onto fd 3 silently
replaces whatever is there, and `Command::spawn` reports exec failures over a `SOCK_SEQPACKET` pair
opened just before the fork — had that landed on fd 3, a *failed* exec would have looked successful.

### ~~1. The daemon's local ConnectionService socket regresses on a supervised host~~ (superseded)

Marked `TODO(tddy-supervisor):` at `install:381-388` and in `systemd-install.md`.

Today `tddy-daemon.socket` creates `/run/tddy-daemon.sock` **as root** and hands the daemon the
listening fd, which is why an unprivileged daemon never binds in `/run`. With the daemon demoted
to a supervisor child it is no longer a systemd service, so `resolve_socket_source`
(`local_socket_server.rs`) falls through to `SelfBind` and the daemon tries to bind
`${XDG_RUNTIME_DIR:-/run}/tddy-daemon.sock` as `tddy` → `EACCES`. Keeping a `tddy-daemon.socket`
unit would be worse: it would try to activate a service that no longer exists.

**The clean fix is (a), and it needs no daemon changes at all:** give `ManagedService` a socket
declaration, and have the supervisor create the listener as root with the configured
owner/group/mode and pass it to the child via `LISTEN_PID`/`LISTEN_FDS` — exactly what systemd did.
The daemon already implements the receiving half (`SocketSource::Activated`, `SD_LISTEN_FDS_START`,
and it already clears `LISTEN_*` so grandchildren don't see the fd). The alternative (b) — move
`local.socket_path` into a service-user-owned directory and have the daemon chmod its own socket for
the client group — is weaker: it puts the socket somewhere clients don't look and makes the daemon
responsible for its own access control.

No test covers this yet. It was deliberately not built blind.

### 2. PRD Goal 3 is wrong about the AppArmor userns grant

The PRD claims the unprivileged-userns grant "disappears because the supervisor is root". That does
not follow from the binding mitigation in *Design Risks*: the supervisor `setuid(target)` **before**
`unshare(CLONE_NEWUSER)`, so at the moment the namespace is created the process is unprivileged, and
the AppArmor label in force is the one attached at exec of `tddy-supervisor`. On a host with
`kernel.apparmor_restrict_unprivileged_userns=1` the grant therefore has to exist for the
**supervisor** binary — a new `packages/tddy-supervisor/apparmor/tddy-supervisor` profile — not
disappear.

`install` still renders and loads the existing `tddy-daemon` profile, correctly: it is
*path-attached* to the daemon binary (`flags=(unconfined) { userns, }`), so it applies at every exec
regardless of any unit directive, and the daemon keeps its own non-brokered jail path during
migration. Dropping it would silently break sandbox startup on Ubuntu 24.04-class hosts for zero
security gain.

Goal 3 in the PRD should be corrected to "the grant moves to the supervisor binary", and a
supervisor profile added, with a test that pins which binary carries it.

## Technical Debt

- **`Supervisor::shutdown` still signals sessions by pid, not by process group**
  (`src/supervisor.rs`). Now that every child leads its own group, group-signalling there would also
  reach a session's own descendants — the same argument that justifies it in `stop_session`. Left
  alone because no test pins it and it would change managed-service shutdown behavior too.
- **⚠️ `resolve_mount_source` has no symlink containment — must be closed in Milestone 5's red phase.**
  `plain_absolute_path` + `starts_with` is correct component-wise prefix containment and correctly
  refuses `..`. The deliberate no-canonicalization argument is sound for `resolve_tool_path`, where
  the allowlist is exact equality against root-owned paths — but it does **not** transfer to mount
  roots, which are *prefixes* over trees that session users can write into (the production template's
  roots are repo directories).

  So `alice` can create `/srv/tddy/repos/alice/esc -> /`, and a `SpawnSandbox` naming that source
  passes the gate — component-wise it *is* under the allowed root — while `mount --bind` follows
  symlinks in the source and binds `/` into the jail. Not exploitable today only because the jail
  does not execute. It is recorded here because six acceptance tests in
  `tests/supervisor_sandbox.rs` present this function as *the* mount gate, so a reader would
  reasonably assume it is complete.

  The safe shape is to resolve the source once with `openat2(RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH)`
  against the allowed root and bind by `/proc/self/fd/N`, so check and use are the same object rather
  than the same path re-walked.
- **`chdir` is planned before the mount namespace and bind mounts.** A jailed session's cwd will
  therefore resolve against the pre-mount tree. Not a defect today (the jail does not run) but it is
  a deliberate decision Milestone 5 has to make rather than inherit.
- **A sandbox mount's `target` is not validated** — not checked absolute, not checked for traversal.
  These are paths inside the jail's own namespace rather than host paths, which is why no test pins
  them, but it wants a decision alongside the `working_dir` item below.
- **`apply_socket_ownership` exists twice** — in `server.rs` (for the privileged socket) and
  `supervisor.rs` (for a service socket). Both carry a `TODO`; they should fold into `socket.rs`.
- **`--user` mode is documented only in the `install` header.** Rebasing onto master's `--user`
  (rootless) install forced a decision: a `--user` install gets the legacy `tddy-daemon` user unit and
  **no supervisor**, because rootless it could neither `setuid` nor delegate cgroups nor own a
  root socket — it would broker nothing. `docs/ft/daemon/systemd-install.md` documents neither
  `--user` nor `--headless` (neither side of the rebase added them), so that decision currently lives
  only in the script. Worth its own changeset.
- **`./publish.sh` does not ship the supervisor.** Master's new packaging script builds a `.deb`
  installing binaries plus a systemd unit into `/lib/systemd/system`; it knows nothing about
  `tddy-supervisor` or `supervisor.yaml`, so a `.deb`-installed host gets the old daemon-only
  deployment. Not touched here.
- **Two codex-acp install tests now fail earlier than their names claim.**
  `install_fails_when_config_lists_codex_acp_without_native` and
  `install_fails_when_install_bundle_codex_acp_without_native` do not override `INSTALL_BIN_DIR`, so
  the new `chmod go-w` hardening aborts them at the real `/usr/local/bin` before the codex-acp check
  they are named for. They assert only a non-zero exit, so they still pass — for the wrong reason.
  Pre-existing sloppiness that this changeset's hardening made visible; the fix is to give them the
  four `INSTALL_*_DIR` overrides every other install test uses.
- **`working_dir` on a session spawn is not policy-checked.** It is `chdir`'d after the privilege
  drop, so it is traversed with the target user's authority rather than root's — which is the
  important part — but unlike `tool_path` it is not matched against any allowlist. Needs a decision
  on whether it should be, plus a test.
- **`cgroup_broker.rs:146`** — on real cgroupfs the supervisor must relocate itself into
  `supervisor_leaf` and write `controllers` into the base's `cgroup.subtree_control` (cgroup v2's
  no-internal-processes rule). Doing it unconditionally fails against a tempdir base, and branching
  on that would be a test-environment branch. For now the unit's `Delegate=yes` slice must supply a
  usable base. Needs a seam, not a branch.
- **The acceptance harness's `Drop` comment is wrong about orphans.** It claims `SIGKILL`ing the
  supervisor stops the managed `sleep`s; measured, it left ~39 orphans per run. The supervisor now
  sets `PR_SET_PDEATHSIG` on every child (which is also correct behavior on a real host — a killed
  supervisor must not leave sessions running), bringing it to 0. The harness comment should be
  corrected so nobody relies on the kill doing the cleanup.

- Two privilege-drop mechanisms coexist during migration: the supervisor's `setuid` and
  `pty_runtime.rs`'s `setpriv` shell-out. The latter should be retired once PTY spawning also
  routes through the supervisor — out of scope here.
- `spawn_worker.rs`'s fork-before-tokio machinery stays for the no-supervisor path. It becomes
  dead weight on supervised hosts and should be removed once the supervisor is the only
  supported deployment.
- `detect_and_prepare_base`'s process-global `OnceLock` cache means the supervisor resolves its
  base exactly once; a cgroup topology change requires a restart.
- Scope `rmdir` on session end was already an open follow-up from the
  `cgroups-sandbox-unprivileged` changeset; this changeset closes it for supervised hosts only.
