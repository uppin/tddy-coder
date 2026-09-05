# 2026-07-01 — `TddyRemote` and `SandboxService` unified across tonic and stdio transports

**Type:** Feature

new `build.rs` pass dual-codegens `TddyRemote` as an `RpcService` (`crate::proto::remote`, `extern_path`-reusing the existing tonic message types from `crate::gen`), the same pattern `terminal.proto` already used; `sandbox.proto`'s own message types are now `extern_path`-unified across its tonic (`tonic_sandbox`) and RpcService (`proto::sandbox`) codegen passes (previously generated independently, an untracked pre-existing gap this surfaced). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-service, tddy-coder, tddy-sandbox-runner)
