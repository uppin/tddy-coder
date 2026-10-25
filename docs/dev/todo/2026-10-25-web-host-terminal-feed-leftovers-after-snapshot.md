# 2026-10-25 — The web's host terminal feeds keep resume-by-offset plumbing nothing reads

**Category:** Deferred from `#term-snapshot` 1/4 (#760)
**Source:** `/pr-wrap` of #760 — every web host terminal opens with `SNAPSHOT` now, so the byte
resume (`TAIL` then `FROM_OFFSET`) the web used to track has no reader left.

Four leftovers in `packages/tddy-web`, none a defect:

| Leftover | Where | State after #760 |
|---|---|---|
| `TerminalResumePoint` | `src/rpc/connections/terminalFeed.ts`, built per terminal in `hostServedSession.ts` and `localHost.ts` | Written (`record`) on every frame, never read by production code: `synced` and `fromOffset` are read only by `terminalFeed.test.ts` (`a terminal resume point`) |
| `GhosttyTerminalSession`'s `onOffsetUpdate` prop | `src/components/GhosttyTerminalSession.tsx` | No production caller (`GrpcSessionTerminal` stopped passing it); only `GhosttyTerminalSession.cy.tsx` uses it |
| The bidi request queue | `GrpcSessionTerminal.tsx` and `openDaemonTerminalFeed` | Each builds the same `QueuedInput` + `requests()` async generator (open message, then one message per send). De-duplicating the two paths was planned out of scope for #760 |
| `inputDeliverySequence` | `src/rpc/connections/inputDelivery.ts` | Only `InputRefusalTracker` uses it, and there each settle is called at once from in-order stream frames, so the out-of-order guard it provides never applies |

## Why it was deferred

Every file above except `GhosttyTerminalSession.tsx` is also modified by `#term-snapshot` 2/4–4/4
(#761, #762, #763): removing the plumbing or extracting a shared request queue in the root node
would conflict in each successor. Removing `TerminalResumePoint` and `onOffsetUpdate` also deletes the
tests that pin them, which is a test change the wrap could not make unattended.

## What closing it would take

After the stack lands: drop `TerminalResumePoint` (and the `resume` dependency of
`openDaemonTerminalFeed` and its two callers) with its unit tests; drop `onOffsetUpdate` with its
Cypress case; extract one `bidiTerminalRequests(open, queue)` helper both terminal paths use; and let
`InputRefusalTracker` report directly.
