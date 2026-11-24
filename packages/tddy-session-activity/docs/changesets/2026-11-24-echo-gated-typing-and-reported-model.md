# 2026-11-24 — The record follows the reported model; a wake's paste holds no project lock

**Type:** Fix

- `ReportSessionStatus` runs `follow_reported_model` (`service/model_report.rs`) after
  `follow_conversation_rotation`: the spec's `model_report` sources, a `usage` source evaluated with
  no fallback model over the current main conversation; a differing model is written to
  `.session.yaml` `model` on a copy re-read just before the write. Tests: `tests/model_report_record.rs`.
- `SessionWatchStore::deliver_wake_with` plans under the project lock (`plan_delivery` →
  `PlannedDelivery { delivery, commit: DeliveryCommit }`), pastes with the lock dropped, and commits a
  successful paste (`commit_delivery`: forward only, handed endings taken, a cancelled watch stays
  gone). A per-owner delivery guard (`deliveries`, `delivery_guard`) is held by the wake,
  `take_delivery` and `take_stream_batch_picking`. Tests added to `session_watch_wake_and_pick_unit.rs`.

Docs: [activity-service.md § Following the reported model](../activity-service.md#following-the-reported-model), [session-notifications.md § Session watches](../session-notifications.md#session-watches-session_watches). Full record: [cross-package entry](../../../../docs/dev/changesets/2026-11-24-cursor-tui-submit-and-resume-model.md).
