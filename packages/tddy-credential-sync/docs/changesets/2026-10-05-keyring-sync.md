# 2026-10-05 — The crate is born: journaled credential propagation

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

New crate: `SyncEngine`, the `PeerTransport`/`IdentityVerifier` ports, `VaultTransportKey` (X25519),
`SyncJournal` and its `AccountSyncSummary` aggregation. Depends on `tddy-credentials` alone — not
`tddy-daemon-livekit`, not `tddy-daemon-auth`. Detail: [credential-sync.md](../credential-sync.md).
