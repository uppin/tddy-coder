# 2026-10-25 — The accounts service answers DescribeSessionStartOptions

**Type:** Feature

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)).
`runtime/mount_accounts.rs`: on a host with credential vaults, the accounts service instance the RPC
serves is also attached to the RPC handlers as their `ProviderAccountsPort`
(`AccountsProviderPort`: `ListProviderRoutes` and `ListProviderAccountModels` by the caller's token).
A host without vaults attaches none.
