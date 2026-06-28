# Changeset: Linux cgroups sandbox + shared runner/host-relay + sandbox session flag

**Date**: 2026-06-28
**Status**: 🚧 In Progress
**Type**: Feature

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests — verify 100% pass
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps

## Affected Areas

- **New crate** (`packages/tddy-sandbox-runner/src/`): shared in-jail runner + host relay
  - `runner.rs` — moved from `tddy-sandbox-darwin`; gains AF_UNIX gRPC server (`serve_grpc_over_uds`)
  - `main.rs` — `tddy-sandbox-runner` binary
  - `host_relay.rs` — extracted host-side `SessionChannel` driver (reader/poller/tunnel/egress),
    parameterized by a `HostToolHandler`
  - `lib.rs` — re-exports `run_sandbox_runner`, `SandboxRunnerArgs`, `connect_sandbox_client`,
    `connect_sandbox_client_uds`, `run_host_relay`, `HostToolHandler`
- **New crate** (`packages/tddy-sandbox-cgroups/src/`): Linux rootless backend (`#![cfg(target_os="linux")]`)
  - `lib.rs` — `spawn(SandboxSpec)`, `detect_allow_read_paths()`
  - `userns.rs`, `cgroup.rs`, `mounts.rs`, `netns.rs`
- **Sandbox core** (`packages/tddy-sandbox/src/`): `spec.rs` — cgroup limit fields on `SandboxSpec`;
  `grpc_socket_path` documented as the UDS path
- **Darwin sandbox** (`packages/tddy-sandbox-darwin/src/`): `runner.rs`/`main.rs` removed; depends on
  and re-exports `tddy-sandbox-runner`; `spawn.rs`/`profile.rs` unchanged
- **Daemon** (`packages/tddy-daemon/src/`): `sandbox_session.rs` — Linux `spawn_sandbox_runner` arm,
  UDS `connect_sandbox_client`, `dial_and_bridge` collapsed onto `run_host_relay`, runner-argv builder
  swaps `--grpc-listen-port`→`--grpc-uds` on Linux
- **CLI** (`packages/tddy-coder/src/`): `run.rs` — `--sandbox` flag → `StartSessionRequest.sandbox`
- **Web** (`packages/tddy-web/src/`): new-session form — `sandbox` toggle → RPC `sandbox: true`
- **Testing commons** (`packages/tddy-testing-commons/src/`): `sandbox_session_channel.rs` collapsed
  onto `run_host_relay` with a stub tool handler
- **Standalone app** (`packages/tddy-sandbox-app/src/`): `bridge.rs` routed through `run_host_relay`
  (drops the cross-import of `relay_egress_request` from the daemon)
- **Integration tests** (`packages/tddy-integration-tests/tests/`): new egress-relay-over-TLS test
- **Workspace** (`Cargo.toml`): add the two new crate members

## Related Feature Documentation

- [PRD-2026-06-28-linux-cgroups-sandbox.md](../../ft/daemon/1-WIP/PRD-2026-06-28-linux-cgroups-sandbox.md)
- [Claude Code CLI session](../../ft/daemon/claude-cli-session.md)
- [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md)

## Summary

Bring the platform sandbox to Linux with a rootless cgroup v2 + namespaces backend, extract the
in-jail runner and the host-side egress relay into shared code, move the gRPC control channel to
AF_UNIX so it survives a network namespace, finish the daemon wiring, and expose a `sandbox` opt-in
at the CLI and web session-start surfaces.

## Background

See PRD. The sandbox is macOS-only; the daemon installs on Linux. The runner is portable but trapped
in the Darwin crate, and the host relay is triplicated. A netns isolates loopback, so the control
channel must move off TCP.

## Scope

