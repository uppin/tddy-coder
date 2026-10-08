# 2026-10-08 — The PR tools ask the session host for the project account's token per call

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`src/github_credential.rs` replaces the environment read: the PR tools and `real_gh` / `pr_search_impl` send
`github-token` over `TDDY_SOCKET`, and the host's refusal reaches the agent as `{"error": <message>}`.
`get_info` states the tools authenticate as the project's assigned account. `server.rs` grew by 23 production
lines.

Detail: [github-credential.md](../github-credential.md).
