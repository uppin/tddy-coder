# 2026-11-28 — PR-Stack: `QueryBranch` call-volume follow-ups

**Category:** Future enhancement
**Source:** #1063 (throttle QueryBranch polling and de-duplicate unavailable warnings)

Poll throttling (#1063) bounds the client's rate but not the per-call cost: every `QueryBranch`
still costs the daemon a session scan, a git comparison on its blocking lane and a GitHub PR
lookup, per branch, on whatever cadence the view polls. Three ways to cut the cost itself:

- **Batch the branches into one call.** The PR-Stack view issues one `QueryBranch` per rendered
  branch per round, in parallel — a ten-node stack is ten RPCs, ten session scans and ten PR
  lookups every round. A batched `QueryBranches` (one request carrying every branch, one response
  with a resolution per branch) would amortise the prologue and let the daemon share one session
  scan across branches of the same queue.
- **Cache the PR status server-side, briefly.** Each lookup is an authenticated `GET /pulls`
  against GitHub's rate limit, and consecutive polls of an unchanged branch ask GitHub the same
  question. A short server-side cache (tens of seconds) would collapse repeated lookups of the
  same branch without visibly delaying a change; the throttled client cadence already tolerates
  staleness of that order.
- **Push branch status through the status stream instead of polling.** The daemon already has
  `StreamWorkQueueStatus` (`work_queue/status_stream.rs`), woken by the GitHub inbox and by
  `QueueChanges`. Delivering `BranchResolution`s (or the deltas that drive them) over that stream
  would remove the poll loop entirely: the view updates when something actually changes, instead
  of asking on an interval whether anything did.
