# tddy-sandbox architecture

Platform-agnostic sandbox abstraction for confined agent sessions.

## Crates

| Crate | Role |
|-------|------|
| `tddy-sandbox` | `SandboxSpec`, `SandboxHandle`, `SandboxError`, `SandboxContextDir`, spawn facade |
| `tddy-sandbox-runner` | Platform-agnostic in-jail runner (gRPC `SessionChannel` server + `claude` PTY + `HTTPS_PROXY` CONNECT egress shim) **and** the host-side relay (`run_host_relay`). Ships the `tddy-sandbox-runner` binary. |
| `tddy-sandbox-darwin` | macOS Seatbelt: SBPL template render + `sandbox-exec` spawn (re-exports the runner) |
| `tddy-sandbox-cgroups` | Linux rootless jail: unprivileged user namespace + network namespace (loopback-only) + private mount namespace + cgroup v2 limits |

## Platform spawn

Each platform crate exposes `spawn(SandboxSpec) -> Result<SandboxHandle, SandboxError>`; the daemon's
`spawn_sandbox_runner` dispatches by target OS (macOS → `tddy-sandbox-darwin`, Linux →
`tddy-sandbox-cgroups`, else `Unsupported`). The **runner** itself (`tddy-sandbox-runner`) is shared:
both backends launch the same `tddy-sandbox-runner` binary inside their jail, and tests run it
in-process. The **host-side relay** (`run_host_relay`, parameterized by a `HostToolHandler`) is the
single implementation consumed by the daemon (real `tool_engine` exec), the standalone app, and tests
(stub handler) — it answers `HostPoll`, relays CONNECT tunnels (host owns the outbound socket; TLS
stays end-to-end), and forwards PTY output.

### Control-channel transport

| Platform | gRPC `SessionChannel` transport |
|----------|----------------------------------|
| macOS | loopback **TCP** (port written to the ready marker; Seatbelt allows loopback) |
| Linux | **AF_UNIX** (`--grpc-uds`, `connect_sandbox_client_uds`) on a shared-filesystem path — survives the jail's network namespace, where loopback TCP cannot |

### Linux cgroups jail

`tddy-sandbox-cgroups::spawn` confines the runner via `Command::pre_exec`: `unshare(CLONE_NEWUSER)`
with the caller mapped to root-in-ns, then `NEWNS | NEWNET`, a private root mount, and `lo` brought up
(no other interfaces → no direct egress, so outbound must use the in-jail `HTTPS_PROXY`). The process
is placed in a cgroup v2 scope with memory/CPU/pids limits. It **fails fast** with
`SandboxError::Unsupported` when unprivileged user namespaces are unavailable (e.g. Ubuntu AppArmor
`apparmor_restrict_unprivileged_userns=1`) or the cgroup v2 subtree isn't writable — never a silent
unconfined fallback. The production daemon runs as a root systemd service, where the userns
restriction does not apply. *(Follow-up: `pivot_root` read-only-root filesystem write-confinement; the
network-namespace egress guarantee and cgroup limits are in place.)*

## SandboxSpec

| Field | Purpose |
|-------|---------|
| `project_root` | Read-only context dir inside the jail |
| `scratch_dir` | Writable per-session scratch (`.work/`) |
| `egress_dir` | Logs, spawn manifest, diagnostics |
| `allow_read_paths` | Extra toolchain paths for SBPL read allow-list |
| `command` | argv for the jailed process (typically `tddy-tools sandbox-runner …`) |
| `env` | Clean environment (`HOME`/`TMPDIR` redirected into scratch) |
| `profile_path` | Rendered SBPL file path |
| `loopback_allow_ports` | Loopback TCP ports allowed in SBPL (gRPC + egress shim) |
| `ipc_socket` | Short out-of-tree AF_UNIX path for tool IPC |

## Context dir

`SandboxContextDir` copies project guidance files (`CLAUDE.md`, `AGENTS.md`, skills) into a read-only tree and appends `REMOTE_APPENDIX` (same notice as remote-codebase mode). The in-jail `claude` working directory is this tree; the host worktree is reached only via MCP tools.

## Unsupported platforms

macOS (Seatbelt) and Linux (rootless cgroups) are supported. On other platforms — and on Linux/macOS
hosts that can't provide the required isolation (no unprivileged user namespaces, no writable cgroup v2
subtree) — `spawn` returns `SandboxError::Unsupported`. Callers map this to gRPC `failed_precondition`
— no fallback spawn path.

## Consumers

- **`tddy-daemon`**: `start_sandboxed_claude_cli_session`, `sandbox_session.rs`
- **`tddy-tools`**: `sandbox-runner` subcommand (in-jail entrypoint)

## See also

- [tddy-sandbox-darwin troubleshooting](../tddy-sandbox-darwin/docs/troubleshooting.md)
- Feature: [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md)
- Feature: [remote-codebase-mode.md](../../../docs/ft/daemon/remote-codebase-mode.md)
