# 2026-10-17 — The warm index's rust-analyzer and toolchain are configuration

A host with no developer shell on its `PATH` — Tddy Desktop launched from the Dock — names the rust-analyzer the warm
index runs (`index_daemon.rust_analyzer_path`) and the environment it loads workspaces with (`index_daemon.env`).
A rust-analyzer that cannot be found is refused as `FailedPrecondition` ("language server not found"), never reported
as a server that exited. See [warm-code-intelligence-daemon.md](../warm-code-intelligence-daemon.md#lifecycle).
