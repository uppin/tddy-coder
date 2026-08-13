# PRD: tddy-supervisor — a small privileged broker in front of tddy-daemon

**Created:** 2026-08-02
**Product Area:** supervisor
**Status:** WIP

## Summary

Introduce **`tddy-supervisor`**, a small root-run process that systemd starts instead of
`tddy-daemon`. The supervisor spawns and keeps alive the declared unprivileged services
(`tddy-daemon` first among them) and exposes a narrow, policy-gated RPC surface over a unix
socket for the four operations that genuinely need privilege: spawning declared services,
cgroup v2 scope lifecycle, spawning sessions as other OS users, and sandbox namespace/mount
setup.

## Background

### Today there are only two deployment modes, and both are wrong

`./install --systemd` writes a single `tddy-daemon.service`. `INSTALL_DAEMON_USER` picks
between:

**Root mode (`INSTALL_DAEMON_USER=root`).** The only mode where multi-user session isolation
works — `packages/tddy-daemon/src/spawner.rs` can `setgid`/`initgroups`/`setuid` into a
per-GitHub-user OS account (`clone_as_user`, `run_capture_as_user`, `spawn_as_user`). The
price is that *everything else* in the daemon runs as root too: the axum HTTP server and
Connect-RPC router (`server.rs`), the LiveKit participant and peer discovery, the GitHub OAuth
and token store, the Telegram bot, the LSP executor, the BSP catalog, the PTY runtime, the VM
manager — ~65 modules and a large third-party dependency tree, all reachable from the network,
all with uid 0. That is the surface this PRD exists to remove.

**Unprivileged mode (default, `User=tddy`).** Safe, but it buys safety by giving up the
feature: `same_user` in `spawner.rs` evaluates true, the `pre_exec` privilege-drop closure is
skipped entirely, and *every* session runs as the single `tddy` account with no isolation
between users. It also needs two host grants to make the sandbox work at all — systemd
`Delegate=yes` for a writable cgroup v2 subtree, and an AppArmor profile granting the daemon
binary unprivileged user namespaces on hosts with
`kernel.apparmor_restrict_unprivileged_userns=1`.

Neither mode is acceptable for a multi-user host: one is unsafe, the other is not the product.

### The shape of the fix already exists in the codebase

`packages/tddy-daemon/src/spawn_worker.rs` already forks a dedicated single-threaded child
*before tokio starts* and talks to it over newline-delimited JSON on anonymous pipes, purely
because `fork()` from a multi-threaded process can deadlock. That child is a supervisor in
everything but name and privilege — it just happens to be a *descendant* of the daemon, so it
can hold no privilege the daemon does not already have.

Inverting the relationship — supervisor as *parent*, daemon as unprivileged child — is what
turns that existing structure into a privilege boundary. It also removes the fork-before-tokio
constraint from the daemon entirely.

### It also fixes cgroup delegation containment

`packages/tddy-sandbox/docs/architecture.md` documents that an unprivileged process cannot
place its own child into a limited cgroup scope, because the common ancestor of its own scope
and any writable delegated subtree is the root cgroup. `tddy-sandbox-cgroups`'
`detect_and_prepare_base` works around this by relocating *the calling process itself* into a
`supervisor` leaf of a delegated base. A real supervisor that owns the delegated subtree and
creates scopes on request is the correct owner of that operation, and lets
`tddy-sandbox-app` stop routing through the daemon for the same reason.

## Goals

1. No network-facing code runs as root. The only root process is a small binary whose entire
   job is policy enforcement and process/cgroup/namespace mechanics.
