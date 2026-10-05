# 2026-10-05 — Records carry a version, and a deletion is a tombstone

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

`CredentialRecord.version`, `Tombstone`, `VaultEntry` (`Record | Tombstone`, neither derives
`Serialize` for the secret-leak reason `CredentialRecord` already didn't). `SessionVault::remove`
replaces a slot with a tombstone rather than emptying it; `SessionVault::entries` surfaces every
slot, deletions included — what reconciliation between daemons reads. Detail:
[credential-store.md](../credential-store.md).
