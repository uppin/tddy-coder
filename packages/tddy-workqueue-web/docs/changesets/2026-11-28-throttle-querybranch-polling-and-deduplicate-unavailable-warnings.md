# 2026-11-28 — Throttle QueryBranch polling and de-duplicate unavailable warnings

**Type:** Bug Fix

`useQueryBranch` no longer polls `QueryBranch` on a fixed 5 s `setInterval`. It polls through
`createPoller` (`tddy-react-commons/async`) — the loop pauses while the page is hidden and polls
at once when it is shown again — and `nextQueryBranchDelayMs` (`queryBranchPacing.ts`) chooses
the wait from each round's fingerprint: 5 s while answers keep changing, 15 s from the third
unchanged round, 30 s from the sixth, capped at 60 s; any change returns it to 5 s. A desktop
window left open on a three-branch stack had polled every 5 s all night, ~2,200 lookups an hour
(investigation #1061). The daemon-side counterpart is the `WarnGate` log de-duplication in
`tddy-daemon-rpc`; see
[pr-stack-live-status.md](../../../docs/ft/coder/pr-stack-live-status.md#querybranch-polling-cadence-added-2026-11-28).
