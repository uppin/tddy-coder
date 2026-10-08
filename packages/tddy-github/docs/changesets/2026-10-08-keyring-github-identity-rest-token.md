# 2026-10-08 — REST entry points take the token; no environment read

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`github_token_from_env`, `github_env_token_present`, `TokenSource::ProcessEnv` and the env-reading
`curl_github_*_json` wrappers are deleted. `create_pull_request`, `update_pull_request`, their
`*_via_rest_api` forms and `merge_open_pr_for_branch` take `token: &str` and refuse a blank one.
`RealGithubPrApi` gains `with_api_base` (the REST root is a value on the client), `asking` (a supplier
asked the first time an operation needs a token; a refusal is not remembered) and
`asking_the_session_host`. The `curl_github_*_with_token` entry points take the API root first.

Detail: [rest-token.md](../rest-token.md).
