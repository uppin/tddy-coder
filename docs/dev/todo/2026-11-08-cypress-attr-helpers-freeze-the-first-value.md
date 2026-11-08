# 2026-11-08 — Cypress page helpers that read an attribute through `.then` freeze its first value

**Category:** Flaky test support
**Source:** `#web-split` 13/20 (PR #849), `/fix-pr` — `ModelsPanelStatesAcceptance` failed once on CI with
`expected 'loading' to equal 'ready'` in a spec that ran in 501 ms.

A helper written as `byTestId(id).invoke("attr", name).then(requiredAttribute)` hands the caller a
value, not a query: `.then` is not retryable, so a following `.should("equal", "ready")` checks the
first attribute value rendered and fails whenever the RPC behind it is slower than the first paint.
#849 fixed the four status helpers in `tddy-workspace-web/cypress/support/pages/modelsScreenPage.ts`
with a retryable `statusAttribute` (a `.should` that requires the attribute, then `.invoke("attr")`).

About 12 more `invoke("attr", …).then(…)` helpers in other packages' `cypress/support` have the same
flaw (for example `tddy-chat-web` and `tddy-web` `agentChatPage.ts`, `tddy-web` `prStackScreenPage.ts`,
and the row-attribute readers in `modelsScreenPage.ts`). They predate the stack and are on master too.

**Why deferred:** they are not failing today, and fixing them touches packages this node does not own.

**What closing it takes:** `TODO(web-split sweep)` — turn each into a retryable query (the
`statusAttribute` shape), keeping its "attribute is required" failure message and asserted values.
