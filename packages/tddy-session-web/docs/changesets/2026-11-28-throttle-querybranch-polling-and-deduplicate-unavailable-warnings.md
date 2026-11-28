# 2026-11-28 — Throttle QueryBranch polling and de-duplicate unavailable warnings

**Type:** Bug Fix

`PrStackPollThrottleAcceptance.cy.tsx` pins the PR-Stack view's throttled polling: a hidden page
sends no `QueryBranch` calls and polls at once when it is shown again; unchanged answers stretch
the wait 5 → 15 → 30 → 60 s; a changed answer returns it to the 5 s base interval.
