# 2026-10-22 — The provider route file of a tool session

**Type:** Feature

`#provider-route` 4/9 ([#665](https://github.com/uppin/tddy-coder/pull/665)).
`write_provider_route_file(session_dir, route_json, os_user)` writes `provider-route.json` owner-only
and chowned, through the same `write_owner_only_session_file` the agent definition uses;
`SpawnOptions.provider_route_json` / `SpawnRequest.provider_route_json` (`#[serde(default)]`) make
`plan_session_child` write it and pass `--provider-route-path`. `CloneRequest` moved to
`spawn_worker/clone_request.rs` (re-exported).
