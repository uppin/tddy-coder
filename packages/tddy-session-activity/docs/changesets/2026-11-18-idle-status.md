# 2026-11-18 — An idle session waits for input, and a message to it is delivered

**Type:** Fix · `#coder-start` 6/10, PR [#946](https://github.com/uppin/tddy-coder/pull/946)

`record_pending_question` (`service/hook_reports.rs`) writes no pending-question record for a `WaitingForInput` with no tool and no structured question still pending — Claude's `idle_prompt` leaves the session waiting for input with nothing to answer, instead of an id-less `permission` record. Full record:
[`docs/dev/changesets/2026-11-18-idle-status.md`](../../../../docs/dev/changesets/2026-11-18-idle-status.md).
