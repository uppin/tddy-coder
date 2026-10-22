# 2026-10-22 — One branch policy for every daemon-cut CLI harness

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)).
`BranchIntentPolicy::opencode_cli()` became `BranchIntentPolicy::cli_harness(session_type)`: as
cursor-cli, a `new_branch_from_base` with no name generates one, prefixed by the harness's session
type (`opencode-cli/…`, `muse-cli/…`). Code issue `complexity-branch-intent-resolve-branch-workflow`
re-measured: 86 lines, unchanged.
