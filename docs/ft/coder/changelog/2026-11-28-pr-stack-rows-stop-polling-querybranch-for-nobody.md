# 2026-11-28 — PR-Stack rows stop polling `QueryBranch` for nobody

A desktop window left open on a three-branch stack polled `QueryBranch` every 5 s all night —
~2,200 lookups an hour, 29k blocking-lane refusals and an 81 MB daemon log, with nobody at the
machine (investigation #1061). #1063 bounds the cost on both sides:

- The PR-Stack view pauses its poll loop while the page is hidden and polls at once when it is
  shown again; unchanged answers stretch the wait 5 → 15 → 30 → 60 s, and any change returns it
  to the 5 s base interval (`useQueryBranch` through `createPoller`, `queryBranchPacing.ts`).
- The daemon de-duplicates the "PR status unavailable" warning: one log line per branch and
  reason, again when the reason changes or the lookup succeeds (`WarnGate`). The row still shows
  the unavailable state and its reason tooltip on every poll — only the log line is gated.

Requirements:
[pr-stack-live-status.md](../pr-stack-live-status.md#querybranch-polling-cadence-added-2026-11-28).
