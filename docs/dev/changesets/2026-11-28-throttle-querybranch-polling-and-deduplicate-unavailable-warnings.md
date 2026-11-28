# 2026-11-28 — Throttle QueryBranch polling and de-duplicate unavailable warnings

**Type:** Bug Fix

A desktop window left open on a three-branch stack polled `QueryBranch` every 5 s all night —
~2,200 lookups an hour, 29k blocking-lane refusals and an 81 MB daemon log (investigation #1061).

- The PR leg of a branch resolution that cannot be performed now warns through `WarnGate`
  (`work_queue/warn_gate.rs`): one `log::warn!` line per branch and reason, again only when the
  reason changes or the lookup succeeds. The gate is process-wide (`WarnGate::shared()`), so the
  de-duplication outlives any single handler instance. The RPC answer is unchanged — `unavailable`
  with its reason still reaches the row every poll. (tddy-daemon-rpc)
- `useQueryBranch` polls through `createPoller` (`tddy-react-commons/async`) instead of a bare
  `setInterval`: the loop pauses while the page is hidden and polls at once when it is shown again.
  (tddy-workqueue-web)
- `nextQueryBranchDelayMs` (`tddy-workqueue-web/src/queryBranchPacing.ts`) stretches the poll wait
  while consecutive rounds fingerprint identically — 5 s base, 15 s from the third unchanged round,
  30 s from the sixth, capped at 60 s — and any change returns it to 5 s. (tddy-workqueue-web)
- Acceptance coverage: `PrStackPollThrottleAcceptance.cy.tsx` (hidden page polls nothing and catches
  up when shown; backoff stretches; a change returns to base), `queryBranchPacing.test.ts`,
  `warn_gate_unit_tests.rs`. (tddy-session-web, tddy-workqueue-web, tddy-daemon-rpc)
