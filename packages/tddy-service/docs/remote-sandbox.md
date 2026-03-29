# Remote sandbox service

## Overview

**`remote_sandbox.proto`** defines **`remote_sandbox.v1.RemoteSandboxService`**: unary RPCs for non-interactive execution, object put/stat, a fixed checksum helper for LiveKit smoke tests, and **OpenRsyncSession** for rsync server bridging.

Code generation uses **`prost-build`** with **`tddy_codegen::TddyServiceGenerator`** (async trait + **`RpcService`** server). Generated types and traits live under **`crate::proto::remote_sandbox_v1`**.

## Implementation

- **`RemoteSandboxServiceImpl`** holds an **`Arc<SandboxRegistry>`** mapping logical session ids to temp directories under the system temp dir (**`tddy-remote-sandbox/`**).
- **`sandbox_relative_path`** (module **`sandbox_path`**) canonicalizes user-supplied relative paths: no absolute paths, no **`..`**, UTF-8 path segments only. Daemon VFS helpers and **`tddy-remote`** reuse this module.
- **`ExecNonInteractive`:** Spawns **`tokio::process::Command`** with **`kill_on_drop`**, returns exit code and stdout; stdout size is capped (resource limit).
- **`OpenRsyncSession`:** Binds **`127.0.0.1:0`**, returns host/port; accepts one TCP connection and runs a blocking **`rsync --server`** bridge with stdin/stdout on duplicated socket fds (Unix). **`--mkpath`** is injected after **`--server`** when the client omits it so nested destination paths succeed.
- **Session id:** Empty **`session`** fields map to the **`default`** bucket for exec and for registry lookup where applicable.

## LiveKit

The same **`RemoteSandboxService`** implementation is reachable over LiveKit when the **`tddy-livekit`** bridge dispatches to **`RpcService`**; **`ExecChecksum`** supports integration smoke tests.

## Tests

**`tests/remote_sandbox_stub_red.rs`** exercises handler stubs against the generated server wiring.

## Related

- Proto: **`packages/tddy-service/proto/remote_sandbox.proto`**
- Feature: [docs/ft/daemon/remote-sandbox.md](../../../docs/ft/daemon/remote-sandbox.md)
