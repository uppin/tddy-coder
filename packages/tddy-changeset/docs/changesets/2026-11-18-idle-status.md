# 2026-11-18 — An idle session waits for input, and a message to it is delivered

**Type:** Fix · `#coder-start` 6/10, PR [#946](https://github.com/uppin/tddy-coder/pull/946)

Doc only: `PendingQuestion::request_id` is never empty by design; an empty one survives only in a record written before #946, and readers treat it as nothing to answer. Full record:
[`docs/dev/changesets/2026-11-18-idle-status.md`](../../../../docs/dev/changesets/2026-11-18-idle-status.md).
