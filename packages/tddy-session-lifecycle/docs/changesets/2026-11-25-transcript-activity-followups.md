# 2026-11-25 — Carry the activity Status across hook delivery

**Type:** Fix

`ActivityHookDelivery` maps the activity service's `Status` into `HookDeliveryError` instead of its message alone, so the followers can hold session-not-registered-yet (n1). Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-followups.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-followups.md) (#1037).
