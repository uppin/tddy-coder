# Changeset: A supervised daemon knows it is supervised

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — many crates (supervisor, spawn, kernel, daemon, desktop, agent-launch, daemon-rpc, telegram-control), a config contract removal with migration in `./install`
**Stack**: #start-progress 3/8. PR: #736. Base: `feature/start-progress/worker-mux`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-supervisor-handoff-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md](../todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md) | ⚠ DURING | Its two fail-closed integration tests (`refuses_to_*_when_the_declared_supervisor_is_unreachable`) build a config with `supervisor:`. They are rewritten to declare the supervisor the new way (the handed-down choice); their guarantee — no fallback to a worker — is kept. The file split the entry asks for is not done here. |
| [2026-10-17-clone-passphrase-prompt-verification-gaps.md](../todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md) | — | Unrelated to the handoff; its supervised-clone gap is node 6's. |
| `packages/tddy-supervisor/docs/code-issues/complexity-server-serve.md` | — | Unclaimed; `serve` is not touched (the change is in `spawn_broker::ChildEnvironment`). |
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md` | ⚠ DURING | 586 lines. Two call sites switch from `spawn_backend_choice(&self.config)` to an injected value — net ≤ 0 lines. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-ensure-project-available-for-start-ensure-project-available-for-start.md` | ⚠ DURING | Same one-line substitution; no new branch. |
| `tddy-spawn`, `tddy-daemon` have no `docs/code-issues/` | ℹ | Not analyzed. |

## Responsibility

How a daemon learns it is supervised, and the removal of the config switch. Owns:

- `tddy-supervisor`: `ChildEnvironment::build` sets `TDDY_SUPERVISOR_SOCKET=<socket.path>` for a
  **declared managed service**, after the per-service `env:` overlay (so `supervisor.yaml` cannot set
  or spoof it), never for a session or sandbox spawn.
- `tddy-spawn`: `SpawnBackendChoice::from_environment()` — `Supervisor { socket_path }` when the variable
  is present, `ForkedWorker` otherwise; read once, pre-runtime. `spawn_backend_choice(&DaemonConfig)` is
  deleted.
- Injection: the startup choice is carried in the daemon's runtime state and read by the five call sites
  that re-derive it from config today (`tool_session_spawn.rs`, `svc_ensure_project_available_for_start.rs`,
  `coordinate_handlers.rs` ×2, telegram `workflow_spawn.rs`), and by `tddy-daemon/src/main.rs` and
  `tddy-desktop/src-tauri/src/lib.rs`.
- `tddy-daemon-kernel`: `DaemonConfig.supervisor` and `SupervisorClientConfig` removed; a `supervisor:`
  key fails `DaemonConfig::load` with a message naming the block as obsolete and saying to delete it.
- `./install`: on an existing `daemon.yaml` (system and `--user` modes) delete the `supervisor:` block
  and report it; render no block from `daemon.yaml.production`; drop the socket-path cross-check against
  `daemon.yaml`; fix the stale NOTE/WARNING text.
- Templates and docs: `daemon.yaml.production`, `dev.supervisor.yaml`, `docs/ft/daemon/systemd-install.md`,
  `docs/dev/tddy-*.service.example`, `daemon-kernel.md`.

## Boundaries

- Does not change the supervisor's socket path, group, mode, authorization or spawn policy.
- Does not add a fallback: a daemon told about a supervisor it cannot reach fails the spawn closed, as
  today.
- Does not stream supervised clone output (node 6).
- Does not touch the desktop app's behaviour (never supervised → `ForkedWorker`, as today).

## Dependencies

None in behaviour. The branch sits on `feature/start-progress/worker-mux` (2/8) only because the stack
is a line.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/worker-mux` (2/8) | `SpawnClient` in `spawn_worker/client.rs` | `spawn_worker_for(choice)` still returns it for `ForkedWorker` | change the worker or its client |
| `feature/start-progress/spawn-diag` (1/8) | `StagedCommand` privilege drop | nothing | touch the fork path |

## Draft PR contract

```rust
// tddy-supervisor/src/spawn_broker.rs
pub const SUPERVISOR_SOCKET_ENV: &str = "TDDY_SUPERVISOR_SOCKET";
// ChildEnvironment::build(plan, …) sets it for EnvironmentBase::Inherited (managed services) only

