# Changeset: Install derives the daemon user from supervisor.yaml

**Date**: 2026-10-31
**Status**: 🚧 In Progress
**Type**: Bug Fix (ownership/identity drift between install and the preserved supervisor config)
**Complexity**: 2 — root `install` script + one package's e2e tests (`tddy-e2e`); the risky seam is
targeted YAML extraction in bash; no concurrency, no migrations, no contract change

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-31-install-derives-daemon-user-from-supervisor-config-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

**CRITICAL**: List ALL packages with documentation changes:

- **tddy-e2e**: [tests/install_supervisor.rs](../../packages/tddy-e2e/tests/install_supervisor.rs) — new acceptance tests for user derivation and refusal paths
- **daemon product docs**: [systemd-install.md](../ft/daemon/systemd-install.md) — env-var table, Behavior notes, One-root-process section move from "install decides" to "supervisor.yaml decides, install obeys" (at wrap)
- Root artifacts (no package): `install`, `supervisor.yaml.production` header comments

## Related Feature Documentation

- [systemd-install](../ft/daemon/systemd-install.md) — the operator contract this change rewrites
- [tddy-supervisor](../ft/supervisor/tddy-supervisor.md) — supervisor.yaml as the privilege surface

## Summary

On an upgrade (existing `supervisor.yaml`), `./install` stops deriving the daemon's OS user/group
from `INSTALL_DAEMON_USER`/`INSTALL_DAEMON_GROUP` and instead **reads the `tddy-daemon` service's
`user:`/`group:` back from the preserved config**, doing account creation, socket-group membership
and the log/auth-dir `chown -R` for *that* identity. The env vars remain only the seed for a fresh
install's rendered config; an env var that disagrees with an existing config is a hard error.

## Background

A host whose operator changed `supervisor.yaml`'s `user:` (or re-chowned directories by hand) ends
up with the supervisor spawning the daemon as one user against directories owned by another — the
daemon crash-loops with bare `Permission denied` on its log files and signing key, and a restart
never heals it because nothing in the runtime re-chowns. Root cause: `INSTALL_DAEMON_USER` is a
second source of truth that install renders into `supervisor.yaml` once and then keeps acting on
independently, forever. `daemon.yaml` cannot own the uid (the parent chooses the user before the
daemon process exists, and the daemon is unprivileged by design), so `supervisor.yaml` — which
already declares `user:`/`group:` per service as part of the host's privilege surface — becomes the
single source.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into: **none** — the scan
(`INSTALL_DAEMON_USER|daemon user|chown|useradd` over `docs/dev/todo/`) returned four files, all
about unrelated socket diagnostics, host-socket test coverage, supervisor gap lists and a stack
status note; none blocks or interacts with install's account derivation.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Implementation**: `install` derives the daemon user/group from the existing supervisor config on upgrades; fresh-install seeding unchanged
- [x] **Testing**: e2e acceptance tests for derivation, disagreement refusal, missing-key refusal, root refusal; VM-backed real-chown coverage located or noted
- [ ] **Package Documentation**: `docs/ft/daemon/systemd-install.md` + `supervisor.yaml.production` comments updated (at wrap — template comments already updated with the code)
- [ ] **Code Quality**: scoped lint/test gates for touched packages green *(tests + clippy green; final re-run at wrap)*

## Technical Changes

### State A (Current)

- `install` resolves `DAEMON_USER="${INSTALL_DAEMON_USER:-tddy}"`, `DAEMON_GROUP="${INSTALL_DAEMON_GROUP:-$DAEMON_USER}"` (lines 325–326) and uses them for: root rejection (920), `groupadd`/`useradd`/`usermod` (1083–1095), `chown -R` of log + auth dirs (1099), and rendering `__DAEMON_USER__`/`__DAEMON_GROUP__` into a *newly created* `supervisor.yaml` (1163–1164).
- An existing `supervisor.yaml` is preserved verbatim; the env vars still drive the account branch on every system install, unrelated to what the config declares.
- The account branch sits inside `elif [[ "$NO_SYSTEMCTL" != "1" ]]`, so e2e tests never observe it.
- Docs present `INSTALL_DAEMON_USER` as the way to choose the user, with `supervisor.yaml` mentioned only as the place it is *declared*.

