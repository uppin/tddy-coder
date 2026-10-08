# 2026-10-08 — No reader of a login-keyed GitHub token, and no GitHub token from the environment

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

Test-only: `tests/login_time_token_store_is_retired.rs` pins that the session path, the authentication
service and the tree no longer reach for or declare a login-keyed GitHub token store, and
`no_crate_still_resolves_a_github_token_from_the_process_environment` pins that no crate resolves a
GitHub token from `GITHUB_TOKEN` / `GH_TOKEN`.

Detail: [auth-service.md](../auth-service.md#no-retained-github-token).
