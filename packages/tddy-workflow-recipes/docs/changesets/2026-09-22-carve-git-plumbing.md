# 2026-09-22 — The GitHub REST client leaves for `tddy-github`

**Type:** Refactor · `#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-git-plumbing.md`](../../../../docs/dev/changesets/2026-09-22-carve-git-plumbing.md)

`github_rest_common.rs`, `github_pr.rs` and `orchestrate_pr_stack/github.rs` — 2,124 lines — move to
[`tddy-github`](../../../tddy-github/README.md) and each leaves a one-line `pub use` facade, so
`tddy_workflow_recipes::github_pr::…`, `crate::orchestrate_pr_stack::github::…` and the crate-root
`github_rest_common` re-exports all resolve; no caller was edited. The crate gains a `tddy-github`
dependency; `tempfile` becomes dev-only, since only tests use it now.

Closed `squatting-github-rest-client` (2,124 → 9 lines). Opened `misplaced-tests-github-rest-client`:
three suites import only the facades.
