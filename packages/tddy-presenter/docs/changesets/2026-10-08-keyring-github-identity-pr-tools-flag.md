# 2026-10-08 — The run context is seeded with `github_pr_tools_available`

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`Presenter::set_github_pr_tools_available` and the seeding in `workflow_runner.rs`; the key is
`tddy_workflow::context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY`. `workflow_runner.rs` grew from 1,015 to 1,036
production lines.

Detail: [architecture.md](../architecture.md#github-pr-tools-flag).
