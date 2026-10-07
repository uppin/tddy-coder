# 2026-10-07 — Screen-sharing credentials become vault records

**Type:** Architecture

A desktop's password is a `screen-sharing` record in the signed-in user's credential store. The
package's own vault is deleted: `ScreenSharingVault` (`screen_sharing_vault.rs`, 394 lines),
`DerivedKey`, `ScreenSharingKeyCache`, `.screen-sharing.yaml`, the direct `argon2`,
`chacha20poly1305`, `rand` and `serde_yaml` dependencies, and `UnlockVault`. New modules
`screen_sharing_records` and `session_vault_target_store`; new
[screen-sharing-service.md](../screen-sharing-service.md).

The four limits of the deleted file, measured at planning, and where each went:

| Limit | Now |
|---|---|
| target label, host, port, protocol, username outside the AEAD | inside the sealed record |
| Argon2 parameters unversioned | the store's header carries them |
| passphrase travelled in `UnlockVaultRequest` | no passphrase exists |
| `DerivedKey` cached un-zeroized per `session_id` | `SessionVault` zeroizes on drop |

Targets are per user rather than per session. Existing `.screen-sharing.yaml` files are not read
(breaking by decision: no migration path would avoid keeping the unlock RPC alive).

Code-issue measurements: `oversized-file-screen-sharing-service` 967 production lines, unchanged
(`require_key` left the file; `with_credential_vaults` replaced the key cache argument).