- [ ] **Documentation**: PRD + this changeset; update sandbox architecture on wrap
- [ ] **Implementation**: runner extraction, cgroups backend, host-relay extraction, daemon wiring, flags
- [ ] **Testing**: all acceptance tests passing
- [ ] **Integration**: cross-platform egress routing verified end-to-end (no real jail)
- [ ] **Technical Debt**: production readiness gaps addressed
- [ ] **Code Quality**: builds clean, no warnings

## Technical Changes

### State A (Current)

- Only `tddy-sandbox-darwin` implements `spawn`. The daemon `spawn_sandbox_runner` is
  `#[cfg(target_os="macos")]`→darwin; everything else returns `SandboxError::Unsupported`.
- The runner (gRPC `SessionChannel` server, PTY bridge, `HTTPS_PROXY` CONNECT egress shim) lives in
  `tddy-sandbox-darwin/src/runner.rs` + `main.rs`. The gRPC server binds `127.0.0.1:{port}`; the port
  is written to the ready marker; the host dials `http://127.0.0.1:{port}` via `connect_sandbox_client`.
  The `--grpc-socket` CLI arg is dead.
- The host-side relay (reader loop + `HostPoll` poller + `spawn_tunnel` + `relay_egress_request`) is
  duplicated in `tddy-daemon/src/sandbox_session.rs` (real tool exec), `tddy-sandbox-app/src/bridge.rs`
  (real tool exec, imports `relay_egress_request` from the daemon), and
  `tddy-testing-commons/src/sandbox_session_channel.rs` (stub tool exec).
- `StartSessionRequest.sandbox` (proto field 16) exists and the daemon routes on it; no front-end sets it.

### State B (Target)

- `tddy-sandbox-runner` owns the runner (lib+bin) and the parameterized host relay. Darwin and cgroups
  both reuse the same runner binary; tests run it in-process.
- The gRPC `SessionChannel` serves over AF_UNIX on a bind-mounted path on Linux (`connect_sandbox_client_uds`),
  preserved as TCP on macOS via `cfg`. The ready marker becomes a bind sentinel on Linux.
- `tddy-sandbox-cgroups::spawn` confines the runner with rootless user namespaces, cgroup v2 limits,
  bind-mount fs write-confinement, and a no-egress network namespace.
- `spawn_sandbox_runner` dispatches darwin/cgroups by target OS; `SandboxSpec` carries memory/CPU/pids limits.
- `tddy-coder --sandbox` and the tddy-web new-session toggle set `StartSessionRequest.sandbox`.
- The host relay exists once (`run_host_relay`), consumed by daemon (`DaemonToolHandler`), app, and
  tests (`StubToolHandler`).

### Delta (What's Changing)

#### `tddy-sandbox-runner` (new)
- **Architecture**: portable runner extracted out of the Darwin crate.
- **API**: `run_sandbox_runner`, `SandboxRunnerArgs` (+ `--grpc-uds`), `connect_sandbox_client` (TCP),
  `connect_sandbox_client_uds`, `serve_grpc_over_uds`, `run_host_relay`, `HostToolHandler`, `HostRelayConfig`.
- **Deps** (new direct, all vendored): `hyper-util`, `tower`, `async-trait`, `reqwest` (rustls),
  `tddy-service`, plus the runner's existing tokio/tonic/portable-pty set.

#### `tddy-sandbox-cgroups` (new, Linux-only)
- **Architecture**: rootless jail — `unshare(CLONE_NEWUSER)` then `NEWNS|NEWNET|NEWPID|NEWIPC`,
  uid/gid mapping, cgroup v2 delegated scope writes (`memory.max`/`cpu.max`/`pids.max`), bind-mount
  RW (project/scratch/egress) + RO (toolchain/binaries), bind-mount the UDS parent at the identical
  absolute path, `lo` up in the netns, `execve` the runner with `--grpc-uds`.
- **API**: `spawn(SandboxSpec) -> Result<SandboxHandle, SandboxError>`, `detect_allow_read_paths()`.
- **Deps** (new direct, vendored): `nix`, `libc`. No `cgroups-rs` (cgroup v2 is plain file writes).
- **Hard-fail**: `Unsupported` (with remediation message) when userns/cgroup delegation unavailable —
  no silent degrade.

