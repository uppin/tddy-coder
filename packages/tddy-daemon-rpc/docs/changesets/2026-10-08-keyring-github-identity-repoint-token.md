# 2026-10-08 — `RepointPlannedPr` resolves the project's account itself

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`project_github_token` runs one `acting_identity` over the caller's vault. Only a node that owns a branch needs
GitHub; a refusal is `FAILED_PRECONDITION` with the resolver's words, before the plan is rewritten.
`pr_stack/ports.rs` grew from 775 to 808 production lines.

Detail: [architecture.md](../architecture.md#repointplannedpr-and-the-github-token).
