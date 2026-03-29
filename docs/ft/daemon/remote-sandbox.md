# Remote sandbox (SSH-shaped shell and file access)

**Status:** Stable

## Purpose

Operators and automation use **per-connection sandboxes** on a running **tddy-daemon**: isolated temp directories, a configurable shell environment, a **virtual filesystem** rooted in that directory, and **bash** (or bash-compatible) for interactive and non-interactive commands. Access is **RPC-shaped** (not raw host SSH): **Connect** on the daemon HTTP stack and **LiveKit** data channels reuse the same logical service.

## Capabilities

- **Shell-like execution:** Non-interactive remote commands with exit codes; optional PTY paths for interactive use via the **`tddy-remote`** client.
- **File operations:** Write, stat, and path-safe access within the sandbox root — sufficient for **rsync** push/pull when combined with the rsync session bridge.
- **Rsync compatibility:** **`tddy-remote`** supports **`rsync --rsh`** (external subcommand) mode: the daemon exposes **`OpenRsyncSession`**, which binds a loopback TCP port and bridges the accepted socket to **`rsync --server`** with stdin/stdout on duplicated file descriptors.
- **Isolation:** Each logical **session** id maps to a distinct on-disk sandbox root for concurrent clients.

## Out of scope

- **tddy-workflow** orchestration, recipes, and session lifecycle hooks are **not** dependencies of the remote client crate; workflow integration is a separate product decision.

## Components

| Piece | Role |
|--------|------|
| **`remote_sandbox.v1.RemoteSandboxService`** | Protobuf service: exec, VFS, checksum smoke, rsync session. |
| **tddy-daemon** | Registers the **`RemoteSandboxServiceServer`** with the multi-RPC router (Connect + LiveKit bridge). |
| **tddy-remote** | CLI: list authorities, exec, shell, **`rsync` RSH**; YAML config for authority ids and Connect base URLs. |
| **Shared path rules** | **`tddy_service::sandbox_path`** normalizes relative paths; daemon and client helpers align on traversal rejection. |

## Configuration (client)

YAML lists **`authorities`** with **`id`** and **`connect_base_url`**. Optional **`default_authority`** selects the authority when the host argument is empty or when no authority id matches (fallback).

## Security and operations

- **Transport:** Connect unary RPCs use the same HTTP surface as other daemon RPCs; **`RequestMetadata`** on Connect does not carry bearer cookies by default — deployments that expose the daemon beyond a trusted network must treat remote sandbox RPCs as a privileged surface and layer network policy and authentication accordingly.
- **Resource limits:** Non-interactive exec responses cap captured **stdout** size (see service implementation).

## References

- **Daemon technical:** [packages/tddy-daemon/docs/remote-sandbox.md](../../../packages/tddy-daemon/docs/remote-sandbox.md)
- **Service / proto:** [packages/tddy-service/docs/remote-sandbox.md](../../../packages/tddy-service/docs/remote-sandbox.md)
- **Client:** [packages/tddy-remote/docs/remote-sandbox-client.md](../../../packages/tddy-remote/docs/remote-sandbox-client.md)
- **Transport overview:** [gRPC Remote Control](../coder/grpc-remote-control.md)
