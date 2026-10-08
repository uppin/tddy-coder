# 2026-10-08 — The `github-token` verb on the session toolcall socket

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

A JSON verb `GithubToken` (`GITHUB_TOKEN_METHOD`) on the existing toolcall service answered by a per-instance
`GithubCredentialHandler`; `ToolCallResponse::GithubTokenOk` holds a `RedactedToken` and the `[send]` log
withholds it. The client half, `request_github_token` / `request_github_token_from_session`, lives in
`github_token_client.rs`. No proto change. `listener.rs` grew from 676 to 766 production lines (see the
code issue below).

Detail: [architecture.md](../architecture.md#the-github-token-verb).
