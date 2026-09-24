# 2026-09-24 — A tool's session refresh presents no vault unlock key

**Type:** Fix

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`daemon_rpc.rs`'s `RefreshSessionRequest` sets `vault_unlock_key: ""`: a tool is not a browser session lineage and was handed no key, so its refresh reopens no credential vault.
