# 2026-10-07 — Screen-sharing credentials become vault records, and the screen-sharing passphrase is deleted

**Type:** Architecture

`#keyring` 7/9 — PR [#514](https://github.com/uppin/tddy-coder/pull/514),
`feature/keyring/screen-share`.

Screen-sharing is the second provider in the credential store. A target's password is a
`screen-sharing` record sealed with its label, host, port, protocol and username; the signed-in
session opens the store, so no passphrase exists. Targets are per user and available in the next
session. `tddy-credentials` is unchanged apart from one doc comment that named the deleted module.

- **tddy-screen-sharing** — `screen_sharing_vault.rs`, `ScreenSharingVault`, `DerivedKey`,
  `ScreenSharingKeyCache` and the `argon2`/`chacha20poly1305` use deleted; new
  `screen_sharing_records` and `session_vault_target_store`. See
  [screen-sharing-service.md](../../packages/tddy-screen-sharing/docs/screen-sharing-service.md).
- **tddy-service** — `UnlockVault` and its two messages deleted from `screen_sharing.proto`
  (breaking, deliberately not deprecated); `ListTargetsResponse` carries `vault_locked`.
- **tddy-daemon** — `runtime::build` wires `with_credential_vaults` and drops the key cache; −4
  production lines in `runtime.rs` (1,781 → 1,777).
- **tddy-web** — the passphrase dialog is deleted; the Screen Sharing tab renders a locked store as
  locked, never as "no targets".
- **tddy-host-service**, **tddy-credentials** — doc comments no longer name the deleted module.

Breaking by decision, no migration: `.screen-sharing.yaml` files are not read and targets are added
again once, because migrating would keep the unlock RPC alive for one run.

Measured limits of the deleted vault (inside the AEAD, versioned KDF parameters, no passphrase on the
wire, zeroized keys) are recorded in the package changeset.
