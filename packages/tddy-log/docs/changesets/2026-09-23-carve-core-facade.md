# 2026-09-23 — The crate is created from `tddy-core`'s log sink

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `log_backend.rs` and `stdio_safety.rs`, moved with `git mv`. No workspace dependencies. `tddy-core` re-exports it whole. Its `stdio_safety` test binary moved with it. New code-issue record: `oversized-file-log-backend` (862 production lines, inherited, not grown). See [architecture.md](../architecture.md).
