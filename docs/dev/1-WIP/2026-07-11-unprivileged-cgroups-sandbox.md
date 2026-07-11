# Changeset: Unprivileged (`User=tddy`) cgroups sandbox via `Delegate=yes` + AppArmor

**PRD**: `docs/ft/coder/sandbox-builder.md`
**Branch**: `unprivileged-cgroups-sandbox`

## Checklist

- [x] Create changeset
- [x] Write failing tests (RED)
- [x] tddy-sandbox: `CgroupConfig` type + additive `SandboxPlan.cgroup` field
- [x] tddy-sandbox-cgroups: functional userns probe (real `unshare` + uid/gid mapping)
- [x] tddy-sandbox-cgroups: runtime-derived, config-overridable delegated cgroup base
- [x] tddy-sandbox-cgroups: no-internal-processes relocation (supervisor leaf + subtree_control)
- [x] tddy-sandbox-cgroups: rewire `spawn`/`spawn_plan`; unique per-session scope (`next_seq`)
- [x] tddy-daemon: `SandboxCgroupConfig` section + `sandbox_cgroup_config()` mapping, threaded into `build_sandbox_plan`
- [x] install: flip default unit to `User=tddy`/`Delegate=yes`/`AppArmorProfile`; create service user; chown dirs; ship + auto-load AppArmor profile
- [x] config templates + service example + architecture doc
- [x] Fix pre-existing `tddy-sandbox` lib-test compile break (`exec_reads.rs` macOS-gated `PathBuf`)

## Motivation

Production `tddy-daemon` runs as the unprivileged OS user `tddy` (root is not an option). The Linux
cgroups sandbox failed at spawn because (1) the userns precondition read
`kernel.apparmor_restrict_unprivileged_userns` (Ubuntu 24.04 default `1`) instead of functionally
testing userns, so a per-binary AppArmor grant was invisible; and (2) the cgroup scope was hardcoded
to the `/sys/fs/cgroup` root, which `tddy` cannot write — `Delegate=yes` grants only a subtree. This
change makes the sandbox work under an unprivileged delegated service while preserving the
"never degrade to an unconfined/unlimited process" invariant.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-daemon/apparmor/tddy-daemon` | AppArmor profile template (`userns` grant); `__INSTALL_BIN_DIR__` rendered + loaded by `./install` |
| `docs/dev/1-WIP/2026-07-11-unprivileged-cgroups-sandbox.md` | This changeset |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-sandbox/src/builder.rs` | `CgroupConfig` type; additive `SandboxPlan.cgroup` (Default-empty); set in `build()` |
| `packages/tddy-sandbox/src/lib.rs` | Re-export `CgroupConfig` |
| `packages/tddy-sandbox-cgroups/src/lib.rs` | Functional userns probe + `probe_write`; `/proc/self/cgroup` + mountinfo parse; `resolve_cgroup_base`; controllers/subtree_control/scope helpers; relocation seams; `detect_and_prepare_base` (`OnceLock`); rewired `spawn`/`spawn_plan`; removed hardcoded `CGROUP_ROOT`/`cgroup_scope_path`/`prepare_cgroup_scope` |
| `packages/tddy-daemon/src/config.rs` | `SandboxCgroupConfig` + `DaemonConfig.sandbox_cgroup` + `sandbox_cgroup_config()` |
| `packages/tddy-daemon/src/sandbox_session.rs` | `SandboxRunnerSpawn.cgroup`; set `plan.cgroup` in `build_sandbox_plan` |
| `packages/tddy-daemon/src/connection_service.rs` | Thread `self.config.sandbox_cgroup_config()` at 3 spawn call sites |
| `install` | `INSTALL_DAEMON_USER`/`_GROUP`/`INSTALL_APPARMOR_DIR`; create service user; chown log/auth; flip unit; install+load AppArmor profile |
| `docs/dev/tddy-daemon.service.example` | Unprivileged unit example |
| `daemon.yaml.production`, `dev.daemon.yaml` | Documented commented `sandbox_cgroup:` block |
| `packages/tddy-sandbox/docs/architecture.md` | Revise "runs as a root systemd service" → unprivileged `Delegate=yes` + AppArmor |
| `packages/tddy-sandbox-cgroups/tests/cgroups_spawn.rs`, `packages/tddy-sandbox-cgroups/src/lib.rs` (tests), `packages/tddy-daemon/src/config.rs` (tests), 4 daemon test files | Tests + mechanical `cgroup: Default::default()` on `SandboxRunnerSpawn` literals |
| `packages/tddy-sandbox/src/exec_reads.rs` | Pre-existing fix: unconditional `use std::path::PathBuf` in test module |

## Design decisions

