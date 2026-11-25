# 2026-11-25 — Trunk-watch reads go token-only

**Type:** Fix
Cross-package entry: [`docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md`](../../../../docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md)

The trunk poller's account token resolves token-only: REST conditional reads
authenticate as the account and author nothing, so they never need — and never
derive — a commit identity. Session starts keep the paired, deriving resolution
([session identity](../session-identity.md)).
