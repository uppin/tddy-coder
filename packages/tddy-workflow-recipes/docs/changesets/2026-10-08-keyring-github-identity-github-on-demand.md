# 2026-10-08 — PR-stack tasks ask the session host for the token on demand

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`AssessTask`, `RepointTask` and `MergeTask` ask the host with the `github-token` round trip only when an action
that reaches GitHub runs; construction asks nothing. The prompt hooks read `github_pr_tools_available` from the
context instead of a hardcoded `false`. `merge_pr/github.rs` takes a plain `token: &str`.

Detail: [github-on-demand.md](../github-on-demand.md).
