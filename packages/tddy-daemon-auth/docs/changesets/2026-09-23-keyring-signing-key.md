# 2026-09-23 — The daemon holds its own signing key and declares the key-directory port

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`signing_key.rs` (new): `DaemonSigningKey` (generate once at `0600` via a hard-linked staging file, reuse, refuse a key others can read), `load_signing_key` / `signing_key_path` (`auth_storage`, else `<tddy_data_dir>/auth`; refused when neither is set), `KeyDirectory` (resolve only) and `StandaloneKeyDirectory`, `DirectorySessionTokenVerifier` (`verify_now` polls once), `SessionTokens`. `build_auth_entries_with` takes the daemon's `SessionTokens` and no longer reads `livekit`; it warns once when `auth_storage` is looser than `0700`. `AUTH_LOG_TARGET`. Detail: [auth-service.md](../auth-service.md).
