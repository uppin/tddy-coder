# Linux cgroups sandbox + cross-platform sandbox sessions — PRD

**Date**: 2026-06-28
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Claude Code CLI session](../claude-cli-session.md) — sandboxed claude-cli
  sessions become available on Linux (not just macOS); a `sandbox` flag is exposed at the
  session-start surfaces (CLI + web).
- **Related Feature**: [tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md)
  — the platform sandbox gains a second backend (rootless cgroups + namespaces) alongside the
  Darwin Seatbelt backend; the in-jail runner and the host-side egress relay are extracted into
  shared code.

## Summary

The sandbox that confines a claude-cli session is **macOS-only** today (Darwin Seatbelt). This PRD
brings it to Linux using **rootless cgroup v2 + user/mount/network/pid namespaces**, finishes the
daemon wiring so a session can run sandboxed on either platform, exposes a `sandbox` opt-in at the
`tddy-coder` CLI and the tddy-web new-session form, and adds a cross-platform integration test that
proves the sandbox→host egress routing works end-to-end without spawning a real jail.

## Background

`tddy-sandbox-darwin` is the only platform implementation. The daemon's `spawn_sandbox_runner` is
`#[cfg(target_os="macos")]`, returning `Unsupported` everywhere else, so the whole sandboxed-session
feature is unavailable on the Linux hosts where the daemon installs as a systemd service. The in-jail
runner (gRPC `SessionChannel` server + PTY bridge + `HTTPS_PROXY` CONNECT egress shim) is portable
tokio/tonic code but lives inside the Darwin crate, and the host-side relay that turns CONNECT
tunnels into real outbound sockets is copy-pasted across the daemon, the standalone app, and the test
harness. Adding a Linux backend is the moment to extract both into shared code.

A network namespace gives the jail a **private loopback**, which breaks the current host→runner gRPC
dial over `http://127.0.0.1:{port}`. The control channel therefore moves to an **AF_UNIX socket on a
bind-mounted path**, which crosses the netns boundary because it is filesystem-namespaced.

## Proposed Changes

### What's Changing

- **New Linux sandbox backend** (`tddy-sandbox-cgroups`): rootless `spawn(SandboxSpec)` using
  unprivileged user namespaces + `NEWNS|NEWNET|NEWPID|NEWIPC`, cgroup v2 resource limits
  (memory/CPU/pids), bind-mount filesystem write-confinement, and a no-egress network namespace that
  forces all outbound traffic through the in-jail `HTTPS_PROXY` — mirroring Darwin's deny-network +
  write-confinement guarantees.
- **Runner extracted** to a shared `tddy-sandbox-runner` crate (lib + binary) reused by both
  platform backends and runnable in-process by tests.
- **gRPC control channel over AF_UNIX** (Linux) so it survives the network namespace; the macOS TCP
  path is preserved behind `cfg`.
- **Host-side egress relay extracted** into shared code parameterized by a tool-execution handler, so
  the daemon, the standalone app, and tests stop duplicating it.
- **Daemon wiring**: `spawn_sandbox_runner` dispatches to the cgroups backend on Linux; new cgroup
  limit fields on `SandboxSpec`.
- **`sandbox` flag** exposed on the `tddy-coder` CLI (`--sandbox`) and the tddy-web new-session form;
  both set the existing `StartSessionRequest.sandbox` proto field.

### What's Staying the Same

- The `SandboxSpec` → `SandboxHandle` contract and the daemon's existing sandboxed-session lifecycle
  (worktree setup, context dir, ready marker, metadata persistence) are unchanged in shape.
- The macOS Seatbelt backend keeps its TCP control channel and SBPL profile.
- The CONNECT-tunnel egress wire protocol (`TunnelOpen/Ack/Data/Close`) is unchanged; TLS stays
  end-to-end and the host remains a dumb byte relay.

## Impact Analysis

### Technical Impact

- New crates `tddy-sandbox-runner`, `tddy-sandbox-cgroups`; `tddy-sandbox-darwin` slimmed to
  `spawn`/`profile` and re-exports the runner.
- New direct dependencies (all already vendored in `Cargo.lock`): `nix`, `libc` (cgroups crate);
  `hyper-util`, `tower`, `async-trait` (runner UDS + relay); `tokio-rustls`, `rcgen`
  (integration-test TLS server, dev-only).
- gRPC transport change (TCP→UDS on Linux) touches the runner, the daemon dial, and the runner-argv
  builder; cfg-guarded to avoid Darwin regressions.
- No performance regression expected; UDS is local IPC.

### User Impact

- Linux users can opt a claude-cli session into sandbox mode; previously this silently failed with
  `failed_precondition`.
- New `--sandbox` CLI flag and a web toggle; default is unsandboxed (no behavior change for existing
  flows).
- Sandbox availability still depends on the host: if unprivileged user namespaces or cgroup v2
  delegation are unavailable, `StartSession{sandbox:true}` fails fast with a clear remediation
  message (no silent fallback to an unconfined session).

## Implementation Plan

1. Extract the runner into `tddy-sandbox-runner` (lib+bin); add the AF_UNIX server + client.
2. Extract the host relay into `tddy-sandbox-runner::host_relay` parameterized by a tool handler;
   collapse the daemon, app, and test copies onto it.
3. Add cgroup limit fields to `SandboxSpec`.
4. Build `tddy-sandbox-cgroups` (`spawn` via rootless userns + cgroup v2 + bind mounts + netns).
5. Wire the daemon's Linux `spawn_sandbox_runner` arm + UDS dial.
6. Expose the `sandbox` flag in `tddy-coder` and the tddy-web new-session form.
7. Testing per the changeset (acceptance + unit/integration), red → green.

## Acceptance Criteria

- [ ] Egress routing works test-app → in-jail `HTTPS_PROXY` shim → host relay → TLS server, proven by
  a cross-platform test that runs the runner in-process (no real jail)
  ([tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md))
- [ ] `StartSession{sandbox:true}` starts a sandboxed claude-cli session on the host platform and
  persists `sandbox: true` metadata ([Claude Code CLI session](../claude-cli-session.md))
- [ ] On Linux, `tddy-sandbox-cgroups::spawn` returns `Unsupported` with a clear message when
  unprivileged user namespaces are unavailable, and applies cgroup v2 limits when available
- [ ] The gRPC `SessionChannel` round-trips over an AF_UNIX socket
- [ ] `tddy-coder --sandbox` and the tddy-web new-session toggle set `StartSessionRequest.sandbox`
- [ ] The host-side relay exists in exactly one place, consumed by daemon, app, and tests
- [ ] Tests passing for all affected features

## References

### Affected Features (Complete List)
- [Claude Code CLI session](../claude-cli-session.md) — sandboxed sessions on Linux; `sandbox` flag surfaces
- [tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md) — second backend; runner + host-relay extraction; UDS control channel

### Related Documentation
- Changeset: [docs/dev/1-WIP/2026-06-28-linux-cgroups-sandbox.md](../../../dev/1-WIP/2026-06-28-linux-cgroups-sandbox.md)
- Prior changeset: Sandbox egress — HTTPS_PROXY CONNECT tunnel over SessionChannel (2026-06-28)
