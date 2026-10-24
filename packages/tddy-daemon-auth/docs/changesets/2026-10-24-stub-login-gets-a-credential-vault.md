# 2026-10-24 — A stub daemon's operator is asked to create their vault

**Type:** Fix · PR [#711](https://github.com/uppin/tddy-coder/pull/711)
Cross-package entry: [`docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md`](../../../../docs/dev/changesets/2026-10-24-stub-login-gets-a-credential-vault.md)

`login_opens_the_credential_store_acceptance.rs`: `a_stub_login_is_asked_to_create_the_vault_and_writes_nothing_yet`
and `a_stub_operator_creates_their_vault_with_a_first_passphrase` (sign-in and unlock through one
served `auth.AuthService`, via `sign_in_through`). Detail: [auth-service.md](../auth-service.md).
