# 2026-09-23 — The crate defines the PR-stack RPC family

**Type:** Architecture

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

Added the `rpc` module: `PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry` and
`PR_STACK_SERVICE` (also re-exported at the crate root), moved from `tddy-session-lifecycle`'s
`pr_stack_rpc.rs` and `tddy-workflow-recipes`, both of which re-export them. Defining the trait
here lets the lifecycle crate name it without depending on `tddy-daemon-rpc`, which implements it.
Added the `tddy-rpc` and `tddy-service` dependencies (`tddy-service` was already reachable through
`tddy-github`). `tddy-daemon-rpc`'s shape suite asserts the crate depends on neither
`tddy-session-lifecycle` nor `tddy-workflow-recipes`.

Production lines 2,916 → 3,077, under the 10,000-line cap.