// tddy-spawn/src/supervisor_client.rs
impl SpawnBackendChoice {
    /// Reads TDDY_SUPERVISOR_SOCKET from `env` (std::env::var_os in production). Pure over its input.
    pub fn from_environment(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> SpawnBackendChoice;
}
// spawn_backend_choice(&DaemonConfig) — deleted

// tddy-daemon-kernel/src/config/daemon_methods.rs — DaemonConfig::load
// error: "<path>: the `supervisor:` block is obsolete — tddy-supervisor now tells the daemon its socket. Delete the block."
```

Plus the injected field on the runtime/session host the five call sites read (named in pass 2 from the
host type each call site already holds).

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — supervisor env tests, a pure backend-choice test, a config-load test
and an install script test; none needs another node's behaviour.
**Concurrent with:** #734 spawn-diag (1/8), #735 worker-mux (2/8), split-stream (4/8)
**Blocks:** supervised-clone-progress (6/8) — the supervised clone is reached through this choice

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-supervisor**: [README.md](../../../packages/tddy-supervisor/README.md) — managed-service environment
- **tddy-spawn**: [README.md](../../../packages/tddy-spawn/README.md) — backend choice
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — config key removed
- **tddy-daemon**, **tddy-desktop** — startup choice
- **tddy-agent-launch**, **tddy-daemon-rpc**, **tddy-telegram-control** — injected choice
- **tddy-e2e** — install tests
- Repo root: `install`, `daemon.yaml.production`, `dev.supervisor.yaml`

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-supervisor-handoff.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-supervisor-handoff.md)
- [systemd-install.md](../../ft/daemon/systemd-install.md), [tddy-supervisor.md](../../ft/supervisor/tddy-supervisor.md)

## Summary

The supervisor tells the daemon its socket through the environment; `daemon.yaml`'s `supervisor:` block
is removed, refused if present, and deleted by `./install` on upgrade.

## Background

`mantas-udoo-ros` ran under `tddy-supervisor` with a `daemon.yaml` that predated the supervisor, so it
had no `supervisor:` block and cloned through its own unprivileged worker — which can never become
another user. `install` preserves an existing `daemon.yaml`, and prints only a NOTE (with stale text)
when the block is missing. A config value whose only correct setting is "matches how the process was
started" should not be a config value.

## Scope

- [x] Supervisor sets `TDDY_SUPERVISOR_SOCKET` for managed services, unspoofable by `env:`
- [x] `SpawnBackendChoice::from_environment`; `spawn_backend_choice` deleted
- [x] Startup choice injected into the five call sites, `main.rs`, desktop `lib.rs`
- [x] `DaemonConfig.supervisor` removed; obsolete-key error
- [x] `./install` strips the block from an existing `daemon.yaml` and reports it; no block rendered; cross-check removed
- [~] Templates, examples, `systemd-install.md`, `daemon-kernel.md` updated — `daemon-kernel.md` and `packages/tddy-supervisor/docs/architecture.md` are deferred to `/wrap-context-docs` (CLAUDE.md: package docs change through the changeset, not directly); the rest is done
- [x] Existing tests carrying `supervisor:` rewritten to the new declaration

## Technical Changes

### State A
`spawn_backend_choice(&DaemonConfig)` reads `config.supervisor` (presence = switch); called pre-tokio
and re-derived at five operation sites. `install` renders the block for new files only; an existing
file is never changed. The supervisor gives managed services its own env + `env:` overlay + `LISTEN_*`.

### State B
The supervisor adds `TDDY_SUPERVISOR_SOCKET` after the overlay. The daemon reads it once before the
runtime starts, keeps the `SpawnBackendChoice` in its state, and every spawn/clone site uses that.
`daemon.yaml` has no supervisor key; a leftover one is a named startup error, and `install` removes it.

### Delta
Per package as listed in Responsibility.

Green implementation notes (2026-10-24):
- `EnvironmentBase::Inherited` now carries `supervisor_socket`, so the socket can only reach a
  declared managed service through the variant that describes one; `Slot::spawn_plan` supplies it.
- `ChildEnvironment::build` drops `TDDY_SUPERVISOR_SOCKET` for every child before the managed-service
  value is inserted, so neither a session request nor an `env:` overlay can set it.
- The obsolete-key check runs on the untyped YAML before typed deserialization (`daemon_methods.rs`).
- `./install`: `drop_top_level_yaml_block` now removes the block from an existing `daemon.yaml` in
  both system and `--user` modes, keeps the file's mode and owner, and reports the removal.

Package docs to fold in at `/wrap-context-docs`: `packages/tddy-daemon-kernel/docs/daemon-kernel.md`
(the `config` layout: no `supervisor:` section, `load` refuses one) and
`packages/tddy-supervisor/docs/architecture.md` (managed services are told `TDDY_SUPERVISOR_SOCKET`
after the `env:` overlay). `packages/tddy-daemon/docs/daemon-endpoint.md` still names
`RuntimeOptions::for_binary()` without its new argument.

## Testing Plan

- Supervisor: integration test in the style of `supervisor_socket_handoff.rs` — a declared service
  observes the variable; one whose `env:` sets it observes the supervisor's value; a session spawn does
  not receive it.
- Spawn: pure tests of `from_environment`.
- Kernel: `a_daemon_loaded_from` config test for the obsolete key.
- Install: `tddy-e2e/tests/install_supervisor.rs` style tests with `INSTALL_NO_SYSTEMCTL=1` and path
  overrides — an existing `daemon.yaml` with the block loses it and the run says so; a fresh install
  renders none.
- Rewritten fail-closed tests keep "unreachable declared supervisor refuses the spawn".

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_managed_service_is_told_the_supervisor_socket` | `packages/tddy-supervisor/tests/supervisor_service_environment.rs` (new) | variable present with `socket.path` |
| `a_service_env_entry_cannot_override_the_supervisor_socket` | same | overlay order |
| `a_session_spawn_is_not_told_the_supervisor_socket` | same | sessions get Minimal env without it |
| `a_daemon_told_of_a_supervisor_spawns_through_it` / `…_not_told_forks_its_own_worker` | `packages/tddy-spawn/tests/supervisor_routing.rs` (rewritten) | `from_environment` |
| `a_daemon_yaml_with_a_supervisor_block_is_refused_as_obsolete` | `packages/tddy-daemon-kernel/src/config.rs` tests (beside `a_session_room_poll_faster_than_the_floor_is_refused_by_name`) | message names block + remedy |
| `upgrading_removes_the_supervisor_block_from_an_existing_daemon_yaml_and_says_so` | `packages/tddy-e2e/tests/install_supervisor.rs` | install migration |
| `a_fresh_install_renders_no_supervisor_block` | same | template |

## Technical Debt & Production Readiness

- Children the daemon spawns itself (not through `tddy-supervisor`) inherit the daemon's environment,
  so they can read `TDDY_SUPERVISOR_SOCKET`. `ChildEnvironment` is the only place the variable is set
  or stripped, so this path is not covered by it. The socket is reachable only by `tddy-clients`
  members and the daemon's user is already one, so this gives no new capability. Whether to clear it
  on those spawns is a decision for the developer.

## Decisions & Trade-offs

- **Environment variable, not an inherited fd.** The supervisor client connects per operation by design
  (the supervisor may restart); a path fits that, a connected fd does not. Set after the `env:` overlay,
  like `LISTEN_*`, so config cannot forge it.
- **Obsolete key is an error, not ignored** (developer's choice), with `install` removing it so upgrades
  work.
- **No fallback** when the named socket is unreachable — unchanged fail-closed behaviour.
- **The socket travels in the `Inherited` variant** (`EnvironmentBase::Inherited { supervisor_socket }`),
  not in a separate argument. A session plan has no field that could carry it, so "only a declared
  managed service is told" is a type fact, not a convention.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-supervisor-handoff-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for the whole of pass 2, 2026-10-24)
- [x] TDD Red — write failing unit/integration tests
- [~] TDD Green — implement with quality code. This changeset's own contract is green; `tddy-spawn` `pre_exec_safety` / `clone_privilege_acceptance` and `tddy-daemon-kernel` `pre_exec_stages_acceptance` / `privilege_change_acceptance` stay red — node 1's (spawn-diag, #734) privilege-drop contract, not implemented on this base (`spawner.rs` still calls `initgroups` by hand)
- [x] Update documentation with progress
- [~] Run scoped tests (`./test -p tddy-supervisor -p tddy-spawn -p tddy-daemon-kernel -p tddy-e2e` + compile of the injected crates); full-workspace result from CI. Green: `tddy-daemon-kernel` lib (118, incl. the obsolete-key test), `tddy-spawn` `supervisor_routing`, `install_script` (29), `install_supervisor` (20, incl. both migration tests), `cargo check --all-targets` for the five injected crates, `cargo check -p tddy-desktop`. `supervisor_service_environment.rs` is `cfg(linux)`: it compiles to zero tests on this macOS host, and no Linux target is installed here, so it has neither run nor been compiled for Linux
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (scoped clippy, `cargo fmt`) — `cargo clippy -p` for the nine touched packages with `-D warnings` exits 0; rustfmt `--check` is clean on the 131 Rust files in the PR diff
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
