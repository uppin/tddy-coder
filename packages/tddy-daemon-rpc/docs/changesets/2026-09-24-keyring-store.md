# 2026-09-24 — The PR-status read comes from the caller's credential vault

**Type:** Architecture

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`PrStackRpcHandler.credential_vaults` (from `DaemonSessionHost::credential_vaults()`) replaces `github_token_store`; `pr_stack/pr_status.rs` reads through `retained_github_token`, so a closed vault is *unavailable* with a reason saying to unlock it. `pr_lookup_for_caller`'s three outcomes are unchanged. A path dependency on `tddy-credentials` (internal). Detail: [architecture.md](../architecture.md).