#### `tddy-sandbox` core
- **API**: `SandboxSpec` gains `memory_max`/`cpu_max`/`pids_max` (optional, with defaults).

#### `tddy-sandbox-darwin`
- **Refactor**: drop `runner.rs`/`main.rs` + `[[bin]]`; depend on + re-export `tddy-sandbox-runner`.

#### `tddy-daemon`
- **Integration**: `#[cfg(target_os="linux")] spawn_sandbox_runner → tddy_sandbox_cgroups::spawn`;
  UDS dial on Linux; `dial_and_bridge` delegates to `run_host_relay` via a `DaemonToolHandler` wrapping
  `tool_engine::execute_tool`; runner-argv swaps `--grpc-listen-port`→`--grpc-uds` on Linux.

#### `tddy-coder` / `tddy-web`
- **UX**: `--sandbox` CLI flag; new-session `sandbox` toggle; both set `StartSessionRequest.sandbox`.

#### `tddy-testing-commons` / `tddy-sandbox-app`
- **Refactor**: both consume `run_host_relay` (stub vs app handler); removes the duplication.

## Implementation Milestones

- [ ] M1: `tddy-sandbox-runner` crate created; runner moved; darwin re-exports; workspace builds on macOS+Linux
- [ ] M2: AF_UNIX gRPC server + `connect_sandbox_client_uds`; UDS round-trip test green
- [ ] M3: `run_host_relay` extracted; daemon/app/testing-commons collapsed onto it; existing darwin tests green
- [ ] M4: `tddy-sandbox-cgroups::spawn` implemented; `Unsupported` + cgroup-limit unit tests green
- [ ] M5: daemon Linux wiring + UDS dial; daemon sandbox-session acceptance green on the host platform
- [ ] M6: `--sandbox` CLI flag + tddy-web toggle wired; flag tests green
- [ ] M7: egress-relay-over-TLS integration test green (cross-platform, no real jail)

## Testing Plan

### Testing Strategy

**Primary test approach: Integration.** The headline risk is that egress traffic routes correctly
across the new UDS control channel and the host relay. The most valuable test runs the real runner
(in-process, no jail) and a real TLS server, and drives a real client through `HTTPS_PROXY` — an
integration test. It is cross-platform and needs no root/userns, so it runs in CI. Unit tests cover
the UDS transport, the relay's tool dispatch, the cgroups `Unsupported`/limit logic, and the CLI flag.

### Coverage Requirements

- [ ] **Happy path**: TLS round-trip through the tunnel; sandboxed session starts and persists metadata
- [ ] **Error scenarios**: cgroups `Unsupported` when userns unavailable; tunnel open failure surfaces an ack error
- [ ] **Edge cases**: relay routes `TunnelData` only to the matching tunnel id; closing a tunnel drops its sender
- [ ] **Integration points**: UDS gRPC `SessionChannel`; host relay ↔ runner; daemon spawn dispatch

## Acceptance Tests

### Integration tests (cross-package)
- [x] **Integration**: test app → in-jail `HTTPS_PROXY` shim → host relay → TLS server round-trips,
  no real jail — `routes_https_egress_from_jail_through_the_host_relay_to_a_tls_server`
  (`packages/tddy-integration-tests/tests/sandbox_egress_relay_tls.rs`) — RED: panics on the
  `run_sandbox_runner`/`run_host_relay` stubs (missing impl)

### Daemon
- [x] **Integration**: `StartSession{sandbox:true}` starts a sandboxed claude-cli session on Linux
  via the cgroups backend and persists `sandbox: true` metadata —
  `sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend`
  (`packages/tddy-daemon/tests/sandboxed_claude_cli_acceptance.rs`) — RED: `sandbox unsupported on
  linux` (cgroups backend not wired). Also fixed a pre-existing missing `use tddy_rpc::Code;` that
  broke this file's Linux compile. **Green must** re-gate `start_session_sandbox_unsupported_on_non_darwin`
  to exclude linux.

