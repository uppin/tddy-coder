# Remote sandbox (daemon)

## Role

**tddy-daemon** registers **`remote_sandbox.v1.RemoteSandboxService`** with the process-wide RPC registry alongside auth, token, and (when configured) connection services. The implementation lives in **`tddy-service`** (`RemoteSandboxServiceImpl`); this package supplies **VFS path helpers** under **`crate::remote_sandbox::vfs`** and integration tests that drive a real daemon over **Connect**.

## Wiring

- **`main.rs`:** Constructs **`RemoteSandboxServiceImpl`**, wraps **`RemoteSandboxServiceServer`**, pushes a **`ServiceEntry`** whose name matches the generated **`RemoteSandboxServiceServer::NAME`** constant.
- **HTTP:** The **`tddy-coder`** web stack mounts Connect handlers at **`/rpc/{service}/{method}`**; unary calls use protobuf payloads as documented in **`tddy-connectrpc`**.

## Path helpers

**`remote_sandbox::vfs::ensure_relative_under_root`** delegates to **`tddy_service::sandbox_path::sandbox_relative_path`** and maps success to **`Ok(())`**. Integration tests assert acceptance of safe relative paths and rejection of **`..`** segments.

## Tests

| Test binary | Focus |
|-------------|--------|
| **`tests/remote_sandbox_connect_integration.rs`** | Connect POST to **`ExecNonInteractive`**, **`PutObject`**, **`StatObject`**, concurrent session isolation, rsync push/pull checksums when **`rsync`** is installed. |
| **`tests/remote_sandbox_vfs_rules.rs`** | VFS path rules for daemon helpers. |

## Related

- Feature: [docs/ft/daemon/remote-sandbox.md](../../../docs/ft/daemon/remote-sandbox.md)
- Service implementation: [../../tddy-service/docs/remote-sandbox.md](../../tddy-service/docs/remote-sandbox.md)
