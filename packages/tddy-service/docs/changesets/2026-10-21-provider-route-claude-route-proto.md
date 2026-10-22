# 2026-10-21 — Provider-route fields in session.proto and accounts.proto

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). `StartSessionRequest` gains `provider_account_id = 41` (empty = no choice at this level) and
`provider_built_in = 42` (explicitly the vendor). `AccountsService` gains
`ListProviderRoutes(ListProviderRoutesRequest { session_token, harness })` →
`ListProviderRoutesResponse { repeated ProviderRouteOption options, harness_routable,
harness_reason }`, `ProviderRouteOption { account_id, label, preset, supported, reason }`. All
additive. Semantics: [provider accounts](../../../../docs/ft/daemon/provider-accounts.md#running-a-harness-on-an-account).
