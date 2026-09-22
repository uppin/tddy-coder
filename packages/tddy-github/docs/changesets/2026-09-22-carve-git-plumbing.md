# 2026-09-22 — The crate gains the pull-request REST client

**Type:** Refactor · `#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-git-plumbing.md`](../../../../docs/dev/changesets/2026-09-22-carve-git-plumbing.md)

`github_rest_common` (338 lines), `github_pr` (494) and `pr_api` (1,292 — formerly
`tddy-workflow-recipes`' `orchestrate_pr_stack/github.rs`) arrive byte-identical from
`tddy-workflow-recipes`, which keeps facades at all three old paths. `crate::github_rest_common::…`
resolves unchanged once all three share this crate, so no path inside a moved body was rewritten.

New dependencies: `tddy-core`, because `pr_api`'s public traits return `tddy_core::WorkflowError`
in 16 signatures (it closes no cycle); `tempfile` moves from dev to production for `github_pr`'s
request staging; `serial_test` for its tests. **Never** `tddy-workflow-recipes`, which holds the
facades pointing here — `tests/git_plumbing_shape.rs` guards that.

Opened `oversized-file-pr-api` (≈ 915 real production lines behind a gate count of 315) and
`dead-code-github-pr-mock-transport`.
