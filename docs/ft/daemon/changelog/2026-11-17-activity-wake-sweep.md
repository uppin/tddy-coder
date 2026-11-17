# 2026-11-17 — The activity-wake sweep's file splits

The `#activity-wake` stack's sweep (`#activity-wake` 6/6, PR #936) split the three files the
stack's PRs grew past the 500-production-line budget, by restructure-engine moves only — no
behaviour changed, no surface moved. `session_watches.rs` (857 → 205 production lines) became
`matcher`, `delivery` and `store` submodules; `project_notification_log/storage.rs`
(578 → 220) moved its DTOs to `storage/dto.rs`; `mount_host.rs` (532 → 444) moved
`BuildContext::open_shared` to `mount_host/open_shared.rs`. Their three `oversized-file-*`
records are closed with the final measurements in
[the changeset](../../../dev/changesets/2026-11-17-activity-wake-sweep.md).
