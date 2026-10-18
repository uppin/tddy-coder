# 2026-10-18 — Reconnect specs check "not reopened yet" with an assertion that can outrun an early reopen

**Category:** Future enhancement (test strength)
**Source:** `code-index-warm-on-demand` — the keep-alive / re-subscribe fix added `toStillHaveOpened`

## What is left

`packages/tddy-web/cypress/component/WorktreeCodePaneReconnect.cy.tsx` asserts the backoff by ticking
the clock to just before each reopen and checking that nothing reopened yet. Three of its tests — *a
session dropped under the pane is reopened after a second…*, *waits twice as long after each failed
attempt…* and *a host that accepts each session and drops it at once is backed off…* — make that check
with `expectTheHost(host).toHaveOpened(..)`, a **retried** assertion. Everything a `cy.tick` sets off (the
reopening timer, the pane's stream reaching the host) runs as promise continuations, which a retried
assertion can outrun: it passes on the old count before an early reopen has landed. So those tests would
not catch a reopen that fires too early.

`toStillHaveOpened` (`cypress/support/rpc/codeSessionAssertions.ts`) waits one real macrotask first and
is used only by *a reopened session that stayed open for 10 seconds waits a second again…*.

## Why it was deferred

Found while writing that last test, in the fix commit at the end of the change; switching the other
checks was outside that commit's scope and needs the specs re-run under `cy.clock`, which is not cheap
locally.

## What would close it

Use `toStillHaveOpened` for every "nothing reopened before the wait" check in
`WorktreeCodePaneReconnect.cy.tsx` (and the header's equivalent in
`SessionIndexingIndicatorResubscribe.cy.tsx`, if it has the same shape), and confirm each fails against a
deliberately shortened first wait.
