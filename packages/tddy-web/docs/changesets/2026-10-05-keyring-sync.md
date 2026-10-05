# 2026-10-05 — A sync-status badge on the Accounts screen

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

Each account row shows an aggregate sync badge (synced/pending/undeliverable/conflict/refused, or
none at all) — the single worst status across every peer, not a per-peer breakdown. Extracted as
its own `SyncStatusBadge` component rather than inlined into the already over-budget
`AccountRowView`. Detail:
[accounts-screen.md](../accounts-screen.md#the-sync-status-badge).
