# 2026-09-24 — The vault unlock key and the credential vault prompt

**Type:** Feature

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`sessionTokenStore` keeps the lineage's `vault_unlock_key` (`tddy_vault_unlock_key`) beside the refresh token: presented on refresh, replaced with the rotated one (kept when a refresh hands the presented one back), sent on logout, cleared with the tokens; a page load holding one refreshes at once (`reopenVaultOnLoad`). `AuthState.vaultState`, `unlockVault` and `resetVault` on the auth context. `CredentialVaultPrompt` (mounted in `src/index.tsx`) asks for the passphrase while `LOCKED` or `UNINITIALIZED`, with a forgot-passphrase reset behind a warning and "Not now". `src/lib/vaultPassphrase.ts` counts the length rule in code points. At wrap, `useAuth.ts` (520 lines) was split, behaviour-preserving: the session shape and the device-poll step moved to `src/hooks/authSession.ts` (124), `useAuth.ts` 413; `DeviceLogin` is re-exported. Detail: [daemon-sign-in.md](../daemon-sign-in.md#the-credential-vault).
