# 2026-10-21 — Model-provider accounts connected by API key

**Type:** Feature

`#provider-route` 1/9 ([#662](https://github.com/uppin/tddy-coder/pull/662)). New
`provider_account` module: the six-preset table, `endpoints_for` (refuses an unknown preset, a
missing operator-supplied root, an override of a fixed root, a root for an unserved protocol and an
unusable URL), the `ProviderAccount` ⇄ `CredentialRecord` codec, `account_id_for` /
`unclaimed_account_id`, and the `ProviderModelProbe` / `ProviderAccountStore` ports.
`AccountsServiceImpl::with_provider_accounts` serves `ConnectProviderAccount` (probe before store)
and `ListProviderAccountModels`; `AccountSummary` carries `preset` and `wire_protocols`.
`SessionVaultAccountStore::put_provider_account` chooses a free id atomically with the write, so two
simultaneous connects of one label never overwrite each other. See
[provider accounts](../provider-accounts.md).