### tddy-sandbox-runner
- [x] **Integration**: `connect_sandbox_client_uds` round-trips `Echo` over an AF_UNIX socket —
  `round_trips_an_echo_over_an_af_unix_socket`
  (`packages/tddy-sandbox-runner/tests/uds_session_channel.rs`) — RED: `connect_sandbox_client_uds` stub
- [x] **Unit**: `run_host_relay` dispatches a `ToolRequest` to the injected handler and acks CONNECT
  tunnels (success + unreachable-upstream failure) — `dispatches_a_tool_request_to_the_injected_handler`,
  `dials_the_upstream_and_acks_a_connect_tunnel`, `acks_a_connect_tunnel_failure_when_the_upstream_is_unreachable`
  (`packages/tddy-sandbox-runner/tests/host_relay_dispatch.rs`, fake server in `tests/common/mod.rs`)
  — RED: `run_host_relay` stub

### tddy-sandbox-cgroups (Linux-only)
- [x] **Unit**: `write_cgroup_limits` writes `memory.max`/`cpu.max`/`pids.max`; `userns_unsupported_error`
  is `Unsupported{platform:"linux"}` naming user namespaces —
  `writes_cgroup_v2_resource_limits_to_the_scope_directory`, `unsupported_error_names_unprivileged_user_namespaces`
  (`packages/tddy-sandbox-cgroups/tests/cgroups_spawn.rs`) — RED: stubs

### tddy-web
- [x] **Component**: the Claude CLI new-session form shows a sandbox toggle and submitting with it on
  sends `StartSession{sandbox:true}` — in-memory backend (`callsTo` assertion), 2 specs
  (`packages/tddy-web/cypress/component/CreateSessionSandboxToggle.cy.tsx`) — RED: toggle/test-id absent

### CLI (already implemented — no new test)
- [x] `tddy-tools pty-relay --sandbox` already sets `StartSessionRequest.sandbox`, covered by the
  existing `build_start_session_request_sets_sandbox_flag` (`packages/tddy-tools/src/pty_relay.rs`).
  Green only needs to update its macOS-only doc comment to reflect cross-platform support.

## Green status & verification

All tests green. Two tests require unprivileged user namespaces, which this dev host blocks via
Ubuntu's AppArmor `apparmor_restrict_unprivileged_userns=1` (no root available to change it), so they
were verified in a **privileged Docker container** (`rust:latest`, `--privileged --security-opt
apparmor=unconfined`, host `/nix` mounted so the nix-built binaries resolve):

- `tddy-sandbox-runner` host_relay_dispatch (3) + uds_session_channel (1) — host ✅
- `tddy-sandbox-cgroups` cgroups_spawn unit (2) — host ✅; `jail_smoke` self-skips on host, **verified
  in Docker** (`uid=0` via the userns map; only `lo` in the netns → no direct egress) ✅
- `tddy-integration-tests` egress-relay-over-TLS (1) — host ✅
- `tddy-sandbox-darwin` runner behavior acceptance (3, via the collapsed `SandboxSessionChannelHost`) — host ✅
- `tddy-daemon` `sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend` — **verified in Docker**
  (runner spawned in the jail, served gRPC over UDS, daemon dialed via `connect_sandbox_client_uds`,
  metadata persisted) ✅
- `tddy-web` sandbox toggle (2) — ✅; `tddy-tools` `build_start_session_request_sets_sandbox_flag` — ✅
- `cargo clippy -p tddy-sandbox-runner -p tddy-sandbox-cgroups -- -D warnings` clean; `cargo fmt` applied.

## Technical Debt & Production Readiness

