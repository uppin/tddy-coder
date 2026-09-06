# 2026-08-15 — `reflection.cy.tsx` had never executed: wrong relative import

**Category:** Known failing test
**Source:** ci-setup, 2026-08-15

- `packages/tddy-livekit-web/cypress/component/reflection.cy.tsx` imported
  `./support/ReflectionTestHarness`, but the harness lives at `cypress/support/`, one level up —
  `transport.cy.tsx` beside it correctly uses `../support/TransportTestHarness`. Vite failed the
  import, Cypress reported it as an uncaught error outside any test, and the spec's real assertions
  never ran.
- Fixed here by correcting the path. Worth noting **how long this survived**: nothing ran this suite,
  so a spec that could not even be parsed looked no different from a passing one. That is the
  argument for the suite being in the PR gate rather than run by hand.
- Its first execution then found two more things, both fixed here:
  - **`JSON.stringify` on a protobuf message.** `ReflectionTestHarness` logged the unary invoke
    result with `JSON.stringify(response.message)`, which throws `Do not know how to serialize a
    BigInt` on the 64-bit field — protobuf-es maps `int64` to `BigInt`. Both server-stream paths in
    the same file already used `toJsonString`; the unary path now matches them.
  - **Reflection did not advertise itself.** Callers of `reflection_entry_from` collect the names of
    the entries they already hold, which by construction cannot include the reflection entry the
    call is about to return, so `list_services` omitted `grpc.reflection.v1.ServerReflection`.
    Appending its own name moved into the helper, so all seven call sites (tddy-coder ×5,
    tddy-daemon, tddy-service) get the conventional gRPC behaviour rather than each fixing it. The
    existing "only registered names" tests construct `ServerReflectionImpl` directly and are
    unaffected — the impl still reports exactly what it is given; the helper decides what to give it.
