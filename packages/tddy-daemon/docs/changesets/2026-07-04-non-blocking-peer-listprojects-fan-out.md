# 2026-07-04 — Non-blocking peer `ListProjects` fan-out

**Type:** Refactor

`EligibleDaemonSource::peer_project_entries` is now an `async` trait method (`#[async_trait]`); `merge_listed_projects_with_peers` and the `ListProjects` handler await it directly, deleting the `block_in_place`+`Handle::block_on` (and `Handle::try_current`) bridge in `LiveKitEligibleDaemonSource` — no worker thread is parked and a current-thread runtime now suffices. `aggregate_peer_project_entries` fans out to peers concurrently (`futures_util::future::join_all`) so the aggregate is bounded by the slowest responsive peer, not the serial sum; timeout/error skip semantics unchanged. New acceptance `non_blocking_peer_fanout_acceptance`; existing peer sources migrated to the async trait. [connection-service.md § Multi-host projects](../connection-service.md#multi-host-projects). (tddy-daemon)