- [ ] **fs write-confinement (Linux)**: the cgroups jail isolates network + uids + resources but does
  not yet `pivot_root` into a minimal read-only root (marked `FIXME(fs-confinement)` in
  `tddy_sandbox_cgroups::spawn`). The netns egress guarantee and cgroup limits are in place; bind-mount
  RO-root confinement is the remaining hardening to reach Seatbelt parity.
- [ ] cgroup limits use built-in defaults (2 GiB / 1 CPU / 512 pids); wiring them to config /
  `SandboxSpec` is a follow-up. cgroup application is best-effort (degrades to no-limits without
  cgroup v2 delegation; netns isolation still applies).
- [ ] cgroup v2 delegation path discovery is environment-dependent; document required systemd `Delegate=yes`.
- [ ] Linux `detect_allow_read_paths` is a first cut (toolchain probing) — wired for the future
  pivot_root bind set; not yet consumed.
- [ ] macOS control channel left on TCP; unifying on UDS is a deferred follow-up.
- [ ] Production daemon runs as a root systemd service, where unprivileged-userns restrictions don't
  apply; on unprivileged hosts the spawn fails fast with a remediation message (no silent fallback).
- [ ] `cgroups::spawn` `pre_exec` performs small `std::fs`/`nix` allocations in the forked child;
  acceptable in practice (verified under the multi-threaded daemon test in Docker) but raw-syscall
  writes would remove the residual post-`fork` allocation risk.

### Validation Results

#### validate-changes (2026-06-28)

**Critical: 0 · Warning: 0 (fixed) · Info: documented**

- **[fixed]** `tddy-daemon/src/sandbox_session.rs`: `connect_sandbox_client` (TCP dialer) was dead on
  Linux → `cfg(not(target_os = "linux"))`-gated (would have failed `clippy -D warnings`).
- **[fixed]** `tddy-sandbox-cgroups/src/lib.rs`: cgroup setup was best-effort (a fallback without
  consent, contrary to the agreed design) → now hard-fails with `Unsupported` when the cgroup v2
  subtree isn't writable; the process is moved into its scope or the spawn is aborted. Per-controller
  limit writes remain best-effort (the process is already scoped).
- **[info]** `unsafe` blocks (`pre_exec`, `bring_loopback_up`) carry SAFETY rationale; new-crate
  `clippy -D warnings` clean; `cargo fmt` applied. Tests are fluent-style with real assertions;
  `jail_smoke` uses a host-capability skip-guard (documented), not assertion branching.

## Decisions & Trade-offs

- **AF_UNIX control channel (not veth/slirp4netns)**: UDS is filesystem-namespaced so it crosses the
  netns boundary with no networking machinery; already proven in-repo by the tool-IPC `UnixListener`.
  macOS stays on TCP behind `cfg` to avoid regressing Seatbelt acceptance tests.
- **Runner home = new `tddy-sandbox-runner` crate** (not `tddy-sandbox` core): keeps the heavy
  tonic/tokio/reqwest deps out of the core crate while letting darwin, cgroups, daemon, app, and tests
  all share it.
- **Host relay home = `tddy-sandbox-runner::host_relay`** parameterized by `HostToolHandler`: one
  implementation, three injection points (daemon real exec, app exec, test stub).
- **Rootless + hard-fail**: no root requirement; if the host can't provide unprivileged userns or
  cgroup v2 delegation, fail fast rather than silently running unconfined (CLAUDE.md no-fallback rule).
- **cgroup v2 via direct file writes**: avoids a `cgroups-rs` dependency for three `*.max` files.

## References

- PRD: [PRD-2026-06-28-linux-cgroups-sandbox.md](../../ft/daemon/1-WIP/PRD-2026-06-28-linux-cgroups-sandbox.md)
- Prior changeset: Sandbox egress — HTTPS_PROXY CONNECT tunnel over SessionChannel (2026-06-28)
- Plan: `/var/tddy/.claude/plans/i-d-like-to-add-scalable-wozniak.md`
