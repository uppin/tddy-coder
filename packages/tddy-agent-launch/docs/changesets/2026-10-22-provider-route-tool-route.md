# 2026-10-22 — A tool session's provider route

**Type:** Feature

`#provider-route` 4/9 ([#665](https://github.com/uppin/tddy-coder/pull/665)). `tool_session_route.rs`:
`tool_session_route_json` (start) and `resumed_tool_session_route_json` (resume) resolve the session's
route for its agent into route-file JSON, carried as `ToolSpawnPlan.provider_route_json`. See
[module layout § Provider route of a session](../module-layout.md#provider-route-of-a-session).
