# 2026-10-05 — `DaemonSigningKey::sign`: attesting something that is not a session token

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

`DaemonSigningKey::sign(message) -> Vec<u8>` signs arbitrary bytes with this daemon's identity key —
`#keyring` 6/9's peer advertisements, the first caller that is not a session token. `signer()`'s
`SessionTokenSigner` is unchanged. Detail:
[auth-service.md](../auth-service.md#one-key-signs-one-thing).
