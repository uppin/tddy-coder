# 2026-10-24 — A sign-in with no credential can still create a vault

**Type:** Fix · PR [#711](https://github.com/uppin/tddy-coder/pull/711)
Cross-package entry: [`docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md`](../../../../docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)

`SessionVaults::sign_in_without_credential(subject)`: over an open vault it hands the lineage an
unlock slot and seals nothing; over a closed one it holds a credential-free pending mark
(`Pending::record` is `Option<CredentialRecord>`) that permits `create` / `reset` like a waiting
credential, for the same pending lifetime. Tests:
`a_credential_free_sign_in_permits_creating_a_vault_that_holds_nothing`,
`a_credential_free_sign_in_over_an_open_vault_gets_an_unlock_slot_and_seals_nothing`. Detail:
[credential-store.md](../credential-store.md).
