# 2026-08-15 — `handles AbortSignal cancellation` asserts a log prefix production no longer emits

**Category:** Known failing test
**Source:** ci-setup, 2026-08-15

- `packages/tddy-livekit-web/cypress/component/transport.cy.tsx:141` looks for a captured log line
  containing **`[LiveKitTransport]`** and `cancelled`, and fails `expected undefined to exist`.
  Grepping `packages/` for the literal `[LiveKitTransport]` finds it in exactly two places — this
  assertion and the capture filter in `cypress/support/component.ts:13`. **No production code emits
  it.** `src/transport.ts` logs through the `debug` package as
  `createDebug("tddy:rpc:livekit-transport")`, so the prefix is the namespace, not that bracketed
  string. The assertion cannot pass in any environment, with or without `DEBUG` set.
- The other five tests in the file pass; they assert on `[TEST] error:`, which the harness does emit.
- **Interim (2026-08-15):** the `transportError` assertion was dropped so the test proves what it is
  actually for — that cancellation reaches the caller, via the `[TEST] error: cancelled` assertions
  that do pass. The test is narrower than it was written to be, and knowingly so.
- **Still open:** whether the transport should emit a stable, capturable marker at all. Either
  production logs a `[LiveKitTransport]` prefix that `cypress/support/component.ts` captures, or the
  test asserts on the `debug` namespace and that filter is widened to match. That is a contract
  decision about what the transport promises, not a test cleanup — which is why it was not settled
  here.
- Found the first time this suite ran in CI. It had never run before — see the entry below.
