# 2026-07-03 — PR-stack: internal status + child-spawn relay types

**Type:** Feature

`StackNode` gains `internal_status: Option<PrInternalStatus>` (`{kind, note, source}`; `#[serde(default, skip_serializing_if)]` for back-compat), the action-needed signal orthogonal to `pr_status`. `toolcall` gains a `spawn-child` verb: `SpawnChildRequestWire`, `ToolCallResponse::SpawnChildOk { session_id }`, and a `ChildSpawnHandler` async trait + per-instance `ToolcallRpcService::with_child_spawn_handler` (mirrors the `transition` handler) so a session-owning process can spawn a stack child over `TDDY_SOCKET`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core, tddy-workflow-recipes, tddy-tools, tddy-daemon)