### State B (Target)

- When `$SUPERVISOR_CONFIG_FILE` exists (system install), install extracts the `tddy-daemon` service's `user:` and `group:` from it and those values drive the root-rejection check, account creation, socket-group membership and the `chown -R`.
- `INSTALL_DAEMON_USER`/`INSTALL_DAEMON_GROUP` are honored only when `supervisor.yaml` is about to be created; on an upgrade, setting either to a value that disagrees with the config is a hard error naming both values and the remedy (drop the env var, or edit the config and re-install).
- A config whose `tddy-daemon` service block lacks `user:` or `group:` (or has no `tddy-daemon` service) is a hard error naming the remedy — no silent fallback to `tddy`.
- A config-derived user of `root` is rejected with the same message as today's env-var rejection.
- Narration states the source of the identity: `install: daemon service user read from <path>: <user>`.
- `--user` mode is untouched: no supervisor config exists, the daemon runs as the invoking user.

### Delta

#### Root `install`
- **Implementation**: new extraction step (targeted, indentation-aware read of the `tddy-daemon` service block) placed before the account branch; `DAEMON_USER`/`DAEMON_GROUP` become derived values (env-seeded on fresh install, config-read on upgrade); disagreement and missing-key failures added; narration updated.
- **`supervisor.yaml.production`**: header comments updated to state the file owns the daemon's OS identity and install obeys it on upgrade.

#### tddy-e2e
- **Testing**: new acceptance tests in `tests/install_supervisor.rs` exercising the decision paths through the rootless harness (exit status, stderr, narration); existing tests unchanged.

#### daemon product docs (at wrap)
- **Documentation**: env-var table rows, Daemon state / Service user / Directory ownership notes rewritten around config-authority.

## Implementation Milestones

- [x] Milestone 1: Red — failing e2e tests for the four decision paths (derivation, disagreement, missing key, root)
- [x] Milestone 2: Green — install implements config-derived identity with loud failures and narration
- [x] Milestone 3: VM-backed real-chown coverage located (or gap recorded), scoped gates green
- [ ] Milestone 4: Docs + template comments updated, changeset wrapped

**Milestone 3 finding**: the VM image chain (`packages/tddy-vm/src/tddy_host.rs`) bakes
`./install --systemd` into the guest with the cloud-init login account equal to install's default
`INSTALL_DAEMON_USER=tddy` — so the real `useradd`/`chown` path runs end-to-end in VM tests, but
only ever with the default identity. Deriving a *different* user in a VM bake would cost an image
re-bake measured in hours; the derivation logic itself is fully covered by the rootless e2e tests.
Recorded as an accepted gap rather than new VM coverage.

## Testing Plan

### Testing Strategy

**Test Level**: E2E (functional, rootless) for the decision paths; existing VM-backed suites for the privileged actions.

**Why**: the change *is* the install script's observable decision-making; the existing `install_supervisor.rs` harness runs the real script in a temp tree and asserts produced files, exit status and narration — the highest level reachable without root. `useradd`/`chown` need root and stay with the VM-backed coverage (locate during Milestone 3).

### Testing Options Analysis

#### Option 1 (Primary): Functional e2e via `install_supervisor.rs` harness
**Test Level**: E2E (rootless)
**Scope**: upgrade-time derivation and every refusal path, via `overwrite_installed_config` to plant operator configs and `attempt_install_with` for failure assertions.
**Assertions**:
- [ ] Upgrade with config `user: mantas`/`group: mantas` succeeds and narrates the config-derived user
- [ ] `INSTALL_DAEMON_USER=other` over config `user: tddy` fails, naming both values and the remedy
- [ ] Config service block without `user:` (or without a `tddy-daemon` service) fails with the remedy, and does **not** fall back to `tddy`
- [ ] Config declaring `user: root` fails with the existing root-rejection message
- [ ] Fresh install still renders `INSTALL_DAEMON_USER` into the new config (existing test extended if needed)

