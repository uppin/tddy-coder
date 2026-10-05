# 2026-10-05 — `credential_sync.rs`: a real `SyncEngine`, wired into `runtime::build`

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

The one crate that may depend on both `tddy-credential-sync` and `tddy-daemon-livekit` assembles
them: the persisted transport key, the identity verifier bridging `#keyring` 1/9's `KeyDirectory`,
the signing transport, `EngineSyncStatusSource`, and the peer-join publish trigger — constructed
whenever a common room exists, regardless of `keyring.group_secret`. Detail:
[daemon-endpoint.md](../daemon-endpoint.md#credential-sync).
