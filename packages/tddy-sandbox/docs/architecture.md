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

**Session-end signaling:** when the jailed PTY command exits, the runner records the exit code but
never pushes `SessionEnded` immediately on the raw outbound stream — it is always deferred to the next
`HostPoll` reply, after any queued `terminal_backlog` has been drained. The host-side reader processes
frames strictly in order and stops as soon as it sees `SessionEnded`, so delivering it ahead of
still-queued terminal output would drop the tail of the PTY's output and, if no `HostPoll` had arrived
yet, could stall the sandboxed process waiting on a stream that never closes.

### Control-channel transport

**stdio is the transport for the daemon's real sandboxed-session lifecycle** (macOS proven
end-to-end through a real Seatbelt jail; Linux piping added in `tddy-sandbox-cgroups::spawn_plan`
but not runtime-verified — no Linux dev box). `--stdio` serves `SandboxService`
(`Echo`/`EchoStream`/`SessionChannel`) over the jailed process's own piped stdin/stdout instead of
a gRPC socket, via `tddy-rpc`/`tddy-stdio`. `tddy_sandbox_darwin::spawn_plan` and
`tddy_sandbox_cgroups::spawn_plan` both pipe stdin/stdout (instead of redirecting stdout to an
egress log, on macOS) when `--stdio` is present in the command; `SandboxHandle::take_stdio()`
exposes the piped (blocking) `std::process::ChildStdin`/`ChildStdout`;
`tddy_daemon::sandbox_session::bridge_sandbox_stdio` converts them to async via
`tokio::net::unix::pipe` and hosts an `RpcService` endpoint over them. `run_host_relay` is
transport-agnostic via the `SessionChannelClient` trait (implemented for both the tonic
`SandboxClient` and `StdioSandboxClient`) — its actual relay logic (PTY/tool/tunnel/egress)
needed no changes, since it only ever touches plain `SessionFrame` structs, the same Rust type on
both transports (`sandbox.proto`'s message types are `extern_path`-unified across the tonic and
RpcService codegen passes). `connection_service.rs`'s spawn/dial orchestration and
`sandbox_session::dial_and_bridge` dial exclusively over stdio now — the
`--grpc-socket`/`--grpc-listen-port`/`--grpc-uds` flags and the port/ready-marker handshake for
this call site were deleted outright (no dual-path fallback, per this repo's convention).

The sandbox-runner's own tonic gRPC server is retained as an independent transport (unaffected by
the above) for `tddy-sandbox-app`'s standalone demo path and `sandbox_action.rs`'s separate
generic-action-execution flow:

| Platform | gRPC `SessionChannel` transport (`tddy-sandbox-app`, `sandbox_action.rs`) |
|----------|----------------------------------|
| macOS | loopback **TCP** (port written to the ready marker; Seatbelt allows loopback) |
| Linux | **AF_UNIX** (`--grpc-uds`, `connect_sandbox_client_uds`) on a shared-filesystem path — survives the jail's network namespace, where loopback TCP cannot |

### Linux cgroups jail

`tddy-sandbox-cgroups::spawn` confines the runner via `Command::pre_exec`: `unshare(CLONE_NEWUSER)`
with the caller mapped to root-in-ns, then `NEWNS | NEWNET`, a private root mount, and `lo` brought up
(no other interfaces → no direct egress, so outbound must use the in-jail `HTTPS_PROXY`). The process
is placed in a cgroup v2 scope with memory/CPU/pids limits. It **fails fast** with
`SandboxError::Unsupported` when unprivileged user namespaces are unavailable or the cgroup v2 subtree
isn't writable — never a silent unconfined fallback.

The production daemon runs as an **unprivileged systemd service** (`User=tddy`) with two grants that
`./install` provisions:

- **`Delegate=yes`** — hands the service a writable cgroup v2 subtree. The backend derives the
  delegated base from `/proc/self/cgroup` at runtime (config-overridable via `sandbox_cgroup:`; never
  hardcoded), relocates the daemon's own process into a `supervisor` leaf to satisfy cgroup v2's
  no-internal-processes rule, enables `memory cpu pids` in the base's `subtree_control`, then creates
  per-session `tddy-<name>-<seq>.scope` children.
- **An AppArmor profile** granting the daemon binary unprivileged user namespaces. On Ubuntu 24.04
  `apparmor_restrict_unprivileged_userns=1` gates the userns *mapping* writes (not `unshare` itself),
  so the precondition check is a **functional probe** — it forks a child that performs the real
  `unshare` + uid/gid mapping and reports success — rather than a sysctl read, which cannot see a
  per-binary grant. (Running as root also works and short-circuits both requirements.)

*(Follow-up: `pivot_root` read-only-root filesystem write-confinement; the network-namespace egress
guarantee and cgroup limits are in place.)*

### Standalone `tddy-sandbox-app` on Linux (daemon-assisted)

