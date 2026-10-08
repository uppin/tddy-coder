# The REST client's token

A GitHub REST call carries a token that was **handed to it**. Nothing in `tddy-github` reads
`GITHUB_TOKEN` or `GH_TOKEN`, and no function that did exists: a deprecated environment read is still
an environment read, so none was left standing. Which token a call carries is decided by the caller —
for a session, by the account its project is assigned
([resolution](../../tddy-accounts/docs/github-identity-resolution.md)).

## Entry points

- `create_pull_request`, `update_pull_request`, their `*_via_rest_api` forms and
  `merge_open_pr_for_branch` take `token: &str` and refuse a blank one.
- `curl_github_{get,post,patch,put}_json_with_token` and the `…_absolute_path` forms take the API root
  as their first argument; their only callers are `RealGithubPrApi`.
- The token travels in the `Authorization: Bearer` header alone, never in the URL.

## `RealGithubPrApi`

| Constructor | Token |
|---|---|
| `with_token(repo, token)` | the one given, kept for the client's life |
| `asking(repo, supplier)` | a supplier asked the first time an operation needs a token |
| `asking_the_session_host(repo)` | `asking` over `tddy_toolcall::request_github_token_from_session()` — the `github-token` round trip to the session's host |

`asking` holds a *supplier*, not a token. A refusal is **not remembered**, so the next operation asks
again and assigning an account mid-run is not stuck behind an earlier refusal; a token the host did hand
over is kept for that client's life. The synchronous supplier runs the async request on a thread and
runtime of its own, so it is safe from an async task and from a runtime thread, and never depends on
the caller's runtime making progress while the caller waits.

`with_api_base(base)` sets the REST root as a value on the client; the default is
`https://api.github.com`. It is never an environment variable.

## Tests

- `src/pr_api.rs` `real_impl_tests`: the supplier is asked once an operation needs it, a refusal is not
  remembered, a handed-over token is kept.
- `tddy-workflow-recipes/tests/github_rest_call_carries_the_session_hosts_token.rs`: a fake session host
  on a real toolcall socket and a loopback listener standing in for GitHub. The request's
  `Authorization: Bearer` is the token the host returned; a refusing host means no HTTP request is made;
  a `GITHUB_TOKEN` in the environment authenticates no call. Proven for a GET; the PUT, POST and PATCH
  paths build their header with the same format string and have no request-level test of their own.
- `tddy-daemon-auth/tests/login_time_token_store_is_retired.rs`:
  `no_crate_still_resolves_a_github_token_from_the_process_environment`.