2. Multi-user session isolation works *without* a root daemon.
3. The daemon stops needing `Delegate=yes` and the AppArmor userns grant — `Delegate=yes` moves to
   the supervisor unit, and the userns grant moves to the **supervisor binary**.

   > **Correction (found during implementation).** An earlier draft of this goal said the userns
   > grant "disappears because the supervisor is root". That is wrong, and it contradicts the
   > mitigation in [Design Risks](#namespace-setup-moves-into-the-privileged-process): the
   > supervisor drops to the target uid *before* `unshare(CLONE_NEWUSER)`, so the process is
   > unprivileged when the namespace is created and the AppArmor label in force is the one attached
   > at exec of `tddy-supervisor`. On a host with
   > `kernel.apparmor_restrict_unprivileged_userns=1` the grant must therefore exist for the
   > supervisor binary. A `packages/tddy-supervisor/apparmor/tddy-supervisor` profile is required,
   > with a test pinning which binary carries the grant.
4. One systemd unit to operate.

## Non-Goals

- Replacing systemd. The supervisor manages tddy processes only; systemd still owns the
  supervisor.
- A general-purpose init (no dependency graph, no socket activation *for children*, no
  timers). Services start in declaration order.
- Windows or macOS. The supervisor is Linux-only; macOS keeps the current
  `tddy-sandbox-darwin` path and an unprivileged daemon.

## Requirements

### Functional Requirements

**Process supervision (mini-init)**

- [ ] Supervisor reads a root-owned config declaring a list of managed services: name,
      binary path, argv, OS user/group, environment, working directory, restart policy.
- [ ] Supervisor starts every declared service at boot, in declaration order, dropping to the
      declared user via `setgid` → `initgroups` → `setuid` before `exec`.
- [ ] Supervisor reaps children and restarts a service that exits, with exponential backoff
      and a configurable retry ceiling; a service that stays up past a stability threshold
      resets its backoff.
- [ ] Supervisor forwards `SIGTERM`/`SIGINT` to all managed services, waits a configurable
      grace period, then `SIGKILL`s survivors, then exits.
- [ ] Supervisor exposes service state (name, pid, state, restart count) over its RPC surface.

**Privileged RPC surface**

- [ ] Supervisor serves a `SupervisorService` over an AF_UNIX socket owned `root:<socket
      group>` with mode `0660`, supporting systemd socket activation and self-bind.
- [ ] Every request is authorized by peer credentials (`SO_PEERCRED`). Only uids that own a
      declared service may call; all other peers are rejected before the request is parsed
      for meaning.
- [ ] **Declared-service control** — `StartService` / `StopService` / `ListServices` operate
      on names from the root-owned config. A caller can never name a binary.
- [ ] **Cgroup v2 lifecycle** — `CreateScope`, `ApplyLimits`, `AttachPid`, `DestroyScope`
      carve per-session scopes out of the subtree the supervisor owns. Requested limits are
      clamped to policy ceilings, never raised above them.
- [ ] **Session spawning as another user** — `SpawnSession` fork/execs a tool as a target OS
      user. Both the target user and the tool path must appear in root-owned allowlists;
      neither is taken on the caller's word.
- [ ] **Sandbox namespace/mount setup** — `SpawnSandbox` performs the user/mount/net namespace
      setup, bind mounts and loopback bring-up that `tddy-sandbox-cgroups::spawn_plan` does
      today, plus scope placement, and returns the child pid and its stdio/socket paths.

**Policy**

- [ ] `root` is never a valid target user for `SpawnSession` or `SpawnSandbox`; a config that
      declares it is rejected at load.
- [ ] Tool paths are matched against an allowlist of absolute paths; relative paths, symlink
      escapes and `..` traversal are rejected.
- [ ] Bind-mount sources requested for a sandbox must fall under a policy-declared set of
      permitted roots.
- [ ] Config uses `deny_unknown_fields` — an unrecognized key is a startup failure, not a
      silently ignored setting.

**Daemon integration**

- [ ] `tddy-daemon` gains a supervisor client. When a supervisor socket is configured, session
      spawning, cloning and sandbox spawning route through it.
- [ ] When a supervisor socket is configured but unreachable or refuses a request, the daemon
      **fails the operation**. There is no fallback to in-process spawning — a silent
      downgrade from "isolated session" to "session as the daemon user" is exactly the
      security regression this feature removes.
- [ ] With no supervisor configured, the daemon behaves as it does today (unchanged
      unprivileged path), so development without root keeps working.

**Install and migration**

- [ ] `./install --systemd` installs the `tddy-supervisor` binary, writes
      `tddy-supervisor.service` (root, `Delegate=yes`) and `tddy-supervisor.socket`, and
      installs a `supervisor.yaml` declaring `tddy-daemon` as an unprivileged managed service.
- [ ] `./install` no longer installs `tddy-daemon.service`; an existing one is disabled so the
      daemon is not started twice.
- [ ] Existing `INSTALL_*` override variables keep working; new ones follow the same naming.

### Non-Functional Requirements

- [x] **Auditable size.** The supervisor's dependency tree stays small enough to review. It links
      `tddy-rpc` for the wire protocol — over a crate-local `proto/supervisor.proto` and `build.rs`,
      deliberately *not* `tddy-service` — plus `nix`/`libc` for syscalls and serde/yaml for config.
      No HTTP server, no LiveKit, no git, no GitHub, no browser, no SQL engine, no TUI, no TLS.

      Measured: **53 crates link into the binary** (`cargo tree --edges normal`), 91 counting
      build- and dev-only edges. An earlier implementation routed the proto through `tddy-service`
      and reached **327**, dragging in `sqlx`/`libsqlite3-sys`, `ratatui`/`syntect`/`tui-markdown`,
      `axum`/`hyper`/`reqwest` and `rustls`/`ring` — none of it reachable from the supervisor's
      code, all of it in the supply-chain surface of the privilege boundary. Moving the proto to a
      crate-local home fixed it without changing the wire protocol.
- [ ] **Fail closed.** Every authorization and policy decision denies by default. An
      unparseable request, an unknown peer, or an unresolvable user is a rejection, never a
      permissive default.
- [ ] **Testable without root.** Policy and decision logic is pure and unit-tested; syscall
      execution sits behind narrow seams, following the pattern `tddy-sandbox-cgroups` already
      uses (`resolve_cgroup_base`, `unprivileged_userns_available_with`). Acceptance tests run
      the real binary as the invoking unprivileged user.
- [ ] **No behavioral branch on test environment.** The same code paths run in tests and
      production; only the injected filesystem root and target user differ.

## Design Risks

### Namespace setup moves into the privileged process

Today `enter_rootless_jail` runs inside the *unprivileged* daemon: it `unshare(CLONE_NEWUSER)`s
first and writes `uid_map = "0 <euid> 1"`, so the jailed process is root only inside a
namespace owned by an unprivileged uid. A bug there is an unprivileged-user bug.

Executing the same setup from a root supervisor changes that: a namespace created by uid 0 with
a naive map gives the child *real* root-mapped capabilities against the host user namespace,
and a mount bug is a host mount bug.

**Mitigation (binding on the implementation):** the supervisor drops to the target uid/gid in
`pre_exec` **before** `unshare(CLONE_NEWUSER)`, so the resulting namespace is owned by the
unprivileged target user and the uid map is identical to today's rootless map. The supervisor's
root privilege is used for exactly two things the daemon cannot do — choosing the target uid,
and writing the cgroup scope — and is surrendered before any namespace exists. Acceptance
tests pin the ordering.

### Naming collision

`tddy_sandbox::CgroupConfig::supervisor_leaf` (default `"supervisor"`) is an existing cgroup
*directory* name, unrelated to this process. The changeset must not conflate them; the
supervisor's own leaf keeps that name and the docs call out the distinction.

## Acceptance Criteria

- [ ] A host installed with `./install --systemd` runs exactly one tddy systemd unit, as root,
      and `tddy-daemon` appears as its unprivileged child.
- [x] Killing `tddy-daemon` results in the supervisor restarting it; killing it repeatedly and
      immediately stops after the configured retry ceiling rather than spinning.
- [ ] `systemctl stop tddy-supervisor` terminates the daemon and every session process.
- [ ] A session started for a GitHub user mapped to OS user `alice` runs as `alice`, while the
      daemon continues to run as `tddy`.
- [x] A request from a process that owns no declared service is rejected, and the rejection
      does not reveal whether the requested user or path exists.
- [x] A request naming an OS user or tool path outside the root-owned allowlists is rejected.
- [x] A request for memory/cpu/pids limits above the policy ceiling is clamped down, not
      honored and not rejected.
- [ ] A sandbox session receives a cgroup scope with the resolved limits applied, and the scope
      directory is removed when the session ends.
- [ ] With a supervisor configured but its socket removed, a session start fails with a clear
      error and no process is spawned as the daemon user.
- [ ] `tddy-daemon` no longer requires `Delegate=yes` or the AppArmor userns grant on its own
      unit.

## Related

- [systemd-install.md](../../daemon/systemd-install.md) — current install and unprivileged mode
- [packages/tddy-sandbox/docs/architecture.md](../../../../packages/tddy-sandbox/docs/architecture.md) — Linux cgroups jail, delegation containment
- `docs/dev/changesets.md` — `cgroups-sandbox-unprivileged` entry (the prior art this replaces)
