# 2026-10-04 — The daemon serves accounts.AccountsService

**Type:** Feature

`#keyring` 4/9 — PR [#511](https://github.com/uppin/tddy-coder/pull/511). Cross-package entry:
[2026-10-04-keyring-accounts.md](../../../../docs/dev/changesets/2026-10-04-keyring-accounts.md).

`runtime::build` registers `tddy_accounts::build_accounts_entry` over the credential vaults auth
builds, when `auth_storage` is set. `DaemonRuntime` keeps that one `SessionVaults` handle privately
and exposes it as `credential_vaults()`. `tests/accounts_stub_daemon_acceptance.rs` (registered in
`test_placement.rs`) opens a stub user's vault through it and lists accounts through the real roster.
Detail: [daemon-endpoint.md](../daemon-endpoint.md).

`runtime.rs` 1,677 → 1,700 production lines and `build` 937 → 950: both records regressed, split
deferred with the developer's consent (`docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`).
