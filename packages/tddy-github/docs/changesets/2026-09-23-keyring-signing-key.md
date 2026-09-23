# 2026-09-23 — Session tokens become `v2`, Ed25519-signed with a key id

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`session_token_v2.rs` replaces `session_token.rs` (deleted, with `hmac` and `subtle`). `KeyId` is derived from the SPKI DER of the signing key; `SessionTokenSigner::new(signing_key)` derives it; `SessionTokenVerifier::key_id_of` reads it untrusted and `verify(token, &key, now)` checks it, the signature (`verify_strict`) and the expiry. `SessionTokenAuthority` is what `AuthServiceImpl::new_signed` verifies status and refresh through. `v1` is `UnsupportedVersion`; a claims payload without `kind` is `Malformed`. The tampered-signature test flips a decoded signature byte and no longer flakes. Detail: [session-token.md](../session-token.md).
