# 2026-10-05 — `AccountSummary.sync_status`, optional and additive

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

`AccountSummary` carries the aggregate sync standing across every peer; the `SyncStatusSource` port
and `AccountsServiceImpl::with_sync_status` wire it in without changing `::new`'s signature, so every
existing construction site keeps reporting `SYNC_STATUS_UNSPECIFIED` unchanged. Detail:
[accounts-service.md](../accounts-service.md#syncstatussource--keyring-69s-sync-status-optional-and-additive).
