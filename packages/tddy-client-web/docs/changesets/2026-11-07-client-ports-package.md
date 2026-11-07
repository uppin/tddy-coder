# 2026-11-07 — Package created: generated code, connection model, ports (#web-split 5/16, PR #842)

- `src/gen/**` and its buf config moved from `tddy-web` with `git mv`; the codegen manifest names this
  package.
- Moved from `tddy-web`: connection types, `ConnectionProviderRegistry` (+ test), `useHostFanOut`,
  `hostLabel` (+ test), `appLocation` (+ test).
- New: `ClientsPort`, `NavigationPort`, `AuthPort` as plain React contexts with the pre-split hook names;
  testkit (`aClientsPortDouble`, `aNavigationDouble`, `anAuthDouble`, `withClients`, Cypress
  `mountWithClients`); boundary rule; `web-tddy-client-web.yml`.
- Deviation from plan: ports are not declared with `tddy-react-commons` `withPorts` (rationale in
  [README](../README.md#ports)).
- Red-suite corrections at green: `GetAuthStatusResponse`'s field is `authenticated` (the test named a
  non-existent `isAuthenticated`), and handlers return `create(Schema, …)` messages so `tsc` accepts
  them — same assertions. At wrap, the auth double's actions became one plain recording spy (the
  bun-only `mock` branch and its top-level `await import("bun:test")` were removed); the test asserts
  `port.logout.mock.calls` length 1.

Verified: 54 `bun test` cases, `tsc`, `MountWithClients.cy.tsx`, drift gate.
