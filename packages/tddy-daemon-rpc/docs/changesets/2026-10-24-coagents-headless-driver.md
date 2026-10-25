# 2026-10-24 — `ExecToolService.RequestApproval`

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). `ExecToolRpcHandler` answers `RequestApproval` through `ApprovalRoutes` (field 10 of its budget of 10), from the caller's session id and token; a refused route is `PermissionDenied`. The coding-agent catalog row carries `transport` and `permission_policy`. Tests: `approval_round_trip_through_the_tool_client`, `exec_tool_approval_route`, `exec_tool_approval_release_on_disconnect`, `headless_listing_acceptance`; `rpc_handlers_shape` budget 9 → 10. See [architecture](../architecture.md#the-handlers).
