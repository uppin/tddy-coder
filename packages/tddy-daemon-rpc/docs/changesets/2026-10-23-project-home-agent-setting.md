# 2026-10-23 — SetProjectHomeAgent and one coordinate route

**Type:** Feature

`project/home_agent_handler.rs` serves `SetProjectHomeAgent`: routed to the named host, the id checked
against `catalog::listed_agents` (the `ListAgents` assembly, moved into `catalog/agent_rows.rs`), then
written. `ProjectRpcHandler` gains `model_registry` (budget 7). `project/coordinate_route.rs`
(`coordinate_route`) replaces the routing block four project RPCs repeated. See
[architecture § Module layout](../architecture.md#module-layout).
