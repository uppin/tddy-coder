# 2026-10-24 — A supervised daemon learns it is supervised from its environment

**Type:** Bug Fix

`#start-progress` 3/8 ([#736](https://github.com/uppin/tddy-coder/pull/736)); base
[#735](https://github.com/uppin/tddy-coder/pull/735) (worker-mux). Replaces the `daemon.yaml`
`supervisor:` switch with a handoff from `tddy-supervisor`, across:

- `tddy-supervisor` — `ChildEnvironment::build` strips `TDDY_SUPERVISOR_SOCKET` from every child and sets
  it to `socket.path` for a declared managed service after the `env:` overlay; the value travels in
  `EnvironmentBase::Inherited { supervisor_socket }`, supplied by `Slot::spawn_plan`.
- `tddy-spawn` — `SpawnBackendChoice::from_environment` / `adopt_from_environment` (choice plus the
  variables consumed, `AdoptedSpawnBackend::consume_from_process_environment`); `spawn_worker_for`.
- `tddy-daemon-kernel` — `DaemonConfig.supervisor` and `SupervisorClientConfig` removed; `load` refuses a
  leftover `supervisor:` block by name.
- `tddy-daemon`, `tddy-desktop` — read and consume the variable first thing in `main` / `run`, before any
  thread; the choice rides in `RuntimeOptions::for_binary(choice)`.
- `tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-session-lifecycle`, `tddy-telegram-control` — the five
  former `spawn_backend_choice(&config)` sites take the injected choice.
- `./install` — removes the block from an existing `daemon.yaml` (system and `--user`), keeping mode and
  owner; templates and service examples render none.

**Decisions.** Environment variable, not an inherited fd (the client connects per operation; the
supervisor may restart). Obsolete key is an error, not ignored (developer's choice), with `install`
migrating. No fallback when the socket is unreachable. The daemon consumes the variable pre-runtime so
children it spawns itself (not through the supervisor) do not inherit it — closed at `/validate-changes`;
the pure half is pinned without mutating a shared test binary's environment.

**Tests.** `supervisor_service_environment.rs` (new, Linux only — not run on the macOS dev host),
`supervisor_routing.rs` (rewritten, 5), `a_daemon_yaml_with_a_supervisor_block_is_refused_as_obsolete`,
`install_supervisor.rs` migration tests, rewritten fail-closed `supervisor_spawn_delegation.rs`.

**Code issues.** `tddy-supervisor` `spawn_broker.rs` (1526 → 1549) and `supervisor.rs` (965 → 972) grew
past an already-breached 500-line budget; recorded as `oversized-file-*`, split deferred with the
developer's consent (stack-shared files).

**Feature docs:** [systemd install](../../ft/daemon/systemd-install.md),
[tddy-supervisor](../../ft/supervisor/tddy-supervisor.md).
