# 2026-11-25 — The notification relay: project-log records to the hosts they concern

**Type:** Feature
**PR:** #1036

- `notification_relay`: `NotificationRelay` routes each appended record (orchestrator on another
  host, message to another host, a peer's follow), pushes it over `RelayPorts`, keeps persisted
  per-(project, host) cursors, and reports routed records evicted before they were taken as `lost`.
- `project_notification_log`: records carry `daemon_instance_id`, `origin_seq` and `relay_lost`;
  `append_relayed` (origin-`seq` dedupe, relay gaps), `on_append`, `held_records`, `project_ids`,
  `appended`, `evicted_relay_lost_after`, `record_from_json`, `record_json_on`.
- `session_watches`: relay gaps reported once (`relay_gaps`); lease types moved to `stream/lease.rs`.
- `session_messages::InboxMessage::from_daemon_instance_id`.

Cross-package: [2026-11-25-cross-host-watch-and-messages.md](../../../../docs/dev/changesets/2026-11-25-cross-host-watch-and-messages.md)
