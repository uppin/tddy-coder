# 2026-09-23 — Four RPC families leave; the host reaches them through `DaemonRpcFamilies`

**Type:** Architecture

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

`DaemonSessionHost` no longer implements `ProjectHandler`, `ProjectService`, `CatalogHandler`,
`ExecToolHandler` or `PrStackHandler`. `svc_project_ports.rs`, `project_coordinate_handlers.rs`,
`svc_catalog_ports.rs`, `svc_exec_tool_ports.rs`, `svc_pr_stack_ports.rs`, `svc_family_entries.rs`
and the PR-stack half of `svc_pr_status_for_caller.rs` moved to `tddy-daemon-rpc`, with
`TestDaemon`'s four family impls and 25 suites. The host lost `project_service`, `project_entry`,
`catalog_rpc_service`, `catalog_entry`, `exec_tool_rpc_service`, `exec_tool_entry`,
`pr_stack_rpc_service` and `pr_stack_entry`.

Added:

- **`DaemonRpcFamilies`** (`rpc_families.rs`): `pr_stack_handler()` and `service_entries()`,
  installed by `with_rpc_families` and read by `rpc_families()`, which answers
  `FAILED_PRECONDITION` when unwired. Session start's peer-owned stack base and named-node link,
  and `session_room_roster()`, read it. `with_model_registry`, `with_github_token_store`,
  `with_idle_tracker` and `set_eligible_daemon_source` `debug_assert` it is not installed yet.
- **Shared components**: `RpcActivity` (`relay_idle`), `PeerRouting` (`peer_routing`),
  `LocalExecTools`; free functions `resolve_os_user`, `authorize_exec_tool_caller`,
  `resolve_exec_tool_worktree`, `resolve_tddy_tools_path`, `resolvable_agent_defs`; the
  `handler_state.rs` accessors a handler's `from_host` reads.
- `test_util::RpcFamiliesNotUnderTest` and `tests/rpc_families_port_acceptance.rs`.

`pr_stack_rpc.rs` became a facade over `tddy_pr_stack::rpc`.

**Production lines 22,067 → 20,067** (−2,000; the shape-test rule), against AC11's re-baselined
≥ 2,000 (planned 2,500; the shared helpers stay here as `pub` components). `tests/` 83 → 59 suites.

Code issues: `oversized-file-connection-service` narrowed, ~1,931 → ~1,634 production lines by the
record's own count (total 1,945 → 1,647), still open. `complexity-svc-start-session-core-start-session-core`
(854 → 857), `oversized-file-svc-start-session-core` (908 → 911) and
`complexity-svc-resolve-listed-worktree-ensure-project-available-for-start` (157 → 158) grew from
rustfmt re-wraps of field reads through `PeerRouting`. `complexity-svc-spawn-split-agent-spawn-split-agent`
(251) and `oversized-file-svc-spawn-split-agent` (520) were touched and unchanged. Four records
moved to `tddy-daemon-rpc`.
