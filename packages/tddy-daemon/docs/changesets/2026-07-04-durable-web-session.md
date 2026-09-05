# 2026-07-04 — **Durable web session

**Type:** Feature

refresh-kind token enforcement** — the per-RPC `SessionUserResolver` (`auth.rs`) now additionally requires `claims.kind == TokenKind::Access`, rejecting a `refresh`-kind token even if it signature-verifies — the long-lived credential minted by `RefreshSession` must never authenticate a normal RPC. Complements `tddy-github`'s new two-token (access + refresh) model. Feature [session-auth.md § Durable sessions](../../../../docs/ft/daemon/session-auth.md#durable-sessions-access--refresh-tokens). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#283](https://github.com/uppin/tddy-coder/pull/283). (tddy-daemon, tddy-github, tddy-service, tddy-web)
