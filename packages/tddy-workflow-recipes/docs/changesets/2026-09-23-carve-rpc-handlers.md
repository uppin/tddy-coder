# 2026-09-23 — `PR_STACK_SERVICE` is re-exported from `tddy-pr-stack`

**Type:** Refactor

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

`PR_STACK_SERVICE` became `pub use tddy_pr_stack::PR_STACK_SERVICE`: the constant moved to
`tddy_pr_stack::rpc` with the entry builder that registers it, and `tddy-pr-stack` cannot depend on
this crate to name it here.
The path `tddy_workflow_recipes::PR_STACK_SERVICE` still resolves.
