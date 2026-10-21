# 2026-10-21 — Provider accounts: connect a model-provider API key and list its models

**Type:** Feature

`#provider-route` 1/9 ([#662](https://github.com/uppin/tddy-coder/pull/662)), across
`tddy-service` (`accounts.proto`: `ConnectProviderAccount`, `ListProviderAccountModels`,
`AccountSummary.preset` / `wire_protocols`), `tddy-accounts` (presets, record codec, service, store
port), `tddy-daemon` (HTTP probe, wiring) and `tddy-web` (Connect API key form). Provider accounts
live in the per-person vault, not the model registry; `tddy-accounts` does not depend on
`tddy-model-registry`. Docs: [provider accounts](../../ft/daemon/provider-accounts.md),
[tddy-accounts](../../../packages/tddy-accounts/docs/provider-accounts.md).
