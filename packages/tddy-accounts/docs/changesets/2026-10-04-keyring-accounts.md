# 2026-10-04 — tddy-accounts: the Accounts service over the credential vault

**Type:** Feature

`#keyring` 4/9 — PR [#511](https://github.com/uppin/tddy-coder/pull/511). Cross-package entry:
[2026-10-04-keyring-accounts.md](../../../../docs/dev/changesets/2026-10-04-keyring-accounts.md).

New crate. `AccountsServiceImpl<S: AccountStore>` serves `accounts.AccountsService` —
`ListAccounts`, `SetAccountLabel`, `RemoveAccount` — returning `AccountSummary` with no secret field
(`has_secret` is one bit). `AccountsError` (`NoSuchSession`, `Locked`, `Uninitialized`, `NotFound`,
`Unavailable`) maps to `vault_locked` / `vault_uninitialized` on a listing and to `Unauthenticated`,
`FailedPrecondition`, `NotFound` and `Internal` elsewhere. `SessionVaultAccountStore` is the
production store over `SessionVaults` and an injected `SessionSubjectResolver`; an `Io` failure
reaches the client as a fixed path-free sentence and the log in full. `build_accounts_entry` names
the service `accounts.AccountsService`. Detail: [accounts-service.md](../accounts-service.md).

Tests: `tests/accounts_service_acceptance.rs` over an in-memory store (including the encoded-response
no-secret check), and `vault_store.rs` unit tests against real vaults in a temp dir.

Known limitation kept in the backlog: `docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md`.
