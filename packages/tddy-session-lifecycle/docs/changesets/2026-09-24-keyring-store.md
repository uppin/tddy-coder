# 2026-09-24 — DaemonSessionHost carries the credential vaults

**Type:** Architecture

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`DaemonSessionHost.github_token_store` / `with_github_token_store` become `credential_vaults: Option<Arc<SessionVaults>>` / `with_credential_vaults`, and `handler_state.rs` exposes `credential_vaults()` to the PR-stack handler. `connection_service.rs` unchanged in length (1,652 → 1,652). Detail: [session-service.md](../session-service.md).
