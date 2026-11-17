# 2026-11-17 — An announced agent end becomes a bus notification

**Type:** Feature · `#activity-wake` 1/3, PR [#929](https://github.com/uppin/tddy-coder/pull/929)

`session_notifications::{notification_for_agent_end, forward_agent_ends}`: the forwarder resolves the owner's sessions base and the drawer's label, and publishes one `ATTENTION_REQUIRED` notification with source `SessionEnded`. An end whose owner has no sessions base is logged at `warn` and dropped; a lagging forwarder logs how many ends it missed. Full record: [`docs/dev/changesets/2026-11-17-session-ended-notice.md`](../../../../docs/dev/changesets/2026-11-17-session-ended-notice.md).
