# 2026-11-25 — Hold refused lines until registration; poll the transcript

**Type:** Fix

`HookReportDelivery` answers `HookDeliveryError`, and the sinks hold a report refused only with session-not-registered-yet and retry until the session exists (n1); `follow_transcript` polls (`FileFollower::start_polling`, size + suffix) instead of `tail -F`, so an atomic rewrite delivers only new lines (n2). Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-followups.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-followups.md) (#1037).
