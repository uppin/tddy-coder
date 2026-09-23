# 2026-09-23 — The crate is created with the Project, Catalog, ExecTool and PR-stack handlers

**Type:** Architecture

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

Created `tddy-daemon-rpc` above `tddy-session-lifecycle`. It holds `ProjectRpcHandler` (6 fields),
`CatalogRpcHandler` (5), `ExecToolRpcHandler` (6 of a 9 budget) and `PrStackRpcHandler` (6 of 7),
each built by `from_host(&DaemonSessionHost)` from clones of the host's `Arc`s and never holding the
host; `RpcHandlers` bundles them, serves the four `*ServiceImpl`s and their entries, and implements
the lifecycle crate's `DaemonRpcFamilies` port. `RpcHandlers::install` is the composition root's
last step after every `with_*`. The lifecycle crate has no edge back, normal or dev.

The bodies came from the lifecycle crate unchanged except `self.x` → a handler field or shared
component and `crate::` → re-exported crate paths; error codes, messages, timeouts, forwarding names
and `record_rpc_activity` counts per family are the same. Helpers only a handler used moved in with
them (`project/{entries,clone_destination}.rs`, `catalog/{agent_models,subagent_row}.rs`,
`exec_tool/{path_guard,result_frames}.rs`, `pr_stack/{guards,pr_status,branch_legs}.rs`).

**2,905 production lines** at `88c5eaff`, under the 10,000-line cap every successor node holds it
to. `tests/` holds 35 suites, including `rpc_handlers_acceptance.rs` and `rpc_handlers_shape.rs`.

Four code-issue records moved in with their code, renamed and carrying `**Moved:**`:
`complexity-exec-tool-ports-{list-exec-tools,list-session-tool-calls,stream-execute-tool}` and
`complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate`. None is closed.