### Functional userns probe, not a sysctl read
On Ubuntu 24.04 `unshare(CLONE_NEWUSER)` succeeds unprivileged; the AppArmor restriction gates the
uid/gid **mapping** writes. So `probe_unprivileged_userns` forks a child that performs the real
`unshare` + `setgroups=deny` + uid/gid mapping (mirroring `enter_rootless_jail`) and reports success
— the only way to detect a per-binary AppArmor grant. The child is async-signal-safe (id-maps
formatted in the parent; raw `open`/`write`/`close` via `probe_write`). Root short-circuits to
available without probing. The decision core is a pure `userns_available_from(is_root, probe)`.

### Delegated base derived at runtime, config-overridable — nothing hardcoded
`detect_and_prepare_base` reads `/proc/self/cgroup` + `/proc/self/mountinfo` and resolves the base
(config `base_path` override → derive from the `0::` line joined with the cgroup2 mount root →
`/sys/fs/cgroup` default). One-time (`OnceLock`): relocate the daemon's own thread group into a
`supervisor` leaf (via `cgroup.procs`, the whole TGID), then enable controllers in the base's
`subtree_control` — satisfying cgroup v2's no-internal-processes rule. Per session: a uniquely-named
scope (`tddy-<name>-<seq>.scope`, `seq` from a process-global `AtomicU64`) so concurrent sessions of
one project never collide.

### Fail-fast preserved
Base detection, self-relocation, scope creation, and moving the child PID are fail-fast
(`SandboxError::Unsupported`). Only controller-enable and limit-writing stay best-effort (`log::warn`,
matching prior behavior) — some hosts don't delegate every controller.

### Config threaded, not hardcoded
`spawn_plan` keeps its signature; `CgroupConfig` rides on `SandboxPlan` (additive, Default-empty), so
macOS/QEMU backends ignore it and every existing construction still compiles. The daemon maps its
optional `sandbox_cgroup:` section onto it.

### Install flips to unprivileged by default
`./install` now emits `User=tddy`/`Group=tddy`/`Delegate=yes`/`AppArmorProfile=tddy-daemon`, creates
the (configurable) service user, chowns the log/auth dirs, and installs+loads the AppArmor profile
before starting the service. `INSTALL_DAEMON_USER=root` restores root (multi-user setuid) mode.

## Unit/integration tests

**tddy-sandbox-cgroups — pure unit** (`src/lib.rs`): userns decision (available/denied/root-no-attempt);
`/proc/self/cgroup` v2 parse (path, root `/`, v1-only → None); cgroup2 mount-root parse; base override
vs derivation; controllers default/override; subtree_control line; unique scope naming. (13)

**tddy-sandbox-cgroups — seam** (`tests/cgroups_spawn.rs`, tempdir-injected): relocate self into leaf;
enable controllers in subtree_control; move pid into scope. (3)

**tddy-daemon — config** (`src/config.rs`): `sandbox_cgroup:` YAML maps onto `CgroupConfig`. (1)

> The host-touching probe/relocation and `spawn`/`spawn_plan` rewiring are exercised in a
> delegation-capable environment (systemd `Delegate=yes` + loaded AppArmor profile); the acceptance
> tests `jail_smoke`/`stdio_piping` self-skip when the host cannot provide userns.

## Out of scope / follow-ups

- Per-session teardown of empty scope dirs on session end (rmdir).
- `pivot_root` read-only-root filesystem write-confinement (existing follow-up).

## Validation Results

- **clippy** (`-D warnings`, tddy-sandbox / tddy-sandbox-cgroups / tddy-daemon lib+bins): clean.
- **fmt**: edited files formatted (`rustfmt`).
- **Tests**: tddy-sandbox 23+1 pass; tddy-sandbox-cgroups 17 lib + 5 seam pass (host acceptance
  `jail_smoke`/`stdio_piping` self-skip — no userns without the profile in a bare `cargo test`);
  tddy-daemon `--lib` 266 pass. No regressions.
- **Prod-readiness scan** of the diff: no `todo!`/`unimplemented!`/`dbg!`/`println!` in production
  paths, no "red/green phase" comments, no new FIXME/TODO. Pre-existing `FIXME(fs-confinement)` on
  `spawn`/`spawn_plan` left intact.
- **Correctness fix applied during green** (beyond the implementer): the userns probe only tested
  `unshare(CLONE_NEWUSER)`, which succeeds unprivileged on Ubuntu 24.04 — the AppArmor restriction
  gates the uid/gid *mapping* writes. That probe returned a false-positive `Available` and would not
  detect the AppArmor grant. `probe_unprivileged_userns` now performs the full `unshare` + uid/gid
  mapping (async-signal-safe), so `Available` means the jail's userns setup genuinely works.
- **install** `bash -n` syntax check: clean.
- **Not run**: full workspace `cargo test` (host has pre-existing runner-dependent failures unrelated
  to this change); validated per affected package instead. **Deferred**: end-to-end deploy check
  (`./release && sudo INSTALL_OVERWRITE_SYSTEMD_UNIT=1 ./install --systemd` → sandboxed session) —
  see the plan's Verification section; requires an install on the target host.