#### Option 2 (Complementary): VM-backed real install
**Description**: locate the existing VM test that runs `./install` in a guest (`packages/tddy-vm` references it) and extend it to assert the account/chown follow a modified config user; if no such suite exists, record the gap in `## Decisions & Trade-offs` instead of inventing a new VM bake.

#### Option 3 (Not applicable): Unit tests
The logic lives in a bash script consumed end-to-end; unit-shredding it would test text against text. Rejected.

## Acceptance Tests

### tddy-e2e
- [x] **E2E**: upgrade derives account identity from the existing supervisor config (`tests/install_supervisor.rs` — `an_upgrade_derives_the_daemon_identity_from_the_existing_supervisor_config`)
- [x] **E2E**: disagreeing `INSTALL_DAEMON_USER` on upgrade is refused (`tests/install_supervisor.rs` — `an_install_daemon_user_disagreeing_with_the_existing_config_is_refused`)
- [x] **E2E**: disagreeing `INSTALL_DAEMON_GROUP` on upgrade is refused (`tests/install_supervisor.rs` — `an_install_daemon_group_disagreeing_with_the_existing_config_is_refused`)
- [x] **E2E**: config without a usable `user:`/`group:` is refused, no fallback (`tests/install_supervisor.rs` — `an_existing_config_whose_daemon_service_declares_no_user_is_refused`, `…_no_group_is_refused`)
- [x] **E2E**: config declaring `root` is refused (`tests/install_supervisor.rs` — `an_existing_config_declaring_root_as_the_daemon_user_is_refused`)
- [x] **E2E**: fresh install seeding unchanged (`tests/install_supervisor.rs` — `a_fresh_install_renders_the_requested_daemon_identity_into_the_supervisor_config`)

## Technical Debt & Production Readiness

- [ ] Verify AGENTS.md / README mentions of `INSTALL_DAEMON_USER` still read correctly after the change (wrap step)

## Decisions & Trade-offs

- **`supervisor.yaml`, not `daemon.yaml`, owns the uid** — the daemon cannot `setuid` itself; its parent chooses the identity before any config is read. Rejected the developer's initial "daemon config" destination with this reasoning; developer approved the supervisor-config destination.
- **Error, not warning, on env-vs-config disagreement** — unlike a stale socket path (warned), identity drift breaks the daemon outright; silently ignoring the env var would also surprise operators scripting installs.
- **No silent fallback when the config lacks `user:`/`group:`** — repo rule: no fallbacks without consent. The rendered template always carries both, so absence means operator damage; the remedy is named.
- **Decision runs outside the `INSTALL_NO_SYSTEMCTL` guard** — so the rootless harness can test the decision; only the privileged actions (useradd/usermod/chown) stay behind the guard.
- **Known gap**: real `useradd`/`chown` behavior is not reachable from the rootless e2e harness; Milestone 3 either extends VM-backed coverage or records the gap explicitly.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
- [ ] (none yet)

## Validation Results

### Scoped local run (2026-10-31)

`./test -p tddy-e2e --lib --test install_supervisor --test install_script` — **79 passed, 0 failed**
(23 contract checks, 29 install_script, 27 install_supervisor including the 7 new tests); red run
before implementation had exactly the 5 new decision-path tests failing and nothing else.
`cargo clippy -p tddy-e2e -- -D warnings` — clean. Full-repo verification belongs to CI per
[AGENTS.md](../../AGENTS.md).

## References

- Initial discovery: [2026-10-31-install-derives-daemon-user-from-supervisor-config-initial-discovery.md](./2026-10-31-install-derives-daemon-user-from-supervisor-config-initial-discovery.md)
- [systemd-install](../ft/daemon/systemd-install.md) — State A operator contract
- [tddy-supervisor](../ft/supervisor/tddy-supervisor.md) — supervisor.yaml privilege-surface model
