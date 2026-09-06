# 2026-08-02 — tddy-supervisor follow-ups

**Category:** Future enhancement
**Source:** tddy-supervisor changeset, 2026-08-02

- **PTY spawning still drops privilege by shelling out to `setpriv`.**
  `packages/tddy-daemon/src/pty_runtime.rs` (`pty_requires_privilege_drop`,
  `wrap_argv_for_privilege_drop`) prefixes terminal argv with
  `setpriv --reuid --regid --init-groups --`, a second, unrelated privilege-drop mechanism next to
  the supervisor's `setuid`. It should route through `SpawnSession` so there is exactly one path
  and one allowlist. Out of scope here because PTY spawning also carries the pty master fd, which
  has to cross the socket via `SCM_RIGHTS` — its own design problem.
- **`spawn_worker.rs`'s fork-before-tokio machinery becomes dead weight on supervised hosts.**
  `fork_spawn_worker()` exists only because `fork()` from a multi-threaded process can deadlock;
  with the supervisor as *parent*, the daemon never needs to fork at all. Keep it while the
  no-supervisor deployment is supported, then delete it (along with the JSON-over-pipes
  `WorkerRequest`/`WorkerResponse` protocol and the `spawn_worker_request_timeout_secs` setting).
- **`tddy-sandbox-app` on Linux can stop routing through the daemon.**
  `packages/tddy-sandbox/docs/architecture.md` explains it delegates to the daemon purely because
  cgroup v2 delegation containment stops an unprivileged app placing its own child in a limited
  scope. A supervisor that owns the delegated subtree removes that reason — the app could hold a
  supervisor client directly and skip the daemon hop entirely.
- **`supervisor.proto` is outside `buf lint` coverage.**
  `packages/tddy-service/buf.yaml` lints that crate's whole `proto/` directory; moving
  `supervisor.proto` into `packages/tddy-supervisor/proto/` to keep the uid-0 binary's dependency
  tree small took it out of lint scope. `tddy-terminal-rpc` has the same gap, so this is a
  workspace-wide pattern rather than a new regression — but the *privileged surface's* proto is the
  one most worth linting. A 4-line `buf.yaml` per proto-owning crate closes it.
- **`cpu_max_ceiling` cannot express the kernel's `"max <period>"` form.**
  `policy::CpuMax::from_str` accepts exactly two integers, because that is all the tests pin. But
  the kernel writes `cpu.max` as `"max 100000"` when a cgroup is uncapped, so an operator who
  copies that value into `cgroup.cpu_max_ceiling` gets a `SupervisorError::Invalid` on *every*
  scope creation — at runtime, not at config load. Fixing it needs an explicit `CpuMax::Max`
  variant with its own tests, plus load-time validation of the ceiling, not a quiet parse
  fallback. Discovered implementing Milestone 1.
- **`detect_and_prepare_base`'s process-global `OnceLock` means a cgroup topology change needs a
  supervisor restart.** Acceptable today (the base is stable for a boot), but worth revisiting if
  the supervisor ever has to survive a re-delegation.
