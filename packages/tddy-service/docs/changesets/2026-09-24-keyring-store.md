# 2026-09-24 — auth.proto: VaultState, UnlockVault, ResetVault and the unlock key

**Type:** Feature

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`auth.proto`, additive: `vault_unlock_key` on `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionRequest`, `RefreshSessionResponse` and `LogoutRequest`; `VaultState { UNSPECIFIED, NONE, OPEN, LOCKED, UNINITIALIZED }` on `ExchangeCodeResponse`, `PollDeviceLoginResponse`, `RefreshSessionResponse` and `GetAuthStatusResponse`; two RPCs, `UnlockVault(session_token, passphrase, create)` and `ResetVault(session_token, new_passphrase)`, each answering `{ vault_state, vault_unlock_key }`. `build.rs` generates `UnlockVaultRequest` / `ResetVaultRequest` with prost's `skip_debug`, and `src/auth_redacted_debug.rs` prints them — passphrase and access token — redacted (`tests/auth_passphrase_redaction.rs`). `unbundle_service_split` needed no registration (28 / 0).

Code issue opened at this wrap: `oversized-file-build` (`build.rs` 692 → 695 lines, `main` 634), deferred with consent.
