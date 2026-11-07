# 2026-11-07 — The RPC playground screen moves here (#web-split 9/17, PR #846)

New package. `RpcPlaygroundScreen`, `invoke`, `registry`, `protoMessageJson` and the echo-service
descriptor fixture moved from `tddy-web/src/rpc-playground/` with their three `bun test` suites and
two Cypress specs. The screen reads URL state through `tddy-client-web`'s navigation port instead of
`tddy-web`'s hash store; the specs mount it with `cy.mountWithClients` over an in-memory backend
(the URL-state spec with a navigation double) instead of intercepting HTTP. The fake Connect
transports the suites use moved from `tddy-web`'s `src/test-utils` to `src/test-utils/`.
The suite for empty request input now pins the behaviour (`{}`), replacing an open FIXME.
