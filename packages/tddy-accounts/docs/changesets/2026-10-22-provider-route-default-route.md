# 2026-10-22 — SetProviderAccountDefaults and the default_for record key

**Type:** Feature

`#provider-route` 5/9 ([#666](https://github.com/uppin/tddy-coder/pull/666)). The `default_for`
record metadata key (`META_DEFAULT_FOR`) with `defaults_of` and `default_account_for`; a new
`ProviderAccountDefaults` port, implemented by `SessionVaultAccountStore` under the store-wide mutex
(provider records only); `AccountsServiceImpl::with_provider_account_defaults`, which serves
`SetProviderAccountDefaults` after validating every harness with `refuse_undrivable` over the route
matrix; `AccountSummary.default_for`. The two route RPCs share one `route_matrix()` refusal. See
[provider accounts](../provider-accounts.md#ports).
