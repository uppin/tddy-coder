# 2026-10-25 — Development vaults without a passphrase, fenced to stub daemons

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md`](../../../../docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md)

`dev_vault::vaults_without_passphrase` refuses to start a non-stub daemon with the setting and warns when it is on; `provider_auth_entry` passes it to the stub provider's service only. Tests: `tests/dev_vault_without_passphrase_acceptance.rs` (7). `auth.rs` measured 569 → 579 production lines (code issue `oversized-file-auth`, still open). Detail: [auth-service.md](../auth-service.md#development-vaults-without-a-passphrase).
