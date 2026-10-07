# 2026-10-07 — screen_sharing.proto: no RPC carries a passphrase

**Type:** Architecture

`UnlockVault`, `UnlockVaultRequest` and `UnlockVaultResponse` are deleted from `screen_sharing.proto`
— breaking, and not deprecated, since a deprecated RPC carrying a passphrase is still one.
`ListTargetsResponse` carries `vault_locked`, so a store sealed under another login is a state of the
answer rather than an empty list. `tests/screen_sharing_carries_no_passphrase.rs` asserts over the
schema that no RPC is an unlock and no message has a passphrase field. TypeScript codegen
regenerated. Behaviour: [screen-sharing-service.md](../../../tddy-screen-sharing/docs/screen-sharing-service.md).