The standalone launcher spawns the jail in-process only on macOS (Seatbelt). On Linux an unprivileged
app cannot place its own child in a limited cgroup scope (cgroup v2 **delegation containment** — the
common ancestor of its shell scope and any writable delegated subtree is the root cgroup, which it
can't write). So on Linux `tddy-sandbox-app` **delegates to a running `tddy-daemon`**: it connects the
daemon's Unix socket over tonic gRPC, `MintLocalToken` (SO_PEERCRED peer-trust — the peer uid's mapped
os_user → a signed access token), `StartSession` carrying its `repo_path`/`model`/`permission_mode`/
`codebase_mode`/`claude_args`, and PTY-proxies the session over `StreamSessionTerminalIO`. The daemon
serves `ConnectionService` over the UDS via a hand-written tonic adapter over the existing impl,
alongside its HTTP/LiveKit transports; `repo_path` is used directly as the worktree and is never
daemon-removed (`.worktrees` guard).

> **Status — unverified (draft PR #291, stacked on the unprivileged-cgroups work):** this Linux path
> is implemented and unit/transport-integration-tested, but has **not** been run end-to-end (no
> automated test drives a live daemon; the on-host run also depends on the daemon's own cgroups
> sandbox, which has open issues), and the macOS build was not re-verified after the `#[cfg]` split.

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

## Sandbox builder (explicit plan)

`SandboxBuilder` assembles a `SandboxPlan` — an explicit, auditable description of everything a jail
may touch. **Nothing is read, copied, symlinked, mounted, or exposed unless a caller names it**:
`build()` is pure (no filesystem access, no subprocess detection) and carries no implicit defaults.
`SandboxPlan` composes the legacy `SandboxSpec` (so spec-only code keeps working) plus typed
allow-lists:

| Sub-spec | Meaning |
|----------|---------|
| `ReadSpec { host, jail, kind, exec, reason }` | A read grant. `kind` is `Subpath`/`Literal`/`Regex` (SBPL needs a regex rule for `/dev/ttys[0-9]+`). macOS → SBPL `file-read*` rule; Linux → read-only bind mount. |
| `MountSpec { host, jail, writable }` | A host directory made available in the jail. macOS grants read (+write+exec when `writable`) at the real path (no remap); Linux bind-mounts it (rw when `writable`). |
| `CopySpec { src, dest, optional, mode }` | A file copied into the writable jail tree before spawn. |
| `SymlinkSpec { link, target }` | A symlink created inside the jail tree. |
| `SecretSpec { env_name, source }` | Out-of-band secret: written to a `0600` `scratch/.secrets/<NAME>` file referenced by `TDDY_SECRET_<NAME>`; the value never enters the broad env or `sandbox-exec` argv. The runner reads it and sets it on the inner `claude` child only, then unlinks. |
| `PolicySpec` | dynamic-code-generation, process-fork, mach-lookup, sysctl-read, pseudo-tty, `process-exec*` paths. |
| `NetworkSpec { loopback_allow_ports, allow_oauth_inbound }` | Loopback TCP allows; `allow_oauth_inbound` permits the ephemeral OAuth callback listener. |
| `ResourceLimits` | cgroup v2 memory/cpu/pids (Linux). |

**Strict reads (macOS):** `render_plan(&SandboxPlan)` emits an explicit read allow-list (always
including the `(literal "/")` dyld-cache root) and **no blanket `(allow file-read*)`**. The audited
read set lives in one place — `claude_spawn::system_baseline_reads` / `claude_required_reads` (which
also folds in toolchain dirs and the `claude` binary's `otool -L` deps). Materialization of copies,
symlinks, and secrets is shared (`materialize.rs`). Backends consume the plan via
`tddy_sandbox_darwin::{render_plan, spawn_plan}` and `tddy_sandbox_cgroups::{plan_to_bind_mounts,
spawn_plan}`; the daemon's `build_sandbox_plan` builds it from the Claude recipe + per-spawn `mounts`.

**Env:** `default_runner_env` (shared) produces the clean runner env (`HOME`/`TMPDIR`/`PATH`/… plus
`CLAUDE_CODE_TMPDIR` so Claude's `/tmp/claude-$UID` runtime dir lands in writable scratch).

## Context dir

`SandboxContextDir` copies project guidance files (`CLAUDE.md`, `AGENTS.md`, skills) into a read-only tree and appends `SANDBOX_REMOTE_APPENDIX` (same "Managed Codebase" notice as managed-codebase mode). In managed-codebase mode the host worktree is reached only via MCP tools — optionally with one or more [specialized agents](../../../docs/ft/coder/specialized-subagents.md) wired in, if `tddy-sandbox-app --specialized-agent` was given. Alternatively a caller may mount the repo into the jail (`MountSpec`, e.g. `tddy-sandbox-app --repo`, read-write) and set the runner's `--cwd` so `claude` starts in the real project tree.

`sandbox_remote_appendix(replacements)` renders each agent's tool replacements into the appendix: replaced tools get a per-agent delegation clause (`subagent_new_session`/`subagent_prompt`), except a replaced **`Shell`**, which gets its own paragraph describing the session-action surface (`request_action` authored by the replacing agent, `list_actions`, `invoke_action`) — see [no-bash-mode.md](../../../docs/ft/coder/no-bash-mode.md).

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
