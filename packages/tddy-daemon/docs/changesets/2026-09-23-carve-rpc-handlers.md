# 2026-09-23 — The runtime installs `RpcHandlers`; a guard dials the assembled socket

**Type:** Architecture

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

`runtime::build` builds the session host with every `with_*`, then calls
`tddy_daemon_rpc::RpcHandlers::install(host)` and takes the Project, Catalog, ExecTool and PR-stack
local-socket services and transport entries from the returned handlers.
`BinaryLocalSocketServices` names `ProjectServiceImpl<ProjectRpcHandler>`,
`CatalogServiceImpl<CatalogRpcHandler>`, `ExecToolServiceImpl<ExecToolRpcHandler>` and
`PrStackServiceImpl<PrStackRpcHandler>`. Added the `tddy-daemon-rpc` dependency.

Added `tests/local_socket_family_wiring_acceptance.rs` (registered in `test_placement.rs`): it builds
and starts the binary runtime, dials the Unix socket it assembled, and requires one call per family to
be answered by that family's handler rather than `Unimplemented`. Green before and after the rewiring,
by design. `local_socket_reachability_acceptance.rs` is unmodified. `local_token_uds.rs`,
`staging_forwarding_acceptance.rs`, `relay_e2e`, `relay_idle_wired`,
`remote_managed_worktree_cross_host`, `session_agent_remote`, `session_attach_cross_host`,
`session_room_cross_host` and `split_session_resume` build their families from `RpcHandlers` /
`tddy_daemon_rpc::test_util`.

Production lines 3,321 → 3,313. Code issues: `oversized-file-runtime` 1,521 → 1,513 (still open);
`complexity-runtime-build` unchanged at 833 lines — the handler assembly lives in `RpcHandlers`, not
inline in `build()`.
