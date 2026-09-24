# 2026-09-24 — The credential vaults replace the plaintext GitHub token file

**Type:** Architecture

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`github_token_store.rs` (`FileGitHubTokenStore`, `github-tokens.json`) is **deleted**. `build_auth_entries_admitting` probes `auth_storage` with a free `probe_writable` and builds one `SessionVaults` through the new `pending_logins::credential_vaults_in(dir, github.pending_login_ttl_seconds)`, returned as `AuthBuildResult::credential_vaults`; `pending_logins::spawn_pending_login_sweep` drops expired pending tokens every `min(ttl, 60 s)` (no task at `0`). `github_pr_credentials::retained_github_token` reads a caller's token by vault state and names the remedy while it is closed. `SessionVaults` is re-exported. New suites: `login_opens_the_credential_store_acceptance`, `vault_unlock_across_restart_acceptance`, `credential_vault_guard_acceptance`, `pending_login_expiry_acceptance`, with a per-exchange-token fake GitHub in `tests/support/mod.rs`.

Code issues: `oversized-file-auth` opened at this wrap (`auth.rs` 576 → 622 production lines, split deferred with consent); `complexity-auth-build-auth-entries-admitting` regressed 106 → 110 lines, deferred with consent. Detail: [auth-service.md](../auth-service.md#credential-vaults).
