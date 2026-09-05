# 2026-07-03 — `AddProjectToHost` handler + multi-host project aggregation

**Type:** Feature

`ConnectionServiceImpl::add_project_to_host` makes an existing project available on another host reusing its `project_id`: routes local-or-forward (`classify_peer_route` + new `forward_add_project_to_host_via_livekit`, mirroring `StartSession`), clones the repo and persists via the new idempotent `project_storage::add_or_get_project`. `list_projects` honors a new `local_only` flag (skips peer merge); `LiveKitEligibleDaemonSource::peer_project_entries` now fans out to peers' `ListProjects` (`local_only=true`, tagged by `daemon_instance_id`, bridging sync→async via `block_in_place`+`Handle::block_on`). [connection-service.md § Multi-host projects](../connection-service.md#multi-host-projects). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-service, tddy-web)
