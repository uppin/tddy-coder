# tddy-sandbox architecture

Platform-agnostic sandbox abstraction for confined agent sessions.

## Crates

| Crate | Role |
|-------|------|
| `tddy-sandbox` | `SandboxSpec`, `SandboxHandle`, `SandboxError`, `SandboxContextDir`, spawn facade |
| `tddy-sandbox-darwin` | macOS Seatbelt: SBPL template render + `sandbox-exec` spawn |

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

On non-macOS, `spawn` returns `SandboxError::Unsupported`. Callers map this to gRPC `failed_precondition` — no fallback spawn path.

## Consumers

- **`tddy-daemon`**: `start_sandboxed_claude_cli_session`, `sandbox_session.rs`
- **`tddy-tools`**: `sandbox-runner` subcommand (in-jail entrypoint)

## See also

- [tddy-sandbox-darwin troubleshooting](../tddy-sandbox-darwin/docs/troubleshooting.md)
- Feature: [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md)
- Feature: [remote-codebase-mode.md](../../../docs/ft/daemon/remote-codebase-mode.md)
