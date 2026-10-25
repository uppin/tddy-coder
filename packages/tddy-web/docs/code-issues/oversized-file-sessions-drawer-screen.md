# oversized-file: SessionsDrawerScreen.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #741 (`#start-progress` 8/8); detected again 2026-10-25 on PR #727 (`#coagents` 8/8)
**Metrics:** **508 lines** (non-test source, whole file) · budget 500 · **1.016× over**
**Thresholds breached:** length 508 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 497 | base of PR #741 (`origin/feature/start-progress/progress-ui`, under budget) |
| 2026-10-24 | 504 | +7 in PR #741: the drawer's `taskClient` (from `useSessionsDrawerClients`) and the owning host's `activeTaskClient` (from `useSessionsDrawerOwningHost`), chosen by mode and passed to `SessionMainPane` the way every other per-service client is, so the create pane and the PR-Stack dialog can cancel a start. **Deferred with the developer's consent, #start-progress 8/8** |
| 2026-10-25 | 497 → 501 | PR #727, measured on its pre-#start-progress base: the signed-in login (`callerLogin`) is read from the auth context and passed to the agent-conversation tab, which needs it to cancel. Not split: deferred to the `#coagents` sweep node by the developer (2026-10-25) |
| 2026-10-25 | 504 → 508 | PR #727 rebased onto master's #start-progress: #741's +7 and #727's +4 together |

## What would close it

The `SessionMainPane` prop block picks each per-service client the same way: the selected daemon's
while creating, the owning host's otherwise. Move that choice into `useSessionsDrawerOwningHost`
(or a `useSessionPaneClients(mode)` beside it) so it returns the one client per service the pane
receives, and spread the result. That takes about forty lines out of this file. The drawer-state
hooks and the agent-conversation tab wiring are separable too, into a `useSessionsDrawerState`
hook beside it. Hand split (TypeScript — the `code-restructuring` engine is Rust-only), green
baseline first (`SessionsDrawerAcceptance.cy.tsx`, `SessionsDrawerCrossHostAcceptance.cy.tsx`),
mechanical move only.

## Verified by hand

2026-10-25 — `wc -l` at the rebased tip of PR #727 (508) and at its pre-rebase base (#726, `518110d17`).
