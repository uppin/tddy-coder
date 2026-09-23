# 2026-09-23 — The crate is created from `tddy-core`'s tool-call protocol

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `toolcall/`, moved with `git mv`. `toolcall/mod.rs` names `tddy_workflow::ClarificationQuestion` directly, where it had gone through `crate::backend`. Workspace dependencies: `tddy-workflow`, `tddy-session-actions`, `tddy-rpc`, `tddy-stdio`. Four stdio-relay acceptance suites moved with it. New code-issue record: `oversized-file-listener` (671 production lines, inherited). `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
