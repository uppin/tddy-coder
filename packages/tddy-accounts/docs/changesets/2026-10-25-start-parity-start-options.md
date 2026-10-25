# 2026-10-25 — build_accounts_entry takes a shared service

**Type:** Change

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)). `build_accounts_entry`
takes `Arc<AccountsServiceImpl<S>>`, so the daemon serves the RPC from the instance it also hands
`DescribeSessionStartOptions` as its provider-accounts port.
